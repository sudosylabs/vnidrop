package com.vnidrop.app.core

import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.IO
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.withContext
import kotlin.random.Random
import uniffi.vnidrop.CoreEvent
import uniffi.vnidrop.CoreEventSink
import uniffi.vnidrop.CoreNetworkConfig
import uniffi.vnidrop.CoreRelayMode
import uniffi.vnidrop.DeviceRelationship
import uniffi.vnidrop.DeviceRelationshipState
import uniffi.vnidrop.PairingEligibilitySummary
import uniffi.vnidrop.PendingTargetedOffer
import uniffi.vnidrop.ReceiveOutputSink
import uniffi.vnidrop.ReceiveOutputSinkV2
import uniffi.vnidrop.ReceiverRequest
import uniffi.vnidrop.SavedDevice
import uniffi.vnidrop.ShareMetadataInput
import uniffi.vnidrop.ShareResult
import uniffi.vnidrop.ShareSource
import uniffi.vnidrop.SourceKind
import uniffi.vnidrop.StoredTransfer
import uniffi.vnidrop.TargetedOfferResponse
import uniffi.vnidrop.TargetedTransfer
import uniffi.vnidrop.TargetedTransferState
import uniffi.vnidrop.TicketInspection
import uniffi.vnidrop.TransferMetadata
import uniffi.vnidrop.TransferAccessMode
import uniffi.vnidrop.VnidropCore
import uniffi.vnidrop.clearInactiveTransferCache
import uniffi.vnidrop.defaultCoreLimits
import uniffi.vnidrop.defaultCoreNetworkConfig

class CoreRepository internal constructor(
	private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
	private val coreFactory: CoreFactory = ProtectedCoreFactory,
) : CoreGateway {
	private val _state = MutableStateFlow(CoreState())
	override val state: StateFlow<CoreState> = _state.asStateFlow()

	private val _signals = MutableSharedFlow<CoreSignal>(
		extraBufferCapacity = 64,
		onBufferOverflow = BufferOverflow.DROP_OLDEST,
	)
	override val signals: SharedFlow<CoreSignal> = _signals.asSharedFlow()

	private var core: VnidropCore? = null
	private var currentAppDataDir: String? = null
	private var currentRelaySettings = RelaySettings()
	private val lifecycleGate = CoreLifecycleGate()

	private val sink = object : CoreEventSink {
		override fun onEvent(event: CoreEvent) {
			val model = event.toModel()
			_state.update { current -> current.copy(events = (listOf(model) + current.events).take(MaxEvents)) }
			for (signal in signalsForCoreEvent(model.phase, model.transferId)) {
				_signals.tryEmit(signal)
			}
			val transferId = model.transferId
			if (transferId != null && model.shouldRefreshTransfers()) {
				_signals.tryEmit(CoreSignal.TransfersChanged(transferId))
			}
		}
	}

	override suspend fun initialize(appDataDir: String, relaySettings: RelaySettings): Result<Unit> =
		runReconfiguration {
			val previousCore = core
			if (previousCore != null) {
				val status = previousCore.status()
				require(status.activeTransfers == 0UL && status.activeShares == 0UL) {
					"Stop active transfers and shares before changing relay settings"
				}
			}
			_state.update { it.copy(isInitialized = false, status = null) }
			core = null
			disposeCore(previousCore)
			core = createCore(appDataDir, relaySettings)
			currentAppDataDir = appDataDir
			currentRelaySettings = relaySettings
			refreshSnapshot(requireCore())
			_state.update { it.copy(isInitialized = true) }
		}

	override fun shutdown() {
		val activeCore = core
		core = null
		disposeCore(activeCore)
		currentAppDataDir = null
		_state.value = CoreState()
	}

	override suspend fun sharePath(
		path: String,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share> =
		shareSources(
			sources = listOf(
				ShareSource(
					kind = SourceKind.PATH,
					value = path,
					displayName = null,
					isDirectory = false,
				),
			),
			transferName = transferName,
			senderName = senderName,
			accessPolicy = accessPolicy,
		)

	override suspend fun shareFileDescriptor(
		fd: Int,
		displayName: String,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share> =
		shareSources(
			sources = listOf(
				ShareSource(
					kind = SourceKind.FILE_DESCRIPTOR,
					value = fd.toString(),
					displayName = displayName.ifBlank { "transfer" },
					isDirectory = false,
				),
			),
			transferName = transferName,
			senderName = senderName,
			accessPolicy = accessPolicy,
		)

	override suspend fun inspectTicket(ticket: String): Result<TicketInspectionModel> = runCore { activeCore ->
		activeCore.inspectTicket(ticket).toModel().also { inspection ->
			_state.update { it.copy(lastInspection = inspection) }
		}
	}

	override suspend fun receive(ticket: String, outputDir: String, receiverName: String): Result<Unit> = runCore { activeCore ->
		activeCore.receive(ticket, outputDir, receiverName.ifBlank { null })
		refreshSnapshot(activeCore)
	}

	override suspend fun receiveWithOutputSink(
		ticket: String,
		outputSink: ReceiveOutputSink,
		receiverName: String,
	): Result<Unit> = runCore { activeCore ->
		activeCore.receiveWithOutputSink(ticket, outputSink, receiverName.ifBlank { null })
		refreshSnapshot(activeCore)
	}

	override suspend fun receiveWithOutputSinkV2(
		ticket: String,
		outputSink: ReceiveOutputSinkV2,
		receiverName: String,
	): Result<Unit> = runCore { activeCore ->
		activeCore.receiveWithOutputSinkV2(ticket, outputSink, receiverName.ifBlank { null })
		refreshSnapshot(activeCore)
	}

	override suspend fun storageUsage(): Result<CoreStorageUsageModel> = runCore { activeCore ->
		val usage = activeCore.storageUsage()
		CoreStorageUsageModel(
			blobStoreBytes = usage.blobStoreBytes,
			databaseBytes = usage.databaseBytes,
			logsBytes = usage.logsBytes,
			previewsBytes = usage.previewsBytes,
			otherCoreBytes = usage.otherCoreBytes,
		)
	}

	override suspend fun clearTransferCache(): Result<ULong> = runReconfiguration {
		val activeCore = requireCore()
		val status = activeCore.status()
		require(status.activeTransfers == 0UL && status.activeShares == 0UL) {
			"Wait for active transfers and shares to finish before clearing the transfer cache"
		}
		val appDataDir = requireNotNull(currentAppDataDir) { "Core data directory is unavailable" }
		val relaySettings = currentRelaySettings
		_state.update { it.copy(isInitialized = false, status = null) }
		core = null
		disposeCore(activeCore)
		var reclaimed = 0UL
		try {
			reclaimed = clearInactiveTransferCache(appDataDir)
		} finally {
			core = createCore(appDataDir, relaySettings)
			refreshSnapshot(requireCore())
			_state.update { it.copy(isInitialized = true) }
		}
		reclaimed
	}

	private fun disposeCore(activeCore: VnidropCore?) {
		if (activeCore == null) return
		try {
			activeCore.shutdown()
		} finally {
			// UniFFI owns the Rust Arc; explicit disposal releases Windows file handles before cache deletion.
			activeCore.close()
		}
	}

	private fun createCore(appDataDir: String, relaySettings: RelaySettings): VnidropCore =
		coreFactory.create(appDataDir, sink, relaySettings)

	override suspend fun receivedArtifacts(): Result<List<ReceivedArtifactModel>> = runCore { activeCore ->
		activeCore.listReceivedArtifacts().map { artifact ->
			ReceivedArtifactModel(
				id = artifact.id,
				locator = artifact.locator,
				locatorKind = artifact.locatorKind,
				logicalSize = artifact.logicalSize,
			)
		}
	}

	override suspend fun cancel(transferId: ULong): Result<Unit> = runCore { activeCore ->
		activeCore.cancelTransfer(transferId)
		refreshSnapshot(activeCore)
	}

	override suspend fun delete(transferId: ULong): Result<Unit> = runCore { activeCore ->
		activeCore.deleteTransfer(transferId)
		refreshSnapshot(activeCore)
		_signals.tryEmit(CoreSignal.ApprovalChanged(transferId))
		_signals.tryEmit(CoreSignal.ReceiverHistoryChanged(transferId))
	}

	override suspend fun clearReceiveHistory(): Result<ULong> = runCore { activeCore ->
		val deleted = activeCore.deleteReceiveHistory()
		refreshSnapshot(activeCore)
		deleted
	}

	override suspend fun receiverRequests(transferId: ULong): Result<List<ReceiverRequestModel>> = runCore { activeCore ->
		activeCore.listReceiverRequests(transferId).map(ReceiverRequest::toModel)
	}

	override suspend fun respondReceiverRequest(
		requestId: String,
		accepted: Boolean,
		reason: String?,
	): Result<Unit> = runCore { activeCore ->
		activeCore.respondReceiverRequest(requestId, accepted, reason)
	}

	override suspend fun refresh(): Result<Unit> = runCore(::refreshSnapshot)

	override suspend fun listPairingEligibilities(): Result<List<PairingEligibilityModel>> = runCore { activeCore ->
		activeCore.listPairingEligibilities().map { it.toModel() }
	}

	override suspend fun declinePairingEligibility(peerEndpointId: String): Result<Unit> = runCore { activeCore ->
		activeCore.declinePairingEligibility(peerEndpointId)
	}

	override suspend fun requestSavedDevicePairing(peerEndpointId: String): Result<Boolean> = runCore { activeCore ->
		activeCore.requestSavedDevicePairing(peerEndpointId)
	}

	override suspend fun respondToDevicePairing(peerEndpointId: String, accepted: Boolean): Result<Boolean> =
		runCore { activeCore ->
			activeCore.respondToDevicePairing(peerEndpointId, accepted)
		}

	override suspend fun listDeviceRelationships(): Result<List<DeviceRelationshipModel>> = runCore { activeCore ->
		activeCore.listDeviceRelationships().map { it.toModel() }
	}

	override suspend fun listSavedDevices(): Result<List<SavedDeviceModel>> = runCore { activeCore ->
		activeCore.listSavedDevices().map { it.toModel() }
	}

	override suspend fun setSavedDeviceLabel(peerEndpointId: String, label: String?): Result<Unit> = runCore { activeCore ->
		activeCore.setSavedDeviceLabel(peerEndpointId, label)
	}

	override suspend fun forgetSavedDevice(peerEndpointId: String): Result<Unit> = runCore { activeCore ->
		activeCore.forgetSavedDevice(peerEndpointId)
	}

	override suspend fun blockDevice(peerEndpointId: String): Result<Unit> = runCore { activeCore ->
		activeCore.blockDevice(peerEndpointId)
	}

	override suspend fun unblockDevice(peerEndpointId: String): Result<Unit> = runCore { activeCore ->
		activeCore.unblockDevice(peerEndpointId)
	}

	override suspend fun listBlockedDevices(): Result<List<String>> = runCore { activeCore ->
		activeCore.listBlockedDevices()
	}

	override suspend fun listPendingTargetedOffers(): Result<List<PendingTargetedOfferModel>> = runCore { activeCore ->
		activeCore.listPendingTargetedOffers().map { it.toModel() }
	}

	override suspend fun respondToTargetedOffer(
		transferId: String,
		accepted: Boolean,
	): Result<TargetedOfferResponseModel> = runCore { activeCore ->
		activeCore.respondToTargetedOffer(transferId, accepted).toModel()
	}

	override suspend fun createTargetedTransfer(
		receiverEndpointId: String,
		sources: List<ShareSource>,
		transferName: String?,
	): Result<TargetedTransferModel> = runCore { activeCore ->
		require(sources.isNotEmpty()) { "Select at least one file to share" }
		withPlatformPathAccess(sources) {
			activeCore.createTargetedTransfer(receiverEndpointId, sources, transferName).toModel()
		}
	}

	override suspend fun getTargetedTransfer(id: String): Result<TargetedTransferModel?> = runCore { activeCore ->
		activeCore.getTargetedTransfer(id)?.toModel()
	}

	override suspend fun listTargetedTransfers(): Result<List<TargetedTransferModel>> = runCore { activeCore ->
		activeCore.listTargetedTransfers().map { it.toModel() }
	}

	override suspend fun receiveTargetedTransfer(transferId: String, outputDir: String): Result<Unit> =
		runCore { activeCore ->
			activeCore.receiveTargetedTransfer(transferId, outputDir)
		}

	override suspend fun receiveTargetedTransferWithOutputSink(
		transferId: String,
		outputSink: ReceiveOutputSink,
	): Result<Unit> = runCore { activeCore ->
		activeCore.receiveTargetedTransferWithOutputSink(transferId, outputSink)
	}

	override suspend fun receiveTargetedTransferWithOutputSinkV2(
		transferId: String,
		outputSink: ReceiveOutputSinkV2,
	): Result<Unit> = runCore { activeCore ->
		activeCore.receiveTargetedTransferWithOutputSinkV2(transferId, outputSink)
	}

	override suspend fun resumeTargetedTransfer(id: String, outputDir: String): Result<Unit> = runCore { activeCore ->
		activeCore.resumeTargetedTransfer(id, outputDir)
	}

	override suspend fun resumeTargetedTransferWithOutputSinkV2(
		id: String,
		outputSink: ReceiveOutputSinkV2,
	): Result<Unit> = runCore { activeCore ->
		activeCore.resumeTargetedTransferWithOutputSinkV2(id, outputSink)
	}

	override suspend fun cancelTargetedTransfer(id: String): Result<Unit> = runCore { activeCore ->
		activeCore.cancelTargetedTransfer(id)
	}

	override suspend fun deleteTargetedTransfer(id: String): Result<Unit> = runCore { activeCore ->
		activeCore.deleteTargetedTransfer(id)
	}

	override suspend fun shareSources(
		sources: List<ShareSource>,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share> = runCore { activeCore ->
		require(sources.isNotEmpty()) { "Select at least one file to share" }
		withPlatformPathAccess(sources) {
			activeCore.shareFiles(
				sources = sources,
				metadata = ShareMetadataInput(
					transferId = nextTransferId(),
					transferName = transferName.ifBlank { null },
					senderName = senderName.ifBlank { null },
					accessMode = accessPolicy.toNative(),
				),
			).toModel()
		}.also { share ->
			refreshSnapshot(activeCore)
			_state.update { it.copy(lastShare = share) }
		}
	}

	private suspend fun <T> withPlatformPathAccess(
		sources: List<ShareSource>,
		index: Int = 0,
		block: suspend () -> T,
	): T {
		if (index >= sources.size) return block()
		val source = sources[index]
		return withPlatformPathAccess(source.kind, source.value) {
			withPlatformPathAccess(sources, index + 1, block)
		}
	}

	private fun refreshSnapshot(activeCore: VnidropCore) {
		val status = activeCore.status()
		_state.update {
			it.copy(
				status = CoreStatus(status.endpointId, status.activeTransfers, status.activeShares),
				transfers = activeCore.listTransfers().map(StoredTransfer::toModel),
				events = activeCore.listEvents(null).map(CoreEvent::toModel).take(MaxEvents),
			)
		}
	}

	private suspend fun <T> runCore(block: suspend (VnidropCore) -> T): Result<T> =
		withContext(dispatcher) {
			try {
				Result.success(
					lifecycleGate.withCall(
						capture = ::requireCore,
						block = block,
					),
				)
			} catch (error: Throwable) {
				if (error is CancellationException) throw error
				Result.failure(error)
			}
		}

	private suspend fun <T> runReconfiguration(block: suspend () -> T): Result<T> =
		withContext(dispatcher) {
			try {
				Result.success(lifecycleGate.withReconfiguration(block))
			} catch (error: Throwable) {
				if (error is CancellationException) throw error
				Result.failure(error)
			}
		}

	private fun requireCore(): VnidropCore = core ?: error("Initialize the core first.")

	private fun nextTransferId(): ULong = Random.nextLong(1, Long.MAX_VALUE).toULong()

	private companion object {
		const val MaxEvents = 200
	}
}

internal fun interface CoreFactory {
	fun create(appDataDir: String, eventSink: CoreEventSink, relaySettings: RelaySettings): VnidropCore
}

private object ProtectedCoreFactory : CoreFactory {
	override fun create(
		appDataDir: String,
		eventSink: CoreEventSink,
		relaySettings: RelaySettings,
	): VnidropCore = VnidropCore.initializeWithLimitsAndNetworkConfig(
		appDataDir,
		eventSink,
		defaultCoreLimits(),
		relaySettings.toNative(),
	)
}

private fun RelaySettings.toNative(): CoreNetworkConfig = when (mode) {
	RelayMode.Automatic -> defaultCoreNetworkConfig()
	RelayMode.StrictCustom -> CoreNetworkConfig(
		mode = CoreRelayMode.STRICT_CUSTOM,
		relayUrls = relayUrls,
	)
	RelayMode.CustomWithDirectFallback -> CoreNetworkConfig(
		mode = CoreRelayMode.CUSTOM_WITH_DIRECT_FALLBACK,
		relayUrls = relayUrls,
	)
	RelayMode.LocalOnly -> CoreNetworkConfig(
		mode = CoreRelayMode.LOCAL_ONLY,
		relayUrls = emptyList(),
	)
}

private fun CoreEvent.toModel(): CoreEventModel = CoreEventModel(
	id = id,
	revision = revision,
	timestamp = timestamp,
	scope = scope,
	transferId = transferId,
	direction = direction,
	phase = phase,
	kind = kind,
	dataJson = dataJson,
)

private fun CoreEventModel.shouldRefreshTransfers(): Boolean =
	phase in setOf("lifecycle", "error", "ticket", "import", "download", "export", "handshake") &&
		kind in setOf(
			"started", "done", "created", "failed", "cancelled", "share-stopped",
			"found-collection", "connected",
		)

private fun StoredTransfer.toModel(): Transfer = Transfer(
	localId = localId,
	transferId = transferId,
	direction = direction.toTransferDirection(),
	status = status.toTransferStatus(),
	peerId = peerId,
	transferName = transferName,
	contentHash = contentHash,
	fileCount = fileCount,
	totalSize = totalSize,
	ticket = ticket,
	accessPolicy = accessMode.toModel(),
	createdAt = createdAt,
	updatedAt = updatedAt,
)

private fun ShareAccessPolicy.toNative(): TransferAccessMode = when (this) {
	ShareAccessPolicy.RequireApproval -> TransferAccessMode.APPROVAL_REQUIRED
	ShareAccessPolicy.AnyoneWithTransfer -> TransferAccessMode.PUBLIC
}

private fun TransferAccessMode.toModel(): ShareAccessPolicy = when (this) {
	TransferAccessMode.APPROVAL_REQUIRED -> ShareAccessPolicy.RequireApproval
	TransferAccessMode.PUBLIC -> ShareAccessPolicy.AnyoneWithTransfer
}

private fun String.toTransferDirection(): TransferDirection = when (this) {
	"send" -> TransferDirection.Send
	"receive" -> TransferDirection.Receive
	else -> error("Unknown transfer direction: $this")
}

private fun String.toTransferStatus(): TransferStatus = when (this) {
	"importing" -> TransferStatus.Importing
	"sharing" -> TransferStatus.Sharing
	"receiving" -> TransferStatus.Receiving
	"done" -> TransferStatus.Done
	"failed" -> TransferStatus.Failed
	"cancelled" -> TransferStatus.Cancelled
	"stopped" -> TransferStatus.Stopped
	else -> error("Unknown transfer status: $this")
}

private fun ShareResult.toModel(): Share = Share(
	transferId = transferId,
	ticket = ticket,
	transferName = transferName,
	contentHash = hash,
	fileCount = fileCount,
	totalSize = totalSize,
)

private fun TicketInspection.toModel(): TicketInspectionModel = TicketInspectionModel(
	kind = kind,
	metadata = metadata.toModel(),
)

private fun TransferMetadata.toModel(): TransferMetadataModel = TransferMetadataModel(
	transferId = transferId,
	transferName = transferName,
	senderName = senderName,
	contentHash = contentHash,
	fileCount = fileCount,
	totalSize = totalSize,
)

private fun ReceiverRequest.toModel(): ReceiverRequestModel = ReceiverRequestModel(
	id = id,
	transferId = transferId,
	remoteEndpointId = remoteEndpointId,
	transferName = transferName,
	receiverName = receiverName,
	receiverDeviceName = receiverDeviceName,
	appVersion = appVersion,
	status = when (status) {
		"requested" -> ReceiverDeliveryStatus.Requested
		"accepted" -> ReceiverDeliveryStatus.Accepted
		"refused" -> ReceiverDeliveryStatus.Refused
		"expired" -> ReceiverDeliveryStatus.Expired
		"completed" -> ReceiverDeliveryStatus.Completed
		"failed" -> ReceiverDeliveryStatus.Failed
		else -> ReceiverDeliveryStatus.Unknown
	},
	reason = reason,
	requestedAt = requestedAt,
	respondedAt = respondedAt,
	completedAt = completedAt,
)

private fun PairingEligibilitySummary.toModel(): PairingEligibilityModel = PairingEligibilityModel(
	peerEndpointId = peerEndpointId,
	remoteDisplayName = remoteDisplayName,
	sessionId = sessionId,
	protocolVersion = protocolVersion,
	createdAt = createdAt,
	expiresAt = expiresAt,
)

private fun DeviceRelationship.toModel(): DeviceRelationshipModel = DeviceRelationshipModel(
	remoteEndpointId = remoteEndpointId,
	state = state.toModel(),
	generation = generation,
	minimumProtocolVersion = minimumProtocolVersion,
	createdAt = createdAt,
	updatedAt = updatedAt,
)

private fun DeviceRelationshipState.toModel(): DeviceRelationshipStateModel = when (this) {
	DeviceRelationshipState.PENDING_OUTGOING -> DeviceRelationshipStateModel.PendingOutgoing
	DeviceRelationshipState.PENDING_INCOMING -> DeviceRelationshipStateModel.PendingIncoming
	DeviceRelationshipState.SAVED -> DeviceRelationshipStateModel.Saved
	DeviceRelationshipState.REVOKED -> DeviceRelationshipStateModel.Revoked
	DeviceRelationshipState.BLOCKED -> DeviceRelationshipStateModel.Blocked
}

private fun SavedDevice.toModel(): SavedDeviceModel = SavedDeviceModel(
	endpointId = endpointId,
	localLabel = localLabel,
	remoteDisplayName = remoteDisplayName,
	createdAt = createdAt,
	lastAuthenticatedAt = lastAuthenticatedAt,
)

private fun PendingTargetedOffer.toModel(): PendingTargetedOfferModel = PendingTargetedOfferModel(
	transferId = transferId,
	senderEndpointId = senderEndpointId,
	receiverEndpointId = receiverEndpointId,
	manifestId = manifestId,
	contentHash = contentHash,
	transferName = transferName,
	fileCount = fileCount,
	totalSize = totalSize,
	protocolVersion = protocolVersion,
	receivedAt = receivedAt,
)

private fun TargetedTransfer.toModel(): TargetedTransferModel = TargetedTransferModel(
	id = id,
	senderEndpointId = senderEndpointId,
	receiverEndpointId = receiverEndpointId,
	manifestId = manifestId,
	transferName = transferName,
	fileCount = fileCount,
	totalSize = totalSize,
	verifiedBytes = verifiedBytes,
	state = state.toModel(),
	createdAt = createdAt,
	updatedAt = updatedAt,
)

private fun TargetedTransferState.toModel(): TargetedTransferStateModel = when (this) {
	TargetedTransferState.PREPARING -> TargetedTransferStateModel.Preparing
	TargetedTransferState.OFFERING -> TargetedTransferStateModel.Offering
	TargetedTransferState.AWAITING_APPROVAL -> TargetedTransferStateModel.AwaitingApproval
	TargetedTransferState.APPROVED -> TargetedTransferStateModel.Approved
	TargetedTransferState.CONNECTING -> TargetedTransferStateModel.Connecting
	TargetedTransferState.TRANSFERRING -> TargetedTransferStateModel.Transferring
	TargetedTransferState.INTERRUPTED -> TargetedTransferStateModel.Interrupted
	TargetedTransferState.COMPLETED -> TargetedTransferStateModel.Completed
	TargetedTransferState.DECLINED -> TargetedTransferStateModel.Declined
	TargetedTransferState.CANCELLED -> TargetedTransferStateModel.Cancelled
	TargetedTransferState.FAILED -> TargetedTransferStateModel.Failed
	TargetedTransferState.DELETED -> TargetedTransferStateModel.Deleted
}

private fun TargetedOfferResponse.toModel(): TargetedOfferResponseModel = when (this) {
	is TargetedOfferResponse.Approved -> TargetedOfferResponseModel.Approved(transferId)
	TargetedOfferResponse.Declined -> TargetedOfferResponseModel.Declined
	is TargetedOfferResponse.AlreadySettled -> TargetedOfferResponseModel.AlreadySettled(transferId)
}
