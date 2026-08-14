package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.CoreStatus
import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.core.TargetedOfferResponseModel
import com.vnidrop.app.core.TargetedTransferModel
import com.vnidrop.app.core.TargetedTransferRoleModel
import com.vnidrop.app.core.TargetedTransferStateModel
import com.vnidrop.app.preferences.AppPreferences
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.support.FakeFileSystemService
import com.vnidrop.app.support.FakePreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import uniffi.vnidrop.ReceiveOutputSinkV2
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class SavedDevicesViewModelTest {
	@AfterTest
	fun tearDown() {
		Dispatchers.resetMain()
	}

	@Test
	fun initializationLoadsOneSavedDeviceExperienceSnapshot() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			pairingEligibilities = listOf(eligibility("eligible", "Phone"))
			deviceRelationships = listOf(incoming("incoming"))
			savedDevices = listOf(device("peer", "Office PC", "Authenticated PC"))
			pendingTargetedOffers = listOf(offer("offer", "peer"))
			targetedTransfers = listOf(
				transfer(
					"history",
					TargetedTransferRoleModel.Sender,
					"local",
					"peer",
					TargetedTransferStateModel.AwaitingApproval,
				),
			)
		}
		val viewModel = createViewModel(core)
		runCurrent()
		advanceUntilIdle()
		assertEquals(0, core.listSavedDevicesCount)

		core.mutableState.value = core.mutableState.value.copy(
			isInitialized = true,
			status = CoreStatus("local", 0u, 0u),
		)
		runCurrent()
		advanceUntilIdle()

		val state = viewModel.state.value
		assertEquals("peer", state.savedDevices.single().endpointId)
		assertEquals("offer", state.targetedOffers.current?.transferId)
		assertEquals("Office PC", state.targetedOffers.currentSenderDisplayName)
		assertEquals(SavedDeviceTransferDirection.Outgoing, state.targetedTransfers.single().direction)
		assertEquals("Office PC", state.targetedTransfers.single().peerDisplayName)
		assertIs<PairingPrompt.IncomingRequest>(state.pairingPrompt.prompt)
		assertEquals(false, state.isLoading)
		assertEquals(false, state.loadFailed)
	}

	@Test
	fun persistedOutgoingTransferKeepsDirectionAfterIdentityReset() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = initializedCore().apply {
			targetedTransfers = listOf(
				transfer(
					id = "past-send",
					role = TargetedTransferRoleModel.Sender,
					sender = "retired-local-identity",
					receiver = "peer",
					state = TargetedTransferStateModel.Completed,
				),
			)
		}
		val viewModel = createViewModel(core)
		runCurrent()
		advanceUntilIdle()

		val transfer = viewModel.state.value.targetedTransfers.single()
		assertEquals(SavedDeviceTransferDirection.Outgoing, transfer.direction)
		assertEquals("peer", transfer.peerEndpointId)
	}

	@Test
	fun pairingPromptCommandsAndDismissalUseTheSameDurableLists() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = initializedCore().apply {
			pairingEligibilities = listOf(eligibility("peer-a", "Remote device"))
		}
		val viewModel = createViewModel(core)
		runCurrent()
		advanceUntilIdle()
		assertEquals(PairingPrompt.Eligibility("peer-a", "Remote device"), viewModel.state.value.pairingPrompt.prompt)

		viewModel.dismissPairingPrompt()
		runCurrent()
		assertNull(viewModel.state.value.pairingPrompt.prompt)
		assertEquals("peer-a", viewModel.state.value.eligibilities.single().peerEndpointId)

		viewModel.rememberEligible("peer-a")
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("peer-a"), core.requestedPairings)

		core.deviceRelationships = listOf(incoming("peer-b"))
		core.pairingEligibilities = emptyList()
		core.mutableSignals.emit(com.vnidrop.app.core.CoreSignal.PairingChanged)
		runCurrent()
		advanceUntilIdle()
		viewModel.acceptPairingPrompt()
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("peer-b" to true), core.pairingResponses)
	}

	@Test
	fun acceptingOfferPullsByIdThroughThePlatformSink() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val sink = unusedSink()
		val core = initializedCore().apply {
			pendingTargetedOffers = listOf(offer("transfer-sink", "sender"))
			respondTargetedResult = Result.success(TargetedOfferResponseModel.Approved("transfer-sink"))
		}
		val viewModel = createViewModel(
			core,
			FakeFileSystemService(receiveFolder(), receiveOutputSink = sink),
		)
		runCurrent()
		advanceUntilIdle()

		viewModel.acceptTargetedOffer("transfer-sink")
		runCurrent()
		advanceUntilIdle()

		assertEquals(listOf("transfer-sink" to true), core.respondedTargetedOffers)
		assertEquals(listOf("transfer-sink"), core.receivedTargetedViaSinkIds)
		assertTrue(core.receivedTargetedPathDirs.isEmpty())
		assertNull(viewModel.state.value.targetedOffers.current)
	}

	@Test
	fun decliningEligibilityAndOfferConsumeOnlyTheirOwnPendingItems() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = initializedCore().apply {
			pairingEligibilities = listOf(eligibility("pairing", "Phone"))
			pendingTargetedOffers = listOf(offer("offer", "sender"))
			respondTargetedResult = Result.success(TargetedOfferResponseModel.Declined)
		}
		val viewModel = createViewModel(core)
		runCurrent()
		advanceUntilIdle()

		viewModel.declinePairingPrompt()
		viewModel.declineTargetedOffer("offer")
		runCurrent()
		advanceUntilIdle()

		assertTrue(core.pairingEligibilities.isEmpty())
		assertEquals(listOf("offer" to false), core.respondedTargetedOffers)
		assertTrue(core.receivedTargetedTransferIds.isEmpty())
		assertNull(viewModel.state.value.targetedOffers.current)
	}

	@Test
	fun interruptedTransferResumeUsesThePlatformSinkWhenAvailable() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = initializedCore().apply {
			targetedTransfers = listOf(
				transfer(
					"resume-sink",
					TargetedTransferRoleModel.Receiver,
					"peer",
					"local",
					TargetedTransferStateModel.Interrupted,
				),
			)
		}
		val viewModel = createViewModel(
			core,
			FakeFileSystemService(receiveFolder(), receiveOutputSink = unusedSink()),
		)
		runCurrent()
		advanceUntilIdle()

		viewModel.resumeTargetedTransfer("resume-sink")
		runCurrent()
		advanceUntilIdle()

		assertEquals(listOf("resume-sink"), core.resumedTargetedViaSinkIds)
		assertTrue(core.resumedTargetedPathDirs.isEmpty())
	}

	@Test
	fun transferHistoryResumeCancelAndDeleteStayTransferIdScoped() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = initializedCore().apply {
			targetedTransfers = listOf(
				transfer("resume", TargetedTransferRoleModel.Receiver, "peer", "local", TargetedTransferStateModel.Interrupted),
				transfer("cancel", TargetedTransferRoleModel.Sender, "local", "peer", TargetedTransferStateModel.AwaitingApproval),
				transfer("delete", TargetedTransferRoleModel.Receiver, "peer", "local", TargetedTransferStateModel.Completed),
			)
		}
		val viewModel = createViewModel(core)
		runCurrent()
		advanceUntilIdle()

		viewModel.resumeTargetedTransfer("resume")
		viewModel.cancelTargetedTransfer("cancel")
		viewModel.deleteTargetedTransfer("delete")
		runCurrent()
		advanceUntilIdle()

		assertEquals(listOf("resume"), core.resumedTargetedTransferIds)
		assertEquals(listOf("resume" to "/tmp"), core.resumedTargetedPathDirs)
		assertEquals(listOf("cancel"), core.cancelledTargetedTransferIds)
		assertEquals(listOf("delete"), core.deletedTargetedTransferIds)
		assertTrue(viewModel.state.value.targetedTransfers.none { it.id == "delete" })
	}

	@Test
	fun labelForgetAndBlockUpdateGateway() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = initializedCore().apply { savedDevices = listOf(device("peer-1", null, null)) }
		val viewModel = createViewModel(core)
		runCurrent()
		advanceUntilIdle()

		viewModel.openLabelEditor("peer-1")
		viewModel.setLabelDraft("Kitchen")
		viewModel.saveLabel()
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf<Pair<String, String?>>("peer-1" to "Kitchen"), core.labeledDevices)

		viewModel.forget("peer-1")
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("peer-1"), core.forgottenDevices)

		core.savedDevices = listOf(device("peer-2", "Desk", null))
		viewModel.block("peer-2")
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("peer-2"), core.blockedPeers)
	}

	@Test
	fun failedLabelSaveKeepsTheEditorAndDraftAvailableForRetry() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = initializedCore().apply {
			savedDevices = listOf(device("peer-1", null, "Kitchen tablet"))
			setSavedDeviceLabelResult = Result.failure(IllegalStateException("save failed"))
		}
		val viewModel = createViewModel(core)
		runCurrent()
		advanceUntilIdle()

		viewModel.openLabelEditor("peer-1")
		viewModel.setLabelDraft("Kitchen")
		viewModel.saveLabel()
		runCurrent()
		advanceUntilIdle()

		assertEquals("peer-1", viewModel.state.value.labelingPeerId)
		assertEquals("Kitchen", viewModel.state.value.labelDraft)
	}

	@Test
	fun labelEditorCannotDismissOrLoseItsDraftWhileSaveIsInFlight() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val releaseSave = CompletableDeferred<Unit>()
		val core = initializedCore().apply {
			savedDevices = listOf(device("peer-1", null, "Kitchen tablet"))
			beforeSetSavedDeviceLabel = { releaseSave.await() }
		}
		val viewModel = createViewModel(core)
		runCurrent()
		advanceUntilIdle()

		viewModel.openLabelEditor("peer-1")
		viewModel.setLabelDraft("Kitchen")
		viewModel.saveLabel()
		runCurrent()
		viewModel.dismissLabelEditor()

		assertTrue(viewModel.state.value.isSavingLabel)
		assertEquals("peer-1", viewModel.state.value.labelingPeerId)
		assertEquals("Kitchen", viewModel.state.value.labelDraft)

		releaseSave.complete(Unit)
		advanceUntilIdle()
		assertEquals(false, viewModel.state.value.isSavingLabel)
		assertNull(viewModel.state.value.labelingPeerId)
	}

	private fun createViewModel(
		core: FakeCoreGateway,
		fileSystemService: FakeFileSystemService = FakeFileSystemService(receiveFolder()),
	) = SavedDevicesViewModel(
		repository = core,
		fileSystemService = fileSystemService,
		preferencesRepository = FakePreferencesRepository(preferences()),
		messages = UiMessageController(),
	)

	private fun initializedCore() = FakeCoreGateway().apply {
		mutableState.value = mutableState.value.copy(
			isInitialized = true,
			status = CoreStatus("local", 0u, 0u),
		)
	}

	private fun eligibility(peer: String, name: String) = PairingEligibilityModel(
		peerEndpointId = peer,
		remoteDisplayName = name,
		sessionId = "session",
		protocolVersion = 1u,
		createdAt = 1L,
		expiresAt = 2L,
	)

	private fun incoming(peer: String) = DeviceRelationshipModel(
		remoteEndpointId = peer,
		state = DeviceRelationshipStateModel.PendingIncoming,
		generation = 1u,
		minimumProtocolVersion = 1u,
		createdAt = 1L,
		updatedAt = 2L,
	)

	private fun device(id: String, label: String?, remoteName: String?) = SavedDeviceModel(
		endpointId = id,
		localLabel = label,
		remoteDisplayName = remoteName,
		createdAt = 1L,
		lastAuthenticatedAt = 2L,
	)

	private fun offer(id: String, sender: String) = PendingTargetedOfferModel(
		transferId = id,
		senderEndpointId = sender,
		receiverEndpointId = "local",
		manifestId = "manifest",
		contentHash = "hash",
		transferName = "Photos",
		fileCount = 1u,
		totalSize = 10u,
		protocolVersion = 1u,
		receivedAt = 1L,
	)

	private fun transfer(
		id: String,
		role: TargetedTransferRoleModel,
		sender: String,
		receiver: String,
		state: TargetedTransferStateModel,
	) = TargetedTransferModel(
		id = id,
		role = role,
		senderEndpointId = sender,
		receiverEndpointId = receiver,
		manifestId = "manifest-$id",
		transferName = "Transfer $id",
		fileCount = 2u,
		totalSize = 100u,
		verifiedBytes = if (state == TargetedTransferStateModel.Completed) 100u else 40u,
		state = state,
		createdAt = 1L,
		updatedAt = 2L,
	)

	private fun receiveFolder() = ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp")

	private fun preferences() = AppPreferences(
		username = "User",
		receiveFolder = receiveFolder(),
		themeMode = ThemeMode.System,
		notificationsEnabled = false,
	)

	private fun unusedSink() = object : ReceiveOutputSinkV2 {
		override fun startFile(relativePath: String) = error("unused")
		override fun writeChunk(relativePath: String, bytes: ByteArray) = error("unused")
		override fun finishFile(relativePath: String) = error("unused")
		override fun abortFile(relativePath: String, reason: String) = error("unused")
	}
}
