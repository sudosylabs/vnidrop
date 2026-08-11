package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

sealed interface PairingPrompt {
	/** Local eligibility after a completed invitation transfer — user may remember the peer. */
	data class Eligibility(val peerEndpointId: String) : PairingPrompt

	/** Peer requested pairing; user may accept or decline mutual consent. */
	data class IncomingRequest(val peerEndpointId: String) : PairingPrompt
}

data class PairingPromptState(
	val enabled: Boolean = false,
	val prompt: PairingPrompt? = null,
	val busy: Boolean = false,
)

/**
 * Foreground in-flow pairing prompts. Dismiss keeps durable eligibility /
 * pending relationship visible in the Saved devices area; Decline consumes it.
 */
class PairingPromptCoordinator(
	private val repository: CoreGateway,
	private val preferencesRepository: PreferencesRepository,
	private val messages: UiMessageController,
	private val scope: CoroutineScope,
) {
	private val _state = MutableStateFlow(PairingPromptState())
	val state: StateFlow<PairingPromptState> = _state.asStateFlow()

	/** Peers whose in-flow eligibility prompt was dismissed this session. */
	private val dismissedEligibility = mutableSetOf<String>()

	init {
		scope.launch {
			preferencesRepository.preferences.collectLatest { preferences ->
				val enabled = preferences.experimentalSavedDevicesEnabled
				_state.update { it.copy(enabled = enabled) }
				if (enabled) {
					refresh()
				} else {
					_state.update { it.copy(prompt = null, busy = false) }
				}
			}
		}
		scope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					CoreSignal.PairingChanged -> if (_state.value.enabled) refresh()
					is CoreSignal.ApprovalChanged,
					is CoreSignal.ReceiverHistoryChanged,
					is CoreSignal.TransfersChanged,
					CoreSignal.TargetedTransferChanged -> Unit
				}
			}
		}
	}

	fun accept() {
		val prompt = _state.value.prompt ?: return
		if (_state.value.busy) return
		_state.update { it.copy(busy = true) }
		scope.launch {
			val result = when (prompt) {
				is PairingPrompt.Eligibility -> repository.requestSavedDevicePairing(prompt.peerEndpointId)
				is PairingPrompt.IncomingRequest ->
					repository.respondToDevicePairing(prompt.peerEndpointId, accepted = true)
			}
			_state.update { it.copy(busy = false) }
			result.fold(
				onSuccess = { refresh() },
				onFailure = messages::error,
			)
		}
	}

	fun decline() {
		val prompt = _state.value.prompt ?: return
		if (_state.value.busy) return
		_state.update { it.copy(busy = true) }
		scope.launch {
			val result = when (prompt) {
				is PairingPrompt.Eligibility -> repository.declinePairingEligibility(prompt.peerEndpointId)
				is PairingPrompt.IncomingRequest ->
					repository.respondToDevicePairing(prompt.peerEndpointId, accepted = false)
			}
			_state.update { it.copy(busy = false) }
			result.fold(
				onSuccess = { refresh() },
				onFailure = messages::error,
			)
		}
	}

	/** Close the dialog without consuming durable eligibility / pending state. */
	fun dismiss() {
		val prompt = _state.value.prompt ?: return
		if (prompt is PairingPrompt.Eligibility) {
			dismissedEligibility += prompt.peerEndpointId
		}
		_state.update { it.copy(prompt = null) }
	}

	private suspend fun refresh() {
		if (!_state.value.enabled || _state.value.busy) return
		val relationships = repository.listDeviceRelationships().getOrElse {
			messages.error(it)
			return
		}
		val incoming = relationships.firstOrNull { it.state == DeviceRelationshipStateModel.PendingIncoming }
		if (incoming != null) {
			_state.update {
				it.copy(prompt = PairingPrompt.IncomingRequest(incoming.remoteEndpointId))
			}
			return
		}
		val eligibilities = repository.listPairingEligibilities().getOrElse {
			messages.error(it)
			return
		}
		val eligibility = eligibilities.firstOrNull { it.peerEndpointId !in dismissedEligibility }
		_state.update {
			it.copy(prompt = eligibility?.let { row -> PairingPrompt.Eligibility(row.peerEndpointId) })
		}
	}
}
