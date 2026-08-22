package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.TargetedTransferStateModel

sealed interface PairingPrompt {
	data class Eligibility(val peerEndpointId: String, val remoteDisplayName: String?) : PairingPrompt

	data class IncomingRequest(val peerEndpointId: String, val remoteDisplayName: String?) : PairingPrompt
}

data class PairingPromptState(
	val prompt: PairingPrompt? = null,
	val busy: Boolean = false,
)

data class TargetedOfferState(
	val pending: List<PendingTargetedOfferModel> = emptyList(),
	val senderDisplayNames: Map<String, String> = emptyMap(),
	val respondingIds: Set<String> = emptySet(),
) {
	val current: PendingTargetedOfferModel?
		get() = pending.firstOrNull()

	val currentSenderDisplayName: String?
		get() = current?.senderEndpointId?.let(senderDisplayNames::get)
}

enum class SavedDeviceTransferDirection {
	Outgoing,
	Incoming,
}

enum class SavedDeviceTransferAction {
	Receive,
	Resume,
	Cancel,
	Delete,
}

data class SavedDeviceTransferItem(
	val id: String,
	val peerEndpointId: String,
	val peerDisplayName: String?,
	val direction: SavedDeviceTransferDirection,
	val transferName: String,
	val fileCount: ULong,
	val totalSize: ULong,
	val verifiedBytes: ULong,
	val state: TargetedTransferStateModel,
	val createdAt: Long,
	val updatedAt: Long,
	val availableActions: List<SavedDeviceTransferAction>,
	val progressFraction: Float?,
)

internal fun savedDeviceTransferActions(
	direction: SavedDeviceTransferDirection,
	state: TargetedTransferStateModel,
): List<SavedDeviceTransferAction> = buildList {
	if (direction == SavedDeviceTransferDirection.Incoming) {
		when (state) {
			TargetedTransferStateModel.Approved -> add(SavedDeviceTransferAction.Receive)
			TargetedTransferStateModel.Interrupted -> add(SavedDeviceTransferAction.Resume)
			else -> Unit
		}
	}
	when (state) {
		TargetedTransferStateModel.Preparing,
		TargetedTransferStateModel.Offering,
		TargetedTransferStateModel.AwaitingApproval,
		TargetedTransferStateModel.Approved,
		TargetedTransferStateModel.Connecting,
		TargetedTransferStateModel.Transferring,
		TargetedTransferStateModel.Interrupted -> add(SavedDeviceTransferAction.Cancel)
		TargetedTransferStateModel.Completed,
		TargetedTransferStateModel.Declined,
		TargetedTransferStateModel.Cancelled,
		TargetedTransferStateModel.Failed -> add(SavedDeviceTransferAction.Delete)
		TargetedTransferStateModel.Deleted -> Unit
	}
}

internal fun savedDeviceTransferProgress(
	direction: SavedDeviceTransferDirection,
	state: TargetedTransferStateModel,
	verifiedBytes: ULong,
	totalSize: ULong,
): Float? {
	if (direction != SavedDeviceTransferDirection.Incoming || totalSize == 0UL) return null
	if (state !in progressStates) return null
	return (verifiedBytes.toDouble() / totalSize.toDouble()).coerceIn(0.0, 1.0).toFloat()
}

private val progressStates = setOf(
	TargetedTransferStateModel.Connecting,
	TargetedTransferStateModel.Transferring,
	TargetedTransferStateModel.Interrupted,
)
