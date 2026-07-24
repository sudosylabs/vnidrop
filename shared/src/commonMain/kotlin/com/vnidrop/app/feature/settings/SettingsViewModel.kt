package com.vnidrop.app.feature.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vnidrop.app.DeviceInfo
import com.vnidrop.app.DeviceInfoProvider
import com.vnidrop.app.PlatformEnvironment
import com.vnidrop.app.core.FileSystemService
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreLifecycleBusyException
import com.vnidrop.app.core.FolderAccessStatus
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.RelayMode
import com.vnidrop.app.core.RelaySettings
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.core.usesCustomRelayUrls
import com.vnidrop.app.diagnostics.BugReportDraft
import com.vnidrop.app.diagnostics.BugReportService
import com.vnidrop.app.diagnostics.DiagnosticsBuildConfig
import com.vnidrop.app.diagnostics.DiagnosticsCoordinator
import com.vnidrop.app.notifications.LocalNotificationService
import com.vnidrop.app.notifications.NotificationPermission
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessage
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiMessageTone
import com.vnidrop.app.ui.feedback.UiText
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.bug_report_missing_expected
import vnidrop.shared.generated.resources.bug_report_missing_what
import vnidrop.shared.generated.resources.bug_report_submit_failed
import vnidrop.shared.generated.resources.bug_report_submitted
import vnidrop.shared.generated.resources.button_open_settings
import vnidrop.shared.generated.resources.diagnostics_disabled_message
import vnidrop.shared.generated.resources.diagnostics_enabled_message
import vnidrop.shared.generated.resources.notifications_enabled_message
import vnidrop.shared.generated.resources.notifications_permission_denied
import vnidrop.shared.generated.resources.notifications_settings_open_failed
import vnidrop.shared.generated.resources.notifications_unsupported
import vnidrop.shared.generated.resources.relay_settings_applied
import vnidrop.shared.generated.resources.storage_transfer_cache_cleared
import vnidrop.shared.generated.resources.storage_transfers_deleted

enum class SettingsSection {
	Overview,
	Preferences,
	Appearance,
	Network,
	Notifications,
	Storage,
	About,
	BugReport,
}

enum class RelaySettingsApplyError {
	ActiveTransfers,
	ApplyFailed,
	RestoreFailed,
}

data class StorageBreakdown(
	val transferCacheBytes: ULong,
	val appDataBytes: ULong,
	val temporaryBytes: ULong,
	val receivedBytes: ULong,
	val receivedFileCount: Int,
	val missingReceivedFileCount: Int,
	val inaccessibleReceivedFileCount: Int,
) {
	val deviceImpactBytes: ULong get() = transferCacheBytes + appDataBytes + temporaryBytes + receivedBytes
}

data class SettingsState(
	val selectedSection: SettingsSection = SettingsSection.Overview,
	val username: String = "",
	val receiveFolder: ReceiveFolder? = null,
	val folderAccessStatus: FolderAccessStatus = FolderAccessStatus.Unavailable,
	val isValidatingFolder: Boolean = false,
	val supportsCustomReceiveFolders: Boolean = true,
	val themeMode: ThemeMode = ThemeMode.System,
	val savedRelaySettings: RelaySettings = RelaySettings(),
	val relayMode: RelayMode = RelayMode.Automatic,
	val relayUrls: List<String> = emptyList(),
	val relayInputError: RelaySettingsInputError? = null,
	val relayApplyError: RelaySettingsApplyError? = null,
	val isApplyingRelaySettings: Boolean = false,
	val hasActiveNetworkWork: Boolean = false,
	val endpointId: String? = null,
	val notificationsEnabled: Boolean = false,
	val notificationPermission: NotificationPermission = NotificationPermission.NotDetermined,
	val diagnosticsEnabled: Boolean = false,
	val deviceInfo: DeviceInfo? = null,
	val appVersion: String = "",
	val isLoadingDeviceInfo: Boolean = false,
	val bugWhatHappened: String = "",
	val bugExpected: String = "",
	val bugSteps: String = "",
	val bugContact: String = "",
	val bugIncludeLogs: Boolean = true,
	val isSubmittingBugReport: Boolean = false,
	val bugLogPreviewBytes: Int = 0,
	val storage: StorageBreakdown? = null,
	val isCalculatingStorage: Boolean = false,
	val isDeletingTransfers: Boolean = false,
	val isClearingTransferCache: Boolean = false,
) {
	val hasRelaySettingsChanges: Boolean
		get() = relayMode != savedRelaySettings.mode ||
			(relayMode.usesCustomRelayUrls && relayUrls != savedRelaySettings.relayUrls)
}

sealed interface SettingsEffect {
	data object OpenReceiveFolderPicker : SettingsEffect
}

class SettingsViewModel(
	private val environment: PlatformEnvironment,
	private val deviceInfoProvider: DeviceInfoProvider,
	private val fileSystemService: FileSystemService,
	private val repository: CoreGateway,
	private val preferencesRepository: PreferencesRepository,
	private val notifications: LocalNotificationService,
	private val messages: UiMessageController,
	private val bugReports: BugReportService,
	private val diagnostics: DiagnosticsCoordinator? = null,
	private val diagnosticsIncluded: Boolean = DiagnosticsBuildConfig.INCLUDED,
) : ViewModel() {
	private val _state = MutableStateFlow(
		SettingsState(
			appVersion = environment.appVersion,
			supportsCustomReceiveFolders = fileSystemService.supportsCustomReceiveFolders,
		),
	)
	val state: StateFlow<SettingsState> = _state.asStateFlow()

	private val effects = Channel<SettingsEffect>(Channel.BUFFERED)
	val effectFlow = effects.receiveAsFlow()
	private var enableNotificationsAfterSettings = false
	private var usernamePersistJob: Job? = null
	private var hasLocalUsernameDraft = false
	private var hasLocalRelayDraft = false

	init {
		viewModelScope.launch {
			preferencesRepository.preferences.collect { preferences ->
				val previousFolder = _state.value.receiveFolder
				val receiveFolder = fileSystemService.effectiveReceiveFolder(preferences.receiveFolder)
				_state.update { current ->
					current.copy(
						username = if (hasLocalUsernameDraft) current.username else preferences.username,
						receiveFolder = receiveFolder,
						themeMode = preferences.themeMode,
						notificationsEnabled = preferences.notificationsEnabled,
						diagnosticsEnabled = preferences.diagnosticsEnabled,
						savedRelaySettings = preferences.relaySettings,
						relayMode = if (hasLocalRelayDraft) current.relayMode else preferences.relaySettings.mode,
						relayUrls = if (hasLocalRelayDraft) {
							current.relayUrls
						} else {
							preferences.relaySettings.relayUrls
						},
					)
				}
				if (receiveFolder != previousFolder) {
					validateFolder(receiveFolder)
				}
			}
		}
		viewModelScope.launch {
			repository.state.collect { coreState ->
				val status = coreState.status
				val hasActiveWork = status?.let { it.activeTransfers > 0UL || it.activeShares > 0UL } == true ||
					coreState.transfers.any { it.status in ActiveTransferStatuses }
				_state.update {
					it.copy(
						hasActiveNetworkWork = hasActiveWork,
						endpointId = coreState.status?.endpointId,
					)
				}
			}
		}
		refreshNotificationPermission()
		loadDeviceInfo()
	}

	fun selectSection(section: SettingsSection) {
		_state.update { it.copy(selectedSection = section) }
		when (section) {
			SettingsSection.Storage -> loadStorageUsage()
			SettingsSection.About, SettingsSection.BugReport -> {
				loadDeviceInfo()
				if (section == SettingsSection.BugReport) refreshBugLogPreview()
			}
			else -> Unit
		}
	}

	fun loadStorageUsage() {
		if (_state.value.isCalculatingStorage) return
		viewModelScope.launch {
			_state.update { it.copy(isCalculatingStorage = true) }
			try {
				val receiveFolder = _state.value.receiveFolder ?: fileSystemService.defaultReceiveFolder()
				val coreUsage = repository.storageUsage().getOrThrow()
				val artifacts = repository.receivedArtifacts().getOrThrow()
				val received = fileSystemService.inspectReceivedArtifacts(artifacts)
				_state.update {
					it.copy(
						storage = StorageBreakdown(
							transferCacheBytes = coreUsage.blobStoreBytes,
							appDataBytes = coreUsage.appDataBytes,
							temporaryBytes = fileSystemService.temporaryUsage(receiveFolder),
							receivedBytes = received.existingBytes,
							receivedFileCount = received.existingCount,
							missingReceivedFileCount = received.missingCount,
							inaccessibleReceivedFileCount = received.inaccessibleCount,
						),
						isCalculatingStorage = false,
					)
				}
			} catch (error: CancellationException) {
				throw error
			} catch (error: Throwable) {
				_state.update { it.copy(isCalculatingStorage = false) }
				messages.error(error)
			}
		}
	}

	fun deleteAllTransfers() {
		if (_state.value.isDeletingTransfers || _state.value.isClearingTransferCache) return
		viewModelScope.launch {
			_state.update { it.copy(isDeletingTransfers = true) }
			val failures = repository.state.value.transfers
				.map { repository.delete(it.transferId) }
				.count { it.isFailure }
			val cleanup = repository.clearTransferCache()
			_state.update { it.copy(isDeletingTransfers = false) }
			loadStorageUsage()
			if (failures > 0) {
				messages.error(IllegalStateException("Could not delete $failures transfer records"))
			} else if (cleanup.isSuccess) {
				messages.tryShow(UiMessage(UiText.Resource(Res.string.storage_transfers_deleted), UiMessageTone.Success))
			}
			cleanup.onFailure(messages::error)
		}
	}

	fun clearTransferCache() {
		if (
			_state.value.isDeletingTransfers ||
			_state.value.isClearingTransferCache ||
			_state.value.hasActiveNetworkWork
		) return
		viewModelScope.launch {
			_state.update { it.copy(isClearingTransferCache = true) }
			repository.clearTransferCache().fold(
				onSuccess = {
					_state.update { it.copy(isClearingTransferCache = false) }
					loadStorageUsage()
					messages.tryShow(
						UiMessage(UiText.Resource(Res.string.storage_transfer_cache_cleared), UiMessageTone.Success),
					)
				},
				onFailure = { error ->
					_state.update { it.copy(isClearingTransferCache = false) }
					messages.error(error)
				},
			)
		}
	}

	fun setUsername(value: String) {
		hasLocalUsernameDraft = true
		_state.update { it.copy(username = value) }
		usernamePersistJob?.cancel()
		usernamePersistJob = viewModelScope.launch {
			delay(UsernamePersistDebounceMs)
			preferencesRepository.setUsername(value)
		}
	}

	fun setThemeMode(mode: ThemeMode) {
		viewModelScope.launch { preferencesRepository.setThemeMode(mode) }
	}

	fun setRelayMode(mode: RelayMode) {
		hasLocalRelayDraft = true
		_state.update {
			it.copy(
				relayMode = mode,
				relayUrls = if (mode.usesCustomRelayUrls && it.relayUrls.isEmpty()) listOf("") else it.relayUrls,
				relayInputError = null,
				relayApplyError = null,
			)
		}
		hasLocalRelayDraft = _state.value.hasRelaySettingsChanges
	}

	fun setRelayUrl(index: Int, value: String) {
		if (index !in _state.value.relayUrls.indices) return
		hasLocalRelayDraft = true
		_state.update {
			it.copy(
				relayUrls = it.relayUrls.toMutableList().apply { this[index] = value },
				relayInputError = null,
				relayApplyError = null,
			)
		}
		hasLocalRelayDraft = _state.value.hasRelaySettingsChanges
	}

	fun addRelayUrl() {
		if (_state.value.relayUrls.size >= MaximumRelayUrls) return
		hasLocalRelayDraft = true
		_state.update {
			it.copy(
				relayUrls = it.relayUrls + "",
				relayInputError = null,
				relayApplyError = null,
			)
		}
		hasLocalRelayDraft = _state.value.hasRelaySettingsChanges
	}

	fun removeRelayUrl(index: Int) {
		if (index !in _state.value.relayUrls.indices) return
		hasLocalRelayDraft = true
		_state.update {
			val remaining = it.relayUrls.toMutableList().apply { removeAt(index) }
			it.copy(
				relayUrls = remaining.ifEmpty { listOf("") },
				relayInputError = null,
				relayApplyError = null,
			)
		}
		hasLocalRelayDraft = _state.value.hasRelaySettingsChanges
	}

	fun applyRelaySettings() {
		val snapshot = _state.value
		if (snapshot.isApplyingRelaySettings || !snapshot.hasRelaySettingsChanges) return
		val validation = validateRelaySettings(
			mode = snapshot.relayMode,
			relayUrls = snapshot.relayUrls,
			retainedUrls = snapshot.savedRelaySettings.relayUrls,
		)
		val desired = validation.settings
		if (desired == null) {
			_state.update { it.copy(relayInputError = validation.error, relayApplyError = null) }
			return
		}
		if (snapshot.hasActiveNetworkWork) {
			_state.update {
				it.copy(relayInputError = null, relayApplyError = RelaySettingsApplyError.ActiveTransfers)
			}
			return
		}

		_state.update {
			it.copy(
				isApplyingRelaySettings = true,
				relayInputError = null,
				relayApplyError = null,
			)
		}
		viewModelScope.launch {
			val previous = snapshot.savedRelaySettings
			val applied = repository.initialize(environment.defaultCoreDataDir, desired)
			if (applied.isFailure) {
				val busy = applied.exceptionOrNull() is CoreLifecycleBusyException
				if (busy || repository.state.value.isInitialized) {
					_state.update {
						it.copy(
							isApplyingRelaySettings = false,
							relayApplyError = if (repository.state.value.isInitialized) {
								RelaySettingsApplyError.ActiveTransfers
							} else {
								RelaySettingsApplyError.ApplyFailed
							},
						)
					}
				} else {
					finishFailedRelayApply(previous)
				}
				return@launch
			}
			try {
				preferencesRepository.setRelaySettings(desired)
			} catch (error: CancellationException) {
				throw error
			} catch (_: Throwable) {
				finishFailedRelayApply(previous)
				return@launch
			}
			hasLocalRelayDraft = false
			_state.update {
				it.copy(
					savedRelaySettings = desired,
					relayMode = desired.mode,
					relayUrls = desired.relayUrls,
					isApplyingRelaySettings = false,
					relayInputError = null,
					relayApplyError = null,
				)
			}
			messages.show(
				UiMessage(UiText.Resource(Res.string.relay_settings_applied), UiMessageTone.Success),
			)
		}
	}

	fun chooseReceiveFolder() {
		if (!fileSystemService.supportsCustomReceiveFolders) return
		viewModelScope.launch { effects.send(SettingsEffect.OpenReceiveFolderPicker) }
	}

	fun onReceiveFolderPicked(folder: ReceiveFolder) {
		viewModelScope.launch { preferencesRepository.setReceiveFolder(folder) }
	}

	fun onReceiveFolderPickFailed(reason: String) = messages.error(IllegalStateException(reason))

	fun resetReceiveFolder() {
		viewModelScope.launch { preferencesRepository.resetReceiveFolder() }
	}

	fun setNotificationsEnabled(enabled: Boolean) {
		viewModelScope.launch {
			if (!enabled) {
				preferencesRepository.setNotificationsEnabled(false)
				notifications.cancelAll()
				return@launch
			}

			val permission = notifications.requestPermission()
			_state.update { it.copy(notificationPermission = permission) }
			if (permission == NotificationPermission.Granted) {
				enableNotifications()
			} else {
				preferencesRepository.setNotificationsEnabled(false)
				messages.show(
					UiMessage(
						UiText.Resource(
							if (permission == NotificationPermission.Unsupported) Res.string.notifications_unsupported
							else Res.string.notifications_permission_denied,
						),
						UiMessageTone.Warning,
						actionLabel = if (permission == NotificationPermission.Denied) {
							UiText.Resource(Res.string.button_open_settings)
						} else {
							null
						},
						onAction = if (permission == NotificationPermission.Denied) ::openNotificationSettings else null,
					),
				)
			}
		}
	}

	fun setDiagnosticsEnabled(enabled: Boolean) {
		if (!diagnosticsIncluded) return
		viewModelScope.launch {
			preferencesRepository.setDiagnosticsEnabled(enabled)
			diagnostics?.record(
				if (enabled) "diagnostics_enabled" else "diagnostics_disabled",
			)
			messages.show(
				UiMessage(
					UiText.Resource(
						if (enabled) Res.string.diagnostics_enabled_message
						else Res.string.diagnostics_disabled_message,
					),
					UiMessageTone.Success,
				),
			)
		}
	}

	fun setBugWhatHappened(value: String) = _state.update { it.copy(bugWhatHappened = value) }
	fun setBugExpected(value: String) = _state.update { it.copy(bugExpected = value) }
	fun setBugSteps(value: String) = _state.update { it.copy(bugSteps = value) }
	fun setBugContact(value: String) = _state.update { it.copy(bugContact = value) }
	fun setBugIncludeLogs(value: Boolean) = _state.update { it.copy(bugIncludeLogs = value) }

	fun submitBugReport() {
		if (_state.value.isSubmittingBugReport) return
		viewModelScope.launch {
			val snapshot = _state.value
			val what = snapshot.bugWhatHappened.trim()
			val expected = snapshot.bugExpected.trim()
			if (what.isEmpty()) {
				messages.show(UiMessage(UiText.Resource(Res.string.bug_report_missing_what), UiMessageTone.Warning))
				return@launch
			}
			if (expected.isEmpty()) {
				messages.show(UiMessage(UiText.Resource(Res.string.bug_report_missing_expected), UiMessageTone.Warning))
				return@launch
			}
			_state.update { it.copy(isSubmittingBugReport = true) }
			try {
				val result = bugReports.submit(
					BugReportDraft(
						whatHappened = what,
						expected = expected,
						steps = snapshot.bugSteps,
						contact = snapshot.bugContact,
						includeLogs = snapshot.bugIncludeLogs,
					),
					deviceInfo = snapshot.deviceInfo,
				)
				result.fold(
					onSuccess = {
						diagnostics?.record("bug_report_submitted")
						_state.update {
							it.copy(
								isSubmittingBugReport = false,
								bugWhatHappened = "",
								bugExpected = "",
								bugSteps = "",
								bugContact = "",
								bugIncludeLogs = true,
							)
						}
						messages.show(
							UiMessage(UiText.Resource(Res.string.bug_report_submitted), UiMessageTone.Success),
						)
						selectSection(SettingsSection.About)
					},
					onFailure = {
						_state.update { it.copy(isSubmittingBugReport = false) }
						messages.show(
							UiMessage(UiText.Resource(Res.string.bug_report_submit_failed), UiMessageTone.Error),
						)
					},
				)
			} catch (error: Throwable) {
				if (error is CancellationException) throw error
				_state.update { it.copy(isSubmittingBugReport = false) }
				messages.show(UiMessage(UiText.Resource(Res.string.bug_report_submit_failed), UiMessageTone.Error))
			}
		}
	}

	fun openNotificationSettings() {
		viewModelScope.launch {
			enableNotificationsAfterSettings = true
			notifications.openSettings().onFailure {
				enableNotificationsAfterSettings = false
				messages.show(
					UiMessage(UiText.Resource(Res.string.notifications_settings_open_failed), UiMessageTone.Error),
				)
			}
		}
	}

	fun refreshNotificationPermission() {
		viewModelScope.launch {
			val permission = notifications.refreshPermission()
			_state.update { it.copy(notificationPermission = permission) }
			if (enableNotificationsAfterSettings) {
				enableNotificationsAfterSettings = false
				if (permission == NotificationPermission.Granted) enableNotifications()
			} else if (permission != NotificationPermission.Granted && _state.value.notificationsEnabled) {
				preferencesRepository.setNotificationsEnabled(false)
				notifications.cancelAll()
			}
		}
	}

	private suspend fun enableNotifications() {
		preferencesRepository.setNotificationsEnabled(true)
		messages.show(
			UiMessage(UiText.Resource(Res.string.notifications_enabled_message), UiMessageTone.Success),
		)
	}

	private fun loadDeviceInfo() {
		if (_state.value.isLoadingDeviceInfo) return
		viewModelScope.launch {
			_state.update { it.copy(isLoadingDeviceInfo = true) }
			try {
				val info = deviceInfoProvider.load()
				_state.update { it.copy(deviceInfo = info, isLoadingDeviceInfo = false) }
			} catch (error: Throwable) {
				if (error is CancellationException) throw error
				_state.update { it.copy(isLoadingDeviceInfo = false) }
				messages.error(
					if (error.message.isNullOrBlank()) {
						IllegalStateException("Could not load device information.")
					} else {
						error
					},
				)
			}
		}
	}

	private fun refreshBugLogPreview() {
		viewModelScope.launch {
			val bytes = runCatching { bugReports.previewLogBytes() }.getOrDefault(0)
			_state.update { it.copy(bugLogPreviewBytes = bytes) }
		}
	}

	private suspend fun validateFolder(folder: ReceiveFolder) {
		_state.update { it.copy(isValidatingFolder = true) }
		val status = fileSystemService.validateReceiveFolder(folder)
		_state.update { it.copy(folderAccessStatus = status, isValidatingFolder = false) }
	}

	private suspend fun finishFailedRelayApply(previous: RelaySettings) {
		val restored = repository.initialize(environment.defaultCoreDataDir, previous).isSuccess
		_state.update {
			it.copy(
				isApplyingRelaySettings = false,
				relayApplyError = if (restored) {
					RelaySettingsApplyError.ApplyFailed
				} else {
					RelaySettingsApplyError.RestoreFailed
				},
			)
		}
	}

	private companion object {
		const val UsernamePersistDebounceMs = 350L
		val ActiveTransferStatuses = setOf(TransferStatus.Importing, TransferStatus.Sharing, TransferStatus.Receiving)
	}
}
