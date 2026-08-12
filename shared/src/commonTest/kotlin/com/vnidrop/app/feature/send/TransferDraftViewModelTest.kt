package com.vnidrop.app.feature.send

import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.PickedShareFile
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.core.Share
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.TargetedTransferModel
import com.vnidrop.app.core.TargetedTransferStateModel
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.support.FakeFilePreviewRepository
import com.vnidrop.app.support.FakePickedShareSourceAdapter
import com.vnidrop.app.ui.feedback.UiMessageController
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class TransferDraftViewModelTest {
	@AfterTest
	fun tearDown() {
		Dispatchers.resetMain()
	}

	@Test
	fun invitationAndTargetedCreationShareOneDraftBehavior() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = readyCore().apply {
			shareResult = Result.success(Share(7UL, "ticket", "Holiday", "hash", 2UL, 3UL))
			createTargetedResult = Result.success(targeted("target-9", "peer-9"))
			savedDevices = listOf(device("peer-9", "Kitchen"))
		}
		val adapter = FakePickedShareSourceAdapter()
		val invitation = draft(core, adapter)
		invitation.openInvitation("Alice")
		invitation.chooseFiles()
		advanceUntilIdle()
		assertIs<TransferDraftEffect.OpenPicker>(invitation.effectFlow.first())
		val invitationRequest = invitation.state.value.pickerRequestId!!
		invitation.onFilesPicked(
			invitationRequest,
			listOf(file("/a", "a.txt"), file("/b", "b.txt")),
		)
		advanceUntilIdle()
		assertEquals("2 localized files", invitation.state.value.transferName)
		invitation.changeTransferName("Holiday")
		invitation.changeAccessPolicy(ShareAccessPolicy.AnyoneWithTransfer)
		invitation.submit()
		advanceUntilIdle()
		val invitationCreated = assertIs<TransferDraftEffect.Created>(invitation.effectFlow.first()).creation
		assertIs<TransferDraftCreation.Invitation>(invitationCreated)
		assertEquals(ShareAccessPolicy.AnyoneWithTransfer, core.lastShareAccessPolicy)
		assertFalse(invitation.state.value.isOpen)

		val targeted = draft(core, adapter)
		targeted.openTargeted(core.savedDevices.single(), "Saved device")
		targeted.chooseFolder()
		advanceUntilIdle()
		assertIs<TransferDraftEffect.OpenPicker>(targeted.effectFlow.first())
		val targetedRequest = targeted.state.value.pickerRequestId!!
		targeted.onFilesPicked(
			targetedRequest,
			listOf(file("/photos", "Photos", directory = true)),
		)
		advanceUntilIdle()
		assertEquals("Photos", targeted.state.value.transferName)
		assertEquals(
			"Kitchen",
			assertIs<TransferDraftDestination.Targeted>(targeted.state.value.destination).receiver.displayName,
		)
		targeted.submit()
		advanceUntilIdle()
		val created = assertIs<TransferDraftEffect.Created>(targeted.effectFlow.first()).creation
		assertEquals(TransferDraftCreation.Targeted("target-9", "peer-9"), created)
		assertTrue(core.createdTargetedTransfers.single().second.single().isDirectory)
	}

	@Test
	fun replacementAndDismissalReleaseOnlyOwnedCopiesExactlyOnce() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val adapter = FakePickedShareSourceAdapter()
		val viewModel = draft(readyCore(), adapter)
		val first = file("/owned-a", "a.txt", temporary = true)
		val second = file("/owned-b", "b.txt", temporary = true)
		viewModel.openInvitation("Alice")
		viewModel.chooseFiles()
		viewModel.onFilesPicked(viewModel.state.value.pickerRequestId!!, listOf(first))
		advanceUntilIdle()
		viewModel.chooseFiles()
		viewModel.onFilesPicked(viewModel.state.value.pickerRequestId!!, listOf(second))
		advanceUntilIdle()
		viewModel.dismiss()
		advanceUntilIdle()

		assertEquals(listOf(first, second), adapter.discardedPickedFiles)
	}

	@Test
	fun failedCreationPreservesEditableDraftAndOwnedSources() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = readyCore()
		val adapter = FakePickedShareSourceAdapter()
		val viewModel = draft(core, adapter)
		val selected = file("/owned", "report.pdf", temporary = true)
		viewModel.openInvitation("Alice")
		viewModel.chooseFiles()
		viewModel.onFilesPicked(viewModel.state.value.pickerRequestId!!, listOf(selected))
		advanceUntilIdle()
		viewModel.changeTransferName("Report")
		viewModel.submit()
		advanceUntilIdle()

		assertTrue(viewModel.state.value.isOpen)
		assertFalse(viewModel.state.value.isSubmitting)
		assertEquals("Report", viewModel.state.value.transferName)
		assertEquals(emptyList(), adapter.discardedPickedFiles)
	}

	@Test
	fun targetedSubmitRevalidatesReceiverWithoutFallingBackToInvitation() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = readyCore().apply { savedDevices = listOf(device("peer-3", "Desk")) }
		val viewModel = draft(core, FakePickedShareSourceAdapter())
		viewModel.openTargeted(core.savedDevices.single(), "Saved device")
		viewModel.chooseFiles()
		viewModel.onFilesPicked(viewModel.state.value.pickerRequestId!!, listOf(file("/a", "a.txt")))
		advanceUntilIdle()
		core.savedDevices = emptyList()
		viewModel.submit()
		advanceUntilIdle()

		assertTrue(viewModel.state.value.isOpen)
		assertIs<TransferDraftDestination.Targeted>(viewModel.state.value.destination)
		assertTrue(core.createdTargetedTransfers.isEmpty())
	}

	@Test
	fun stalePickerResultCannotReopenDismissedDraftAndIsReleased() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val adapter = FakePickedShareSourceAdapter()
		val viewModel = draft(readyCore(), adapter)
		val stale = file("/owned", "late.txt", temporary = true)
		viewModel.openInvitation("Alice")
		viewModel.chooseFiles()
		val requestId = viewModel.state.value.pickerRequestId!!
		viewModel.dismiss()
		viewModel.onFilesPicked(requestId, listOf(stale))
		advanceUntilIdle()

		assertFalse(viewModel.state.value.isOpen)
		assertEquals(listOf(stale), adapter.discardedPickedFiles)
	}

	@Test
	fun pickerCancellationPreservesTheExistingDraft() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val viewModel = draft(readyCore(), FakePickedShareSourceAdapter())
		viewModel.openInvitation("Alice")
		viewModel.chooseFiles()
		val firstRequest = viewModel.state.value.pickerRequestId!!
		viewModel.onFilesPicked(firstRequest, listOf(file("/a", "a.txt")))
		advanceUntilIdle()
		viewModel.chooseFiles()
		val cancelledRequest = viewModel.state.value.pickerRequestId!!
		viewModel.onFilesPicked(cancelledRequest, emptyList())

		assertEquals("a.txt", viewModel.state.value.transferName)
		assertEquals(listOf("a.txt"), viewModel.state.value.sources.map(TransferDraftSource::displayName))
		assertFalse(viewModel.state.value.isPicking)
	}

	@Test
	fun removingAFileKeepsAUserEditedTransferName() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val viewModel = draft(readyCore(), FakePickedShareSourceAdapter())
		viewModel.openInvitation("Alice")
		viewModel.chooseFiles()
		viewModel.onFilesPicked(
			viewModel.state.value.pickerRequestId!!,
			listOf(file("/a", "a.txt"), file("/b", "b.txt")),
		)
		advanceUntilIdle()
		viewModel.changeTransferName("My documents")
		viewModel.removeSource(viewModel.state.value.sources.first().id)
		advanceUntilIdle()

		assertEquals("My documents", viewModel.state.value.transferName)
		assertEquals(listOf("b.txt"), viewModel.state.value.sources.map(TransferDraftSource::displayName))
	}

	private fun draft(core: FakeCoreGateway, adapter: FakePickedShareSourceAdapter) =
		TransferDraftViewModel(
			repository = core,
			sourceAdapter = adapter,
			filePreviewRepository = FakeFilePreviewRepository(),
			messages = UiMessageController(),
			multipleFilesName = { "$it localized files" },
		)

	private fun readyCore() = FakeCoreGateway().apply {
		mutableState.value = CoreState(isInitialized = true)
	}

	private fun file(
		value: String,
		name: String,
		directory: Boolean = false,
		temporary: Boolean = false,
	) = PickedShareFile(
		value = value,
		displayName = name,
		sizeBytes = 1UL,
		isTemporaryCopy = temporary,
		isDirectory = directory,
	)

	private fun device(id: String, label: String?) = SavedDeviceModel(
		endpointId = id,
		localLabel = label,
		remoteDisplayName = "Remote",
		createdAt = 1L,
		lastAuthenticatedAt = 1L,
	)

	private fun targeted(id: String, peerId: String) = TargetedTransferModel(
		id = id,
		senderEndpointId = "me",
		receiverEndpointId = peerId,
		manifestId = "manifest",
		transferName = "Transfer",
		fileCount = 1UL,
		totalSize = 1UL,
		verifiedBytes = 0UL,
		state = TargetedTransferStateModel.Offering,
		createdAt = 1L,
		updatedAt = 1L,
	)
}
