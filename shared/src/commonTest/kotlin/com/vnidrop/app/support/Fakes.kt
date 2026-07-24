package com.vnidrop.app.support

import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreStorageUsageModel
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.FileSystemService
import com.vnidrop.app.core.FolderAccessStatus
import com.vnidrop.app.core.PickedShareFile
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceivedArtifactModel
import com.vnidrop.app.core.ReceivedStorageInspection
import com.vnidrop.app.core.RelaySettings
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.Share
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.TicketInspectionModel
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.notifications.LocalNotification
import com.vnidrop.app.notifications.LocalNotificationService
import com.vnidrop.app.notifications.NotificationPermission
import com.vnidrop.app.preferences.AppPreferences
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.feature.send.FilePreviewRepository
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import uniffi.vnidrop.ReceiveOutputSink
import uniffi.vnidrop.ReceiveOutputSinkV2

class FakeCoreGateway : CoreGateway {
	val mutableState = MutableStateFlow(CoreState())
	override val state: StateFlow<CoreState> = mutableState
	val mutableSignals = MutableSharedFlow<CoreSignal>(extraBufferCapacity = 16)
	override val signals: SharedFlow<CoreSignal> = mutableSignals
	val requests = mutableMapOf<ULong, List<ReceiverRequestModel>>()
	var responseResult: Result<Unit> = Result.success(Unit)
	val responses = mutableListOf<Triple<String, Boolean, String?>>()
	var shareResult: Result<Share> = Result.failure(UnsupportedOperationException())
	var inspectionResult: Result<TicketInspectionModel> = Result.failure(UnsupportedOperationException())
	var receiveResult: Result<Unit> = Result.success(Unit)
	var receiveSuspend: Boolean = false
	private var receiveGate: CompletableDeferred<Unit>? = null
	var deleteResult: Result<Unit> = Result.success(Unit)
	var clearTransferCacheResult: Result<ULong> = Result.success(0UL)
	var clearReceiveHistoryResult: Result<ULong> = Result.success(0UL)
	val deletedTransfers = mutableListOf<ULong>()
	val cancelledTransfers = mutableListOf<ULong>()
	var clearReceiveHistoryCount = 0
	var clearTransferCacheCount = 0
	var receiveCount = 0
	var lastReceiveTicket: String? = null
	var lastReceiveReceiverName: String? = null
	var lastShareAccessPolicy: ShareAccessPolicy? = null
	val initializedRelaySettings = mutableListOf<RelaySettings>()
	var initializeHandler: (RelaySettings) -> Result<Unit> = { Result.success(Unit) }

	fun completeSuspendedReceive() {
		receiveGate?.complete(Unit)
	}

	private suspend fun awaitReceiveIfNeeded() {
		if (!receiveSuspend) return
		val gate = CompletableDeferred<Unit>()
		receiveGate = gate
		gate.await()
	}

	override suspend fun initialize(appDataDir: String, relaySettings: RelaySettings): Result<Unit> {
		initializedRelaySettings += relaySettings
		return initializeHandler(relaySettings).onSuccess {
			mutableState.value = mutableState.value.copy(isInitialized = true)
		}
	}
	override fun shutdown() = Unit
	var lastShareSourceCount: Int = 0
	override suspend fun sharePath(path: String, transferName: String, senderName: String, accessPolicy: ShareAccessPolicy): Result<Share> =
		shareSources(
			sources = listOf(
				uniffi.vnidrop.ShareSource(
					kind = uniffi.vnidrop.SourceKind.PATH,
					value = path,
					displayName = path.substringAfterLast('/'),
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
	) = Result.failure<Share>(UnsupportedOperationException())
	override suspend fun shareSources(
		sources: List<uniffi.vnidrop.ShareSource>,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share> {
		lastShareAccessPolicy = accessPolicy
		lastShareSourceCount = sources.size
		shareResult.onSuccess { share ->
			mutableState.value = mutableState.value.copy(
				transfers = listOf(
					Transfer(
						localId = "local-${share.transferId}",
						transferId = share.transferId,
						direction = TransferDirection.Send,
						status = TransferStatus.Sharing,
						peerId = null,
						transferName = share.transferName,
						contentHash = share.contentHash,
						fileCount = share.fileCount,
						totalSize = share.totalSize,
						ticket = share.ticket,
						accessPolicy = accessPolicy,
						createdAt = 1L,
						updatedAt = 1L,
					),
				) + mutableState.value.transfers,
			)
		}
		return shareResult
	}
	override suspend fun inspectTicket(ticket: String) = inspectionResult
	override suspend fun receive(ticket: String, outputDir: String, receiverName: String): Result<Unit> {
		receiveCount += 1
		lastReceiveTicket = ticket
		lastReceiveReceiverName = receiverName
		awaitReceiveIfNeeded()
		return receiveResult
	}
	override suspend fun receiveWithOutputSink(ticket: String, outputSink: ReceiveOutputSink, receiverName: String): Result<Unit> {
		receiveCount += 1
		lastReceiveTicket = ticket
		lastReceiveReceiverName = receiverName
		awaitReceiveIfNeeded()
		return receiveResult
	}
	override suspend fun receiveWithOutputSinkV2(ticket: String, outputSink: ReceiveOutputSinkV2, receiverName: String): Result<Unit> {
		receiveCount += 1
		lastReceiveTicket = ticket
		lastReceiveReceiverName = receiverName
		awaitReceiveIfNeeded()
		return receiveResult
	}
	override suspend fun storageUsage(): Result<CoreStorageUsageModel> = Result.success(
		CoreStorageUsageModel(0UL, 0UL, 0UL, 0UL, 0UL),
	)
	override suspend fun clearTransferCache(): Result<ULong> {
		clearTransferCacheCount += 1
		return clearTransferCacheResult
	}
	override suspend fun receivedArtifacts(): Result<List<ReceivedArtifactModel>> = Result.success(emptyList())
	override suspend fun cancel(transferId: ULong): Result<Unit> {
		cancelledTransfers += transferId
		return Result.success(Unit)
	}
	override suspend fun delete(transferId: ULong): Result<Unit> {
		if (deleteResult.isSuccess) {
			deletedTransfers += transferId
			mutableState.value = mutableState.value.copy(
				transfers = mutableState.value.transfers.filterNot { it.transferId == transferId },
			)
		}
		return deleteResult
	}
	override suspend fun clearReceiveHistory(): Result<ULong> {
		clearReceiveHistoryCount += 1
		clearReceiveHistoryResult.onSuccess {
			mutableState.value = mutableState.value.copy(
				transfers = mutableState.value.transfers.filterNot { transfer ->
					transfer.direction == TransferDirection.Receive && transfer.status in setOf(
						TransferStatus.Done,
						TransferStatus.Failed,
						TransferStatus.Cancelled,
					)
				},
			)
		}
		return clearReceiveHistoryResult
	}
	override suspend fun receiverRequests(transferId: ULong) = Result.success(requests[transferId].orEmpty())
	override suspend fun respondReceiverRequest(requestId: String, accepted: Boolean, reason: String?): Result<Unit> {
		responses += Triple(requestId, accepted, reason)
		return responseResult
	}
	override suspend fun refresh() = Result.success(Unit)
}

class FakePreferencesRepository(
	initial: AppPreferences,
) : PreferencesRepository {
	val mutablePreferences = MutableStateFlow(initial)
	override val preferences = mutablePreferences
	override suspend fun setUsername(username: String) {
		mutablePreferences.value = mutablePreferences.value.copy(username = username.trim())
	}
	override suspend fun setReceiveFolder(folder: ReceiveFolder) { mutablePreferences.value = mutablePreferences.value.copy(receiveFolder = folder) }
	override suspend fun resetReceiveFolder() = Unit
	override suspend fun setThemeMode(mode: ThemeMode) { mutablePreferences.value = mutablePreferences.value.copy(themeMode = mode) }
	override suspend fun setNotificationsEnabled(enabled: Boolean) { mutablePreferences.value = mutablePreferences.value.copy(notificationsEnabled = enabled) }
	override suspend fun setDiagnosticsEnabled(enabled: Boolean) {
		mutablePreferences.value = mutablePreferences.value.copy(diagnosticsEnabled = enabled)
	}
	override suspend fun setRelaySettings(settings: RelaySettings) {
		mutablePreferences.value = mutablePreferences.value.copy(relaySettings = settings)
	}
	override suspend fun ensureDiagnosticsInstallId(): String {
		val existing = mutablePreferences.value.diagnosticsInstallId
		if (existing.isNotBlank()) return existing
		val created = "test-install-id"
		mutablePreferences.value = mutablePreferences.value.copy(diagnosticsInstallId = created)
		return created
	}
}

class FakeNotificationService(
	permission: NotificationPermission = NotificationPermission.Granted,
) : LocalNotificationService {
	val mutablePermission = MutableStateFlow(permission)
	override val permission: StateFlow<NotificationPermission> = mutablePermission
	val published = mutableListOf<LocalNotification>()
	val cancelled = mutableListOf<String>()
	var cancelAllCount = 0
	var openSettingsCount = 0
	var openSettingsResult: Result<Unit> = Result.success(Unit)
	override suspend fun refreshPermission() = permission.value
	override suspend fun requestPermission() = permission.value
	override suspend fun openSettings(): Result<Unit> = openSettingsResult.also { openSettingsCount += 1 }
	override suspend fun publish(notification: LocalNotification): Result<Unit> = Result.success(Unit).also { published += notification }
	override suspend fun cancel(id: String) { cancelled += id }
	override suspend fun cancelAll() { cancelAllCount += 1 }
}

class FakeFileSystemService(
	private val folder: ReceiveFolder,
) : FileSystemService {
	var supportsCustomFolders = true
	var effectiveFolder: ReceiveFolder? = null
	var canRevealFolder = false
	var revealFolderResult: Result<Unit> = Result.success(Unit)
	val revealedFolders = mutableListOf<ReceiveFolder>()
	val discardedPickedFiles = mutableListOf<PickedShareFile>()
	override val supportsCustomReceiveFolders: Boolean get() = supportsCustomFolders
	override fun defaultReceiveFolder() = folder
	override fun effectiveReceiveFolder(configuredFolder: ReceiveFolder) =
		effectiveFolder ?: super.effectiveReceiveFolder(configuredFolder)
	override suspend fun validateReceiveFolder(folder: ReceiveFolder) = FolderAccessStatus.Writable
	override suspend fun inspectReceivedArtifacts(artifacts: List<ReceivedArtifactModel>) =
		ReceivedStorageInspection(artifacts.fold(0UL) { total, item -> total + item.logicalSize }, artifacts.size, 0, 0)
	override suspend fun temporaryUsage(receiveFolder: ReceiveFolder): ULong = 0UL
	override fun createReceiveOutputSink(folder: ReceiveFolder): ReceiveOutputSinkV2? = null
	override fun canRevealReceiveFolder(folder: ReceiveFolder) = canRevealFolder
	override suspend fun revealReceiveFolder(folder: ReceiveFolder): Result<Unit> {
		revealedFolders += folder
		return revealFolderResult
	}
	override suspend fun discardPickedFiles(files: List<PickedShareFile>) {
		discardedPickedFiles += files
	}
	override suspend fun sharePickedFiles(
		repository: CoreGateway,
		files: List<PickedShareFile>,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share> {
		val sources = files.map { file ->
			uniffi.vnidrop.ShareSource(
				kind = uniffi.vnidrop.SourceKind.PATH,
				value = file.value,
				displayName = file.displayName,
				isDirectory = false,
			)
		}
		return repository.shareSources(sources, transferName, senderName, accessPolicy)
	}
}

class FakeFilePreviewRepository : FilePreviewRepository {
	private val state = MutableStateFlow<Map<ULong, ByteArray>>(emptyMap())
	override val previews: StateFlow<Map<ULong, ByteArray>> = state
	val restored = mutableListOf<Set<ULong>>()
	override suspend fun restore(activeTransferIds: Set<ULong>) { restored += activeTransferIds }
	override suspend fun save(transferId: ULong, bytes: ByteArray) { state.value = state.value + (transferId to bytes) }
	override suspend fun remove(transferId: ULong) { state.value = state.value - transferId }
}
