package com.vnidrop.app.notifications

import com.vnidrop.app.core.ReceiverDeliveryStatus
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.platform.AppVisibility
import com.vnidrop.app.preferences.AppPreferences
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.support.FakeNotificationService
import com.vnidrop.app.support.FakePreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalCoroutinesApi::class)
class TransferNotificationCoordinatorTest {
	@Test
	fun targetedOfferSignalPublishesNotificationWhileBackgrounded() = runTest {
		val core = FakeCoreGateway().apply {
			savedDevices = listOf(
				SavedDeviceModel(
					endpointId = "sender",
					localLabel = "Office PC",
					remoteDisplayName = "Linux",
					createdAt = 1L,
					lastAuthenticatedAt = 2L,
				),
			)
			pendingTargetedOffers = listOf(
				PendingTargetedOfferModel(
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
				),
			)
		}
		val notifications = FakeNotificationService()
		TransferNotificationCoordinator(
			repository = core,
			preferencesRepository = FakePreferencesRepository(
				AppPreferences(
					username = "Receiver",
					receiveFolder = ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp"),
					themeMode = ThemeMode.System,
					notificationsEnabled = true,
				),
			),
			notifications = notifications,
			visibility = AppVisibility(initiallyForeground = false),
			messages = UiMessageController(),
			scope = backgroundScope,
		)

		runCurrent()
		core.mutableSignals.emit(CoreSignal.TargetedTransferChanged)
		runCurrent()
		core.mutableSignals.emit(CoreSignal.TargetedTransferChanged)
		runCurrent()

		assertEquals(2, core.listPendingTargetedOffersCount)
		assertEquals(
			listOf(
				LocalNotification(
					id = "targeted-offer-targeted-1",
					title = "Incoming transfer",
					body = "Office PC wants to send you “Holiday photos”.",
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
