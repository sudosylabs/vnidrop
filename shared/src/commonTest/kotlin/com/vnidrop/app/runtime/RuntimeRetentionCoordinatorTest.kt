package com.vnidrop.app.runtime

import com.vnidrop.app.BackgroundRuntimeKeeper
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.TargetedTransferModel
import com.vnidrop.app.core.TargetedTransferRoleModel
import com.vnidrop.app.core.TargetedTransferStateModel
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
class RuntimeRetentionCoordinatorTest {
	@Test
	fun applicationLifetimeCoordinatorTracksInvitationWorkWithoutAComposableCollector() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
		}
		val keeper = RecordingRuntimeKeeper()
		val coordinator = RuntimeRetentionCoordinator(core, keeper, UiPlatform.Android, backgroundScope)
		runCurrent()

		core.mutableState.value = core.mutableState.value.copy(
			transfers = listOf(invitation(TransferStatus.Receiving)),
		)
		runCurrent()
		core.mutableState.value = core.mutableState.value.copy(
			transfers = listOf(invitation(TransferStatus.Sharing, TransferDirection.Send)),
		)
		runCurrent()
		core.mutableState.value = core.mutableState.value.copy(
			transfers = listOf(invitation(TransferStatus.Done)),
		)
		runCurrent()

		assertEquals(listOf(false, true, false), keeper.requiredCalls)
		coordinator.close()
		coordinator.close()
		assertEquals(false, keeper.requiredCalls.last())
		assertEquals(1, keeper.closeCount)
	}

	@Test
	fun targetedSignalRefreshesDurableStatusesAndTeardownReleasesRetention() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			targetedTransfers = listOf(targeted(TargetedTransferStateModel.AwaitingApproval))
		}
		val keeper = RecordingRuntimeKeeper()
		val coordinator = RuntimeRetentionCoordinator(core, keeper, UiPlatform.Android, backgroundScope)
		runCurrent()
		assertEquals(true, keeper.requiredCalls.last())

		core.targetedTransfers = listOf(targeted(TargetedTransferStateModel.Completed))
		core.mutableSignals.emit(CoreSignal.TargetedTransferChanged)
		runCurrent()
		assertEquals(false, keeper.requiredCalls.last())

		core.targetedTransfers = listOf(targeted(TargetedTransferStateModel.Transferring))
		core.mutableSignals.emit(CoreSignal.TargetedTransferChanged)
		runCurrent()
		assertEquals(true, keeper.requiredCalls.last())

		coordinator.close()
		assertEquals(1, keeper.closeCount)
	}

	@Test
	fun desktopDoesNotStartTheAndroidRetentionMapping() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(
				isInitialized = true,
				transfers = listOf(invitation(TransferStatus.Sharing, TransferDirection.Send)),
			)
			targetedTransfers = listOf(targeted(TargetedTransferStateModel.AwaitingApproval))
		}
		val keeper = RecordingRuntimeKeeper()
		val coordinator = RuntimeRetentionCoordinator(core, keeper, UiPlatform.Linux, backgroundScope)
		runCurrent()

		assertEquals(emptyList(), keeper.requiredCalls)
		coordinator.close()
		assertEquals(1, keeper.closeCount)
	}

	private class RecordingRuntimeKeeper : BackgroundRuntimeKeeper {
		val requiredCalls = mutableListOf<Boolean>()
		var closeCount = 0

		override fun setRequired(required: Boolean) {
			requiredCalls += required
		}

		override fun close() {
			closeCount += 1
			requiredCalls += false
		}
	}

	private fun invitation(
		status: TransferStatus,
		direction: TransferDirection = TransferDirection.Receive,
	) = Transfer(
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

	private fun targeted(state: TargetedTransferStateModel) = TargetedTransferModel(
		id = "targeted",
		role = TargetedTransferRoleModel.Sender,
		senderEndpointId = "sender",
		receiverEndpointId = "receiver",
		manifestId = "manifest",
		contentHash = "hash",
		transferName = "Photos",
		fileCount = 1UL,
		totalSize = 1UL,
		verifiedBytes = 0UL,
		state = state,
		createdAt = 1L,
		updatedAt = 1L,
	)
}
