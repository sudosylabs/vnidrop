package com.vnidrop.app.runtime

import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.TargetedTransferModel
import com.vnidrop.app.core.TargetedTransferRoleModel
import com.vnidrop.app.core.TargetedTransferStateModel
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class RuntimeObligationPolicyTest {
	@Test
	fun activeInvitationWorkCreatesAnObligation() {
		assertTrue(hasRuntimeObligation(listOf(invitation(TransferDirection.Send, TransferStatus.Importing)), emptyList()))
		assertTrue(hasRuntimeObligation(listOf(invitation(TransferDirection.Send, TransferStatus.Sharing)), emptyList()))
		assertTrue(hasRuntimeObligation(listOf(invitation(TransferDirection.Receive, TransferStatus.Receiving)), emptyList()))
	}

	@Test
	fun invitationHistoryAndImpossibleDirectionStatusPairsCreateNoObligation() {
		val inactiveStatuses = listOf(
			TransferStatus.Done,
			TransferStatus.Failed,
			TransferStatus.Cancelled,
			TransferStatus.Stopped,
		)
		inactiveStatuses.forEach { status ->
			assertFalse(hasRuntimeObligation(listOf(invitation(TransferDirection.Send, status)), emptyList()))
			assertFalse(hasRuntimeObligation(listOf(invitation(TransferDirection.Receive, status)), emptyList()))
		}
		assertFalse(hasRuntimeObligation(listOf(invitation(TransferDirection.Receive, TransferStatus.Importing)), emptyList()))
		assertFalse(hasRuntimeObligation(listOf(invitation(TransferDirection.Receive, TransferStatus.Sharing)), emptyList()))
		assertFalse(hasRuntimeObligation(listOf(invitation(TransferDirection.Send, TransferStatus.Receiving)), emptyList()))
	}

	@Test
	fun outgoingTargetedAvailabilityCreatesAnObligationUntilItBecomesTerminal() {
		val activeStates = listOf(
			TargetedTransferStateModel.Preparing,
			TargetedTransferStateModel.Offering,
			TargetedTransferStateModel.AwaitingApproval,
			TargetedTransferStateModel.Approved,
			TargetedTransferStateModel.Connecting,
			TargetedTransferStateModel.Transferring,
			TargetedTransferStateModel.Interrupted,
		)
		activeStates.forEach { state ->
			assertTrue(hasRuntimeObligation(emptyList(), listOf(targeted(TargetedTransferRoleModel.Sender, state))), state.name)
		}

		val terminalStates = listOf(
			TargetedTransferStateModel.Completed,
			TargetedTransferStateModel.Declined,
			TargetedTransferStateModel.Cancelled,
			TargetedTransferStateModel.Failed,
			TargetedTransferStateModel.Deleted,
		)
		terminalStates.forEach { state ->
			assertFalse(hasRuntimeObligation(emptyList(), listOf(targeted(TargetedTransferRoleModel.Sender, state))), state.name)
		}
	}

	@Test
	fun incomingTargetedReceiveCreatesAnObligationOnlyForActiveReceiveStates() {
		val activeStates = listOf(
			TargetedTransferStateModel.Approved,
			TargetedTransferStateModel.Connecting,
			TargetedTransferStateModel.Transferring,
		)
		activeStates.forEach { state ->
			assertTrue(hasRuntimeObligation(emptyList(), listOf(targeted(TargetedTransferRoleModel.Receiver, state))), state.name)
		}

		(TargetedTransferStateModel.entries - activeStates.toSet()).forEach { state ->
			assertFalse(hasRuntimeObligation(emptyList(), listOf(targeted(TargetedTransferRoleModel.Receiver, state))), state.name)
		}
	}

	private fun invitation(direction: TransferDirection, status: TransferStatus) = Transfer(
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

	private fun targeted(role: TargetedTransferRoleModel, state: TargetedTransferStateModel) = TargetedTransferModel(
		id = "targeted",
		role = role,
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
