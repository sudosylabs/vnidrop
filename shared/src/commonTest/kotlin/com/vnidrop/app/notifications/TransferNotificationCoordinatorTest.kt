package com.vnidrop.app.notifications

import com.vnidrop.app.core.ReceiverDeliveryStatus
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import kotlin.test.Test
import kotlin.test.assertEquals

class TransferNotificationCoordinatorTest {
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
