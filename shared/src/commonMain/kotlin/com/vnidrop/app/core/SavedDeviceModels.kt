package com.vnidrop.app.core

/**
 * App-facing models for experimental saved devices and targeted transfers.
 * Maps UniFFI types; features must not depend on `uniffi.vnidrop` for these flows
 * except share sources / output sinks already used by invitation receive.
 */

data class SavedDeviceModel(
	val endpointId: String,
	val localLabel: String?,
	val remoteDisplayName: String?,
	val createdAt: Long,
	val lastAuthenticatedAt: Long?,
)

enum class DeviceRelationshipStateModel {
	PendingOutgoing,
	PendingIncoming,
	Saved,
	Revoked,
	Blocked,
}

data class DeviceRelationshipModel(
	val remoteEndpointId: String,
	val state: DeviceRelationshipStateModel,
	val generation: ULong,
	val minimumProtocolVersion: UShort,
	val createdAt: Long,
	val updatedAt: Long,
)

data class PairingEligibilityModel(
	val peerEndpointId: String,
	val sessionId: String,
	val protocolVersion: UShort,
	val createdAt: Long,
	val expiresAt: Long,
)

data class PendingTargetedOfferModel(
	val transferId: String,
	val senderEndpointId: String,
	val receiverEndpointId: String,
	val manifestId: String,
	val contentHash: String,
	val transferName: String,
	val fileCount: ULong,
	val totalSize: ULong,
	val protocolVersion: UShort,
	val receivedAt: Long,
)

enum class TargetedTransferStateModel {
	Preparing,
	Offering,
	AwaitingApproval,
	Approved,
	Connecting,
	Transferring,
	Interrupted,
	Completed,
	Declined,
	Cancelled,
	Failed,
	Deleted,
}

data class TargetedTransferModel(
	val id: String,
	val senderEndpointId: String,
	val receiverEndpointId: String,
	val manifestId: String,
	val fileCount: ULong,
	val totalSize: ULong,
	val verifiedBytes: ULong,
	val state: TargetedTransferStateModel,
	val createdAt: Long,
	val updatedAt: Long,
)

sealed interface TargetedOfferResponseModel {
	data class Approved(val transferId: String) : TargetedOfferResponseModel
	data object Declined : TargetedOfferResponseModel
	data class AlreadySettled(val transferId: String) : TargetedOfferResponseModel
}
