package com.vnidrop.app.feature.approvals

import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.ReceiverDeliveryStatus
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.platform.AppVisibility
import com.vnidrop.app.preferences.AppPreferences
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.support.FakeNotificationService
import com.vnidrop.app.support.FakePreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class ApprovalCoordinatorTest {
	@Test
	fun ordersRequestsAndPublishesEachNotificationOnce() = runTest {
		val core = FakeCoreGateway()
		core.requests[1UL] = listOf(request("new", 20), request("old", 10))
		core.mutableState.value = CoreState(isInitialized = true, transfers = listOf(activeTransfer()))
		val notifications = FakeNotificationService()
		val visibility = AppVisibility(initiallyForeground = false)
		val coordinator = ApprovalCoordinator(core, preferences(enabled = true), notifications, visibility, UiMessageController(), backgroundScope)

		runCurrent()
		core.mutableSignals.emit(CoreSignal.ApprovalChanged(1UL))
		advanceUntilIdle()
		assertEquals(listOf("old", "new"), coordinator.state.value.pending.map(PendingApproval::id))
		assertEquals(2, notifications.published.size)

		core.mutableSignals.emit(CoreSignal.ApprovalChanged(1UL))
		advanceUntilIdle()
		assertEquals(2, notifications.published.size)

		visibility.setForeground(true)
		runCurrent()
		advanceUntilIdle()
		assertEquals(setOf("approval-old", "approval-new"), notifications.cancelled.toSet())
		assertEquals(0, notifications.cancelAllCount)

		core.requests[1UL] = emptyList()
		core.mutableSignals.emit(CoreSignal.ApprovalChanged(1UL))
		runCurrent()
		advanceUntilIdle()
		assertTrue(coordinator.state.value.pending.isEmpty())
		assertTrue(notifications.cancelled.containsAll(listOf("approval-old", "approval-new")))
	}

	@Test
	fun failedResponseKeepsRequestVisible() = runTest {
		val core = FakeCoreGateway().apply {
			requests[1UL] = listOf(request("request", 10))
			mutableState.value = CoreState(isInitialized = true, transfers = listOf(activeTransfer()))
			responseResult = Result.failure(IllegalStateException("database unavailable"))
		}
		val coordinator = ApprovalCoordinator(
			core,
			preferences(enabled = false),
			FakeNotificationService(),
			AppVisibility(),
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		core.mutableSignals.emit(CoreSignal.ApprovalChanged(1UL))
		advanceUntilIdle()
		coordinator.accept("request")
		runCurrent()
		advanceUntilIdle()
		assertTrue(coordinator.state.value.pending.any { it.id == "request" })
		assertTrue(coordinator.state.value.respondingIds.isEmpty())
	}

	private fun request(id: String, requestedAt: Long) = ReceiverRequestModel(
		id = id,
		transferId = 1UL,
		remoteEndpointId = "endpoint",
		transferName = "Photos",
		receiverName = "Peer",
		receiverDeviceName = "Phone",
		appVersion = "1.0",
		status = ReceiverDeliveryStatus.Requested,
		reason = null,
		requestedAt = requestedAt,
		respondedAt = null,
		completedAt = null,
	)

	private fun activeTransfer() = Transfer(
		localId = "local",
		transferId = 1UL,
		direction = com.vnidrop.app.core.TransferDirection.Send,
		status = com.vnidrop.app.core.TransferStatus.Sharing,
		peerId = null,
		transferName = "Photos",
		contentHash = "hash",
		fileCount = 1UL,
		totalSize = 1UL,
		ticket = null,
		accessPolicy = com.vnidrop.app.core.ShareAccessPolicy.RequireApproval,
		createdAt = 1L,
		updatedAt = 1L,
	)

	private fun preferences(enabled: Boolean) = FakePreferencesRepository(
		AppPreferences(
			username = "Sender",
			receiveFolder = ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp"),
			themeMode = ThemeMode.System,
			notificationsEnabled = enabled,
		),
	)
}
