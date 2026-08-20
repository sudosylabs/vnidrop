package com.vnidrop.app.runtime

import com.vnidrop.app.core.TargetedTransferModel
import com.vnidrop.app.core.TargetedTransferRoleModel
import com.vnidrop.app.core.TargetedTransferStateModel
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus

internal fun hasRuntimeObligation(
	invitationTransfers: List<Transfer>,
	targetedTransfers: List<TargetedTransferModel>,
): Boolean = invitationTransfers.any(Transfer::requiresRuntime) || targetedTransfers.any(TargetedTransferModel::requiresRuntime)

private fun Transfer.requiresRuntime(): Boolean = when (direction) {
	TransferDirection.Send -> status == TransferStatus.Importing || status == TransferStatus.Sharing
	TransferDirection.Receive -> status == TransferStatus.Receiving
}

private fun TargetedTransferModel.requiresRuntime(): Boolean = when (role) {
	TargetedTransferRoleModel.Sender -> state in SenderRuntimeStates
	TargetedTransferRoleModel.Receiver -> state in ReceiverRuntimeStates
}

private val SenderRuntimeStates = setOf(
	TargetedTransferStateModel.Preparing,
	TargetedTransferStateModel.Offering,
	TargetedTransferStateModel.AwaitingApproval,
	TargetedTransferStateModel.Approved,
	TargetedTransferStateModel.Connecting,
	TargetedTransferStateModel.Transferring,
	TargetedTransferStateModel.Interrupted,
)

private val ReceiverRuntimeStates = setOf(
	TargetedTransferStateModel.Approved,
	TargetedTransferStateModel.Connecting,
	TargetedTransferStateModel.Transferring,
)
