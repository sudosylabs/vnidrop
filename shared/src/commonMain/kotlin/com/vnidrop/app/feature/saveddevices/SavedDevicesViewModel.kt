package com.vnidrop.app.feature.saveddevices

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.FileSystemService
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.PickedShareFile
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessage
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiMessageTone
import com.vnidrop.app.ui.feedback.UiText
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.saved_devices_blocked
import vnidrop.shared.generated.resources.saved_devices_forgotten
import vnidrop.shared.generated.resources.saved_devices_labeled
import vnidrop.shared.generated.resources.saved_devices_send_started

data class SavedDevicesState(
	val enabled: Boolean = false,
	val eligibilities: List<PairingEligibilityModel> = emptyList(),
	val pendingRelationships: List<DeviceRelationshipModel> = emptyList(),
	val savedDevices: List<SavedDeviceModel> = emptyList(),
	val busyPeerIds: Set<String> = emptySet(),
	val labelingPeerId: String? = null,
	val labelDraft: String = "",
	val sendTargetPeerId: String? = null,
	val isSending: Boolean = false,
)

sealed interface SavedDevicesEffect {
	data object OpenFilePicker : SavedDevicesEffect
}

class SavedDevicesViewModel(
	private val repository: CoreGateway,
	private val fileSystemService: FileSystemService,
	preferencesRepository: PreferencesRepository,
	private val messages: UiMessageController,
) : ViewModel() {
	private val _state = MutableStateFlow(SavedDevicesState())
	val state: StateFlow<SavedDevicesState> = _state.asStateFlow()

	private val effects = Channel<SavedDevicesEffect>(Channel.BUFFERED)
	val effectFlow = effects.receiveAsFlow()

	init {
		viewModelScope.launch {
			preferencesRepository.preferences.collectLatest { preferences ->
				val enabled = preferences.experimentalSavedDevicesEnabled
				_state.update { it.copy(enabled = enabled) }
				if (enabled) refresh() else _state.update {
					SavedDevicesState(enabled = false)
				}
			}
		}
		viewModelScope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					CoreSignal.PairingChanged,
					CoreSignal.TargetedTransferChanged -> if (_state.value.enabled) refresh()
					is CoreSignal.ApprovalChanged,
					is CoreSignal.ReceiverHistoryChanged,
					is CoreSignal.TransfersChanged -> Unit
				}
			}
		}
	}

	fun rememberEligible(peerEndpointId: String) = mutatePeer(peerEndpointId) {
		repository.requestSavedDevicePairing(peerEndpointId).map { }
	}

	fun declineEligible(peerEndpointId: String) = mutatePeer(peerEndpointId) {
		repository.declinePairingEligibility(peerEndpointId)
	}

	fun acceptIncoming(peerEndpointId: String) = mutatePeer(peerEndpointId) {
		repository.respondToDevicePairing(peerEndpointId, accepted = true).map { }
	}

	fun declineIncoming(peerEndpointId: String) = mutatePeer(peerEndpointId) {
		repository.respondToDevicePairing(peerEndpointId, accepted = false).map { }
	}

	fun openLabelEditor(peerEndpointId: String) {
		val current = _state.value.savedDevices.firstOrNull { it.endpointId == peerEndpointId }
		_state.update {
			it.copy(labelingPeerId = peerEndpointId, labelDraft = current?.localLabel.orEmpty())
		}
	}

	fun setLabelDraft(value: String) = _state.update { it.copy(labelDraft = value) }

	fun dismissLabelEditor() {
		if (_state.value.labelingPeerId !in _state.value.busyPeerIds) {
			_state.update { it.copy(labelingPeerId = null, labelDraft = "") }
		}
	}

	fun saveLabel() {
		val peerId = _state.value.labelingPeerId ?: return
		val label = _state.value.labelDraft.trim().ifBlank { null }
		mutatePeer(peerId) {
			repository.setSavedDeviceLabel(peerId, label).onSuccess {
				messages.tryShow(UiMessage(UiText.Resource(Res.string.saved_devices_labeled), UiMessageTone.Success))
			}
		}
		_state.update { it.copy(labelingPeerId = null, labelDraft = "") }
	}

	fun clearLabel() {
		val peerId = _state.value.labelingPeerId ?: return
		mutatePeer(peerId) {
			repository.setSavedDeviceLabel(peerId, null).onSuccess {
				messages.tryShow(UiMessage(UiText.Resource(Res.string.saved_devices_labeled), UiMessageTone.Success))
			}
		}
		_state.update { it.copy(labelingPeerId = null, labelDraft = "") }
	}

	fun forget(peerEndpointId: String) = mutatePeer(peerEndpointId) {
		repository.forgetSavedDevice(peerEndpointId).onSuccess {
			messages.tryShow(UiMessage(UiText.Resource(Res.string.saved_devices_forgotten), UiMessageTone.Success))
		}
	}

	fun block(peerEndpointId: String) = mutatePeer(peerEndpointId) {
		repository.blockDevice(peerEndpointId).onSuccess {
			messages.tryShow(UiMessage(UiText.Resource(Res.string.saved_devices_blocked), UiMessageTone.Success))
		}
	}

	fun startSend(peerEndpointId: String) {
		if (_state.value.isSending) return
		_state.update { it.copy(sendTargetPeerId = peerEndpointId) }
		viewModelScope.launch { effects.send(SavedDevicesEffect.OpenFilePicker) }
	}

	fun onFilesPicked(files: List<PickedShareFile>) {
		val peerId = _state.value.sendTargetPeerId ?: return
		if (files.isEmpty() || _state.value.isSending) return
		viewModelScope.launch {
			_state.update { it.copy(isSending = true) }
			val transferName = when {
				files.size == 1 -> files.first().displayName
				files.all { it.isDirectory } -> "${files.size} folders"
				else -> "${files.size} files"
			}
			val result = fileSystemService.createTargetedTransferFromPickedFiles(
				repository = repository,
				receiverEndpointId = peerId,
				files = files,
				transferName = transferName,
			)
			if (result.isSuccess) fileSystemService.discardPickedFiles(files)
			_state.update { it.copy(isSending = false, sendTargetPeerId = null) }
			result.fold(
				onSuccess = {
					messages.tryShow(
						UiMessage(UiText.Resource(Res.string.saved_devices_send_started), UiMessageTone.Success),
					)
				},
				onFailure = messages::error,
			)
		}
	}

	fun onFilePickFailed(reason: String) {
		_state.update { it.copy(sendTargetPeerId = null) }
		messages.error(IllegalStateException(reason.ifBlank { "selection failed" }))
	}

	private fun mutatePeer(peerEndpointId: String, block: suspend () -> Result<*>) {
		if (peerEndpointId in _state.value.busyPeerIds) return
		_state.update { it.copy(busyPeerIds = it.busyPeerIds + peerEndpointId) }
		viewModelScope.launch {
			block().fold(
				onSuccess = { refresh() },
				onFailure = messages::error,
			)
			_state.update { it.copy(busyPeerIds = it.busyPeerIds - peerEndpointId) }
		}
	}

	private suspend fun refresh() {
		if (!_state.value.enabled) return
		val eligibilities = repository.listPairingEligibilities().getOrElse {
			messages.error(it)
			return
		}
		val relationships = repository.listDeviceRelationships().getOrElse {
			messages.error(it)
			return
		}
		val saved = repository.listSavedDevices().getOrElse {
			messages.error(it)
			return
		}
		_state.update {
			it.copy(
				eligibilities = eligibilities.sortedByDescending(PairingEligibilityModel::createdAt),
				pendingRelationships = relationships.filter {
					it.state == DeviceRelationshipStateModel.PendingIncoming ||
						it.state == DeviceRelationshipStateModel.PendingOutgoing
				}.sortedByDescending(DeviceRelationshipModel::updatedAt),
				savedDevices = saved.sortedByDescending(SavedDeviceModel::createdAt),
			)
		}
	}
}
