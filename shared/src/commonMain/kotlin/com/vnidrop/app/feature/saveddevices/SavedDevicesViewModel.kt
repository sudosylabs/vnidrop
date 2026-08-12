package com.vnidrop.app.feature.saveddevices

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.ui.feedback.UiMessage
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiMessageTone
import com.vnidrop.app.ui.feedback.UiText
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.saved_devices_blocked
import vnidrop.shared.generated.resources.saved_devices_forgotten
import vnidrop.shared.generated.resources.saved_devices_labeled

data class SavedDevicesState(
	val isLoading: Boolean = true,
	val loadFailed: Boolean = false,
	val eligibilities: List<PairingEligibilityModel> = emptyList(),
	val pendingRelationships: List<DeviceRelationshipModel> = emptyList(),
	val savedDevices: List<SavedDeviceModel> = emptyList(),
	val busyPeerIds: Set<String> = emptySet(),
	val labelingPeerId: String? = null,
	val labelDraft: String = "",
)

class SavedDevicesViewModel(
	private val repository: CoreGateway,
	private val messages: UiMessageController,
) : ViewModel() {
	private val _state = MutableStateFlow(SavedDevicesState())
	val state: StateFlow<SavedDevicesState> = _state.asStateFlow()

	init {
		viewModelScope.launch {
			repository.state.map { it.isInitialized }
				.distinctUntilChanged()
				.collect { initialized ->
					if (initialized) refresh()
				}
		}
		viewModelScope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					CoreSignal.PairingChanged,
					CoreSignal.TargetedTransferChanged -> {
						if (repository.state.value.isInitialized) refresh()
					}
					is CoreSignal.ApprovalChanged,
					is CoreSignal.ReceiverHistoryChanged,
					is CoreSignal.TransfersChanged -> Unit
				}
			}
		}
	}

	fun retry() {
		if (!repository.state.value.isInitialized || _state.value.isLoading) return
		viewModelScope.launch { refresh() }
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
		_state.update { it.copy(isLoading = true, loadFailed = false) }
		val eligibilities = repository.listPairingEligibilities().getOrElse {
			_state.update { state -> state.copy(isLoading = false, loadFailed = true) }
			messages.error(it)
			return
		}
		val relationships = repository.listDeviceRelationships().getOrElse {
			_state.update { state -> state.copy(isLoading = false, loadFailed = true) }
			messages.error(it)
			return
		}
		val saved = repository.listSavedDevices().getOrElse {
			_state.update { state -> state.copy(isLoading = false, loadFailed = true) }
			messages.error(it)
			return
		}
		_state.update {
			it.copy(
				isLoading = false,
				loadFailed = false,
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
