package com.vnidrop.app

import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class BackgroundRuntimePolicyTest {
	@Test
	fun androidKeepsActiveShareAliveAcrossForegroundTransitions() {
		val sharing = transfer(TransferDirection.Send, TransferStatus.Sharing)

		assertTrue(shouldKeepRuntimeActive(UiPlatform.Android, listOf(sharing)))
		assertFalse(shouldKeepRuntimeActive(UiPlatform.Linux, listOf(sharing)))
		assertFalse(
			shouldKeepRuntimeActive(
				UiPlatform.Android,
				listOf(transfer(TransferDirection.Receive, TransferStatus.Receiving)),
			),
		)
	}

	private fun transfer(direction: TransferDirection, status: TransferStatus) = Transfer(
		localId = "local",
		transferId = 1UL,
		direction = direction,
		status = status,
		peerId = null,
		transferName = "Photos",
		contentHash = "hash",
		fileCount = 1UL,
		totalSize = 1UL,
		ticket = null,
		accessPolicy = ShareAccessPolicy.RequireApproval,
		createdAt = 1L,
		updatedAt = 1L,
	)
}
