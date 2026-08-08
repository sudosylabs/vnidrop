package com.vnidrop.app.background

import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.support.FakeCoreGateway
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalCoroutinesApi::class)
class BackgroundSharingCoordinatorTest {
	@Test
	fun keepsThePlatformActiveOnlyWhileAnOutgoingShareIsAvailable() = runTest {
		val core = FakeCoreGateway()
		val activeStates = mutableListOf<Boolean>()
		val coordinator = BackgroundSharingCoordinator(
			repository = core,
			controller = BackgroundSharingController(activeStates::add),
			scope = backgroundScope,
		)

		runCurrent()
		core.mutableState.value = CoreState(isInitialized = true, transfers = listOf(transfer(TransferDirection.Receive, TransferStatus.Receiving)))
		runCurrent()
		assertEquals(listOf(false), activeStates)

		core.mutableState.value = CoreState(isInitialized = true, transfers = listOf(transfer(TransferDirection.Send, TransferStatus.Importing)))
		runCurrent()
		assertEquals(listOf(false, true), activeStates)

		core.mutableState.value = CoreState(isInitialized = true, transfers = listOf(transfer(TransferDirection.Send, TransferStatus.Sharing)))
		runCurrent()
		assertEquals(listOf(false, true), activeStates)

		core.mutableState.value = CoreState(isInitialized = true, transfers = listOf(transfer(TransferDirection.Send, TransferStatus.Stopped)))
		runCurrent()
		assertEquals(listOf(false, true, false), activeStates)

		coordinator.stop()
		assertEquals(listOf(false, true, false, false), activeStates)
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
		ticket = "ticket",
		accessPolicy = ShareAccessPolicy.RequireApproval,
		createdAt = 1L,
		updatedAt = 1L,
	)
}
