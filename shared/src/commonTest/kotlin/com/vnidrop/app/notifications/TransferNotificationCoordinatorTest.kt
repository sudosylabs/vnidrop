package com.vnidrop.app.notifications

import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.ReceiverDeliveryStatus
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.TargetedTransferModel
import com.vnidrop.app.core.TargetedTransferRoleModel
import com.vnidrop.app.core.TargetedTransferStateModel
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.feature.saveddevices.SavedDevicesReadInputs
import com.vnidrop.app.feature.saveddevices.SavedDevicesReadModel
import com.vnidrop.app.platform.AppVisibility
import com.vnidrop.app.runtime.SavedDeviceListenIntent
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.support.FakeNotificationService
import com.vnidrop.app.ui.feedback.UiMessageController
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class TransferNotificationCoordinatorTest {
	@Test
	fun targetedOfferSignalPublishesNotificationWhileBackgrounded() = runTest {
		val core = coreWithOffer()
		val notifications = FakeNotificationService()
		coordinator(core, notifications)

		runCurrent()
		core.mutableSignals.emit(CoreSignal.TargetedTransferChanged)
		runCurrent()
		core.mutableSignals.emit(CoreSignal.TargetedTransferChanged)
		runCurrent()

		assertTrue(core.listPendingTargetedOffersCount >= 1)
		assertEquals(
			listOf(offerNotification()),
			notifications.published,
		)
	}

	@Test
	fun foregroundSkipDoesNotPermanentlySuppressALaterBackgroundOffer() = runTest {
		val core = coreWithOffer()
		val notifications = FakeNotificationService()
		val visibility = AppVisibility(initiallyForeground = true)
		coordinator(core, notifications, visibility)
		runCurrent()
		assertEquals(emptyList(), notifications.published)

		visibility.setForeground(false)
		runCurrent()
		assertEquals(listOf(offerNotification()), notifications.published)
	}

	@Test
	fun emptyOffersWithdrawPublishedPrompt() = runTest {
		val core = coreWithOffer()
		val notifications = FakeNotificationService()
		coordinator(core, notifications)
		runCurrent()
		assertEquals(listOf("targeted-offer-targeted-1"), notifications.published.map(LocalNotification::id))

		core.pendingTargetedOffers = emptyList()
		core.mutableSignals.emit(CoreSignal.TargetedTransferChanged)
		runCurrent()
		assertEquals(listOf("targeted-offer-targeted-1"), notifications.cancelled)
	}

	@Test
	fun plansPendingOfferAndIncomingPairingFromReadSnapshot() {
		val snapshot = SavedDevicesReadModel().derive(
			SavedDevicesReadInputs(
				eligibilities = listOf(
					PairingEligibilityModel(
						peerEndpointId = "peer",
						remoteDisplayName = "Alice's Mac",
						sessionId = "session",
						protocolVersion = 1U,
						createdAt = 1L,
						expiresAt = 2L,
					),
				),
				relationships = listOf(
					DeviceRelationshipModel(
						remoteEndpointId = "peer",
						state = DeviceRelationshipStateModel.PendingIncoming,
						generation = 1UL,
						minimumProtocolVersion = 1U,
						createdAt = 1L,
						updatedAt = 1L,
					),
					DeviceRelationshipModel(
						remoteEndpointId = "other",
						state = DeviceRelationshipStateModel.PendingOutgoing,
						generation = 1UL,
						minimumProtocolVersion = 1U,
						createdAt = 1L,
						updatedAt = 1L,
					),
				),
				savedDevices = listOf(savedDevice()),
				pendingOffers = listOf(offer()),
			),
		)

		assertEquals(
			listOf(
				PlannedSavedDeviceNotification(
					id = "pairing-request-peer",
					kind = SavedDeviceNotificationKind.PairingRequest,
					deviceName = "Alice's Mac",
					transferName = null,
				),
				PlannedSavedDeviceNotification(
					id = "targeted-offer-targeted-1",
					kind = SavedDeviceNotificationKind.TargetedOffer,
					deviceName = "Office PC",
					transferName = "Holiday photos",
				),
			),
			plannedSavedDevicePrompts(snapshot),
		)
	}

	@Test
	fun targetedOutcomesAreRoleAware() {
		val snapshot = SavedDevicesReadModel().derive(
			SavedDevicesReadInputs(
				targetedTransfers = listOf(
					targetedTransfer("in", TargetedTransferRoleModel.Receiver, TargetedTransferStateModel.Completed, 4),
					targetedTransfer("out", TargetedTransferRoleModel.Sender, TargetedTransferStateModel.Completed, 3),
					targetedTransfer("in-fail", TargetedTransferRoleModel.Receiver, TargetedTransferStateModel.Failed, 2),
					targetedTransfer("out-fail", TargetedTransferRoleModel.Sender, TargetedTransferStateModel.Failed, 1),
					targetedTransfer("cancelled", TargetedTransferRoleModel.Receiver, TargetedTransferStateModel.Cancelled, 5),
				),
			),
		)

		assertEquals(
			listOf(
				SavedDeviceNotificationKind.TargetedReceiveCompleted,
				SavedDeviceNotificationKind.TargetedSendCompleted,
				SavedDeviceNotificationKind.TargetedReceiveFailed,
				SavedDeviceNotificationKind.TargetedSendFailed,
			),
			plannedTargetedOutcomes(snapshot.targetedTransfers, published = emptySet()).map { it.kind },
		)
		assertEquals(
			listOf(
				"targeted-receive-completed-in",
				"targeted-send-completed-out",
				"targeted-receive-failed-in-fail",
				"targeted-send-failed-out-fail",
			),
			plannedTargetedOutcomes(snapshot.targetedTransfers, published = emptySet()).map { it.id },
		)
	}

	@Test
	fun newOutgoingCompletionUsesSenderWording() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
		}
		val notifications = FakeNotificationService()
		coordinator(core, notifications)
		runCurrent()

		core.targetedTransfers = listOf(
			targetedTransfer("send-1", TargetedTransferRoleModel.Sender, TargetedTransferStateModel.Completed, 1)
				.copy(transferName = "Holiday photos"),
		)
		core.mutableSignals.emit(CoreSignal.TargetedTransferChanged)
		runCurrent()

		assertEquals(
			listOf(
				LocalNotification(
					id = "targeted-send-completed-send-1",
					title = "Transfer delivered",
					body = "Nearby device received “Holiday photos”.",
				),
			),
			notifications.published,
		)
	}

	@Test
	fun plansOnlyNewTerminalTransferOutcomes() {
		val transfers = listOf(
			transfer(1UL, TransferDirection.Receive, TransferStatus.Done),
			transfer(2UL, TransferDirection.Receive, TransferStatus.Failed),
			transfer(3UL, TransferDirection.Send, TransferStatus.Failed),
			transfer(4UL, TransferDirection.Send, TransferStatus.Sharing),
		)

		assertEquals(
			listOf("receive-completed-1", "send-failed-3"),
			plannedTransferNotifications(transfers, setOf("receive-failed-2")).map { it.id },
		)
	}

	@Test
	fun plansCompletedAndFailedReceiverOutcomesOnce() {
		val requests = listOf(
			request("completed", ReceiverDeliveryStatus.Completed),
			request("failed", ReceiverDeliveryStatus.Failed),
			request("accepted", ReceiverDeliveryStatus.Accepted),
		)

		assertEquals(
			listOf("receiver-failed-failed"),
			plannedReceiverNotifications(requests, setOf("receiver-completed-completed")).map { it.id },
		)
	}

	private fun TestScope.coordinator(
		core: FakeCoreGateway,
		notifications: FakeNotificationService,
		visibility: AppVisibility = AppVisibility(initiallyForeground = false),
		listenIntent: MutableStateFlow<SavedDeviceListenIntent> = MutableStateFlow(
			SavedDeviceListenIntent(optedIn = true, permissionGranted = true),
		),
	) = TransferNotificationCoordinator(
		repository = core,
		listenIntent = listenIntent,
		notifications = notifications,
		visibility = visibility,
		messages = UiMessageController(),
		scope = backgroundScope,
		notificationText = FakeNotificationTextFormatter,
	)

	private fun coreWithOffer() = FakeCoreGateway().apply {
		mutableState.value = mutableState.value.copy(isInitialized = true)
		savedDevices = listOf(savedDevice())
		pendingTargetedOffers = listOf(offer())
	}

	private fun offerNotification() = LocalNotification(
		id = "targeted-offer-targeted-1",
		title = "Incoming transfer",
		body = "Office PC wants to send you “Holiday photos”.",
	)

	private fun savedDevice() = SavedDeviceModel(
		endpointId = "sender",
		localLabel = "Office PC",
		remoteDisplayName = "Linux",
		createdAt = 1L,
		lastAuthenticatedAt = 2L,
	)

	private fun offer() = PendingTargetedOfferModel(
		transferId = "targeted-1",
		senderEndpointId = "sender",
		receiverEndpointId = "receiver",
		manifestId = "manifest",
		contentHash = "hash",
		transferName = "Holiday photos",
		fileCount = 2UL,
		totalSize = 100UL,
		protocolVersion = 1U,
		receivedAt = 10L,
	)

	private fun targetedTransfer(
		id: String,
		role: TargetedTransferRoleModel,
		state: TargetedTransferStateModel,
		updatedAt: Long,
	) = TargetedTransferModel(
		id = id,
		role = role,
		senderEndpointId = if (role == TargetedTransferRoleModel.Sender) "local" else "peer",
		receiverEndpointId = if (role == TargetedTransferRoleModel.Sender) "peer" else "local",
		manifestId = "manifest-$id",
		transferName = id,
		fileCount = 1UL,
		totalSize = 10UL,
		verifiedBytes = 10UL,
		state = state,
		createdAt = 1L,
		updatedAt = updatedAt,
	)

	private fun transfer(id: ULong, direction: TransferDirection, status: TransferStatus) = Transfer(
		localId = "local-$id",
		transferId = id,
		direction = direction,
		status = status,
		peerId = null,
		transferName = "Transfer $id",
		contentHash = null,
		fileCount = 1UL,
		totalSize = 10UL,
		ticket = null,
		accessPolicy = ShareAccessPolicy.RequireApproval,
		createdAt = 1L,
		updatedAt = 1L,
	)

	private fun request(id: String, status: ReceiverDeliveryStatus) = ReceiverRequestModel(
		id = id,
		transferId = 1UL,
		remoteEndpointId = "peer-$id",
		transferName = "Transfer",
		receiverName = "Receiver",
		receiverDeviceName = null,
		appVersion = "1.0",
		status = status,
		reason = null,
		requestedAt = 1L,
		respondedAt = 2L,
		completedAt = 3L,
	)
}
