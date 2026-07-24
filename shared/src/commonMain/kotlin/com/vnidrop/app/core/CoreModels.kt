package com.vnidrop.app.core

import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import uniffi.vnidrop.ReceiveOutputSink
import uniffi.vnidrop.ReceiveOutputSinkV2

data class CoreStatus(
	val endpointId: String,
	val activeTransfers: ULong,
	val activeShares: ULong,
)

data class CoreEventModel(
	val id: String,
	val timestamp: Long,
	val scope: String,
	val transferId: ULong?,
	val direction: String?,
	val phase: String,
	val kind: String,
	val dataJson: String,
)

enum class ShareAccessPolicy {
	RequireApproval,
	AnyoneWithTransfer,
}

enum class RelayMode {
	Automatic,
	StrictCustom,
	CustomWithDirectFallback,
	LocalOnly,
}

data class RelaySettings(
	val mode: RelayMode = RelayMode.Automatic,
	val relayUrls: List<String> = emptyList(),
)

val RelayMode.usesCustomRelayUrls: Boolean
	get() = this == RelayMode.StrictCustom || this == RelayMode.CustomWithDirectFallback

enum class TransferDirection {
	Send,
	Receive,
}

enum class TransferStatus {
	Importing,
	Sharing,
	Receiving,
	Done,
	Failed,
	Cancelled,
	Stopped,
}

data class Transfer(
	val localId: String,
	val transferId: ULong,
	val direction: TransferDirection,
	val status: TransferStatus,
	val peerId: String?,
	val transferName: String?,
	val contentHash: String?,
	val fileCount: ULong,
	val totalSize: ULong,
	val ticket: String?,
	val accessPolicy: ShareAccessPolicy,
	val createdAt: Long,
	val updatedAt: Long,
)

data class Share(
	val transferId: ULong,
	val ticket: String,
	val transferName: String,
	val contentHash: String,
	val fileCount: ULong,
	val totalSize: ULong,
)

data class TransferMetadataModel(
	val transferId: ULong,
	val transferName: String,
	val senderName: String?,
	val contentHash: String,
	val fileCount: ULong,
	val totalSize: ULong,
)

data class TicketInspectionModel(
	val kind: String,
	val metadata: TransferMetadataModel,
)

data class ReceiverRequestModel(
	val id: String,
	val transferId: ULong,
	val remoteEndpointId: String,
	val transferName: String,
	val receiverName: String?,
	val receiverDeviceName: String?,
	val appVersion: String,
	val status: ReceiverDeliveryStatus,
	val reason: String?,
	val requestedAt: Long,
	val respondedAt: Long?,
	val completedAt: Long?,
)

enum class ReceiverDeliveryStatus {
	Requested,
	Accepted,
	Refused,
	Expired,
	Completed,
	Failed,
	Unknown,
}

data class CoreState(
	val isInitialized: Boolean = false,
	val status: CoreStatus? = null,
	val events: List<CoreEventModel> = emptyList(),
	val transfers: List<Transfer> = emptyList(),
	val lastShare: Share? = null,
	val lastInspection: TicketInspectionModel? = null,
)

data class CoreStorageUsageModel(
	val blobStoreBytes: ULong,
	val databaseBytes: ULong,
	val logsBytes: ULong,
	val previewsBytes: ULong,
	val otherCoreBytes: ULong,
) {
	val appDataBytes: ULong get() = databaseBytes + logsBytes + previewsBytes + otherCoreBytes
}

data class ReceivedArtifactModel(
	val id: String,
	val locator: String,
	val locatorKind: uniffi.vnidrop.ReceivedLocatorKind,
	val logicalSize: ULong,
)

sealed interface CoreSignal {
	data class ApprovalChanged(val transferId: ULong) : CoreSignal
	data class ReceiverHistoryChanged(val transferId: ULong) : CoreSignal
	/** Transfer status/history changed enough to re-read the durable snapshot. */
	data class TransfersChanged(val transferId: ULong) : CoreSignal
}

interface CoreGateway {
	val state: StateFlow<CoreState>
	val signals: SharedFlow<CoreSignal>

	suspend fun initialize(
		appDataDir: String,
		relaySettings: RelaySettings = RelaySettings(),
	): Result<Unit>
	fun shutdown()
	suspend fun sharePath(path: String, transferName: String, senderName: String, accessPolicy: ShareAccessPolicy): Result<Share>
	suspend fun shareFileDescriptor(
		fd: Int,
		displayName: String,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share>
	/** Multi-source share used by multi-file pickers. */
	suspend fun shareSources(
		sources: List<uniffi.vnidrop.ShareSource>,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share>
	suspend fun inspectTicket(ticket: String): Result<TicketInspectionModel>
	suspend fun receive(ticket: String, outputDir: String, receiverName: String): Result<Unit>
	suspend fun receiveWithOutputSink(ticket: String, outputSink: ReceiveOutputSink, receiverName: String): Result<Unit>
	suspend fun receiveWithOutputSinkV2(ticket: String, outputSink: ReceiveOutputSinkV2, receiverName: String): Result<Unit>
	suspend fun storageUsage(): Result<CoreStorageUsageModel>
	suspend fun clearTransferCache(): Result<ULong>
	suspend fun receivedArtifacts(): Result<List<ReceivedArtifactModel>>
	suspend fun cancel(transferId: ULong): Result<Unit>
	suspend fun delete(transferId: ULong): Result<Unit>
	suspend fun clearReceiveHistory(): Result<ULong>
	suspend fun receiverRequests(transferId: ULong): Result<List<ReceiverRequestModel>>
	suspend fun respondReceiverRequest(requestId: String, accepted: Boolean, reason: String? = null): Result<Unit>
	suspend fun refresh(): Result<Unit>
}
