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
)
