package com.vnidrop.app.feature.saveddevices

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.FileSystemService
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.core.TargetedOfferResponseModel
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessage
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiMessageTone
import com.vnidrop.app.ui.feedback.UiText
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.receive_completed
import vnidrop.shared.generated.resources.saved_devices_blocked
import vnidrop.shared.generated.resources.saved_devices_forgotten
import vnidrop.shared.generated.resources.saved_devices_labeled

data class SavedDevicesState(
	val isLoading: Boolean = true,
	val loadFailed: Boolean = false,
	val eligibilities: List<PairingEligibilityModel> = emptyList(),
	val pendingRelationships: List<DeviceRelationshipModel> = emptyList(),
	val savedDevices: List<SavedDeviceModel> = emptyList(),
	val targetedTransfers: List<SavedDeviceTransferItem> = emptyList(),
	val pairingPrompt: PairingPromptState = PairingPromptState(),
	val targetedOffers: TargetedOfferState = TargetedOfferState(),
	val busyPeerIds: Set<String> = emptySet(),
	val busyTransferIds: Set<String> = emptySet(),
	val labelingPeerId: String? = null,
	val labelDraft: String = "",
	val isSavingLabel: Boolean = false,
)

/**
 * Product-level Saved-device experience. Callers observe one snapshot and issue
 * named commands; pairing, direct offers, history, and receive destinations stay internal.
 */
class SavedDevicesViewModel(
	private val repository: CoreGateway,
	private val fileSystemService: FileSystemService,
	private val preferencesRepository: PreferencesRepository,
	private val messages: UiMessageController,
) : ViewModel() {
	private val _state = MutableStateFlow(SavedDevicesState())
	val state: StateFlow<SavedDevicesState> = _state.asStateFlow()

	private val refreshMutex = Mutex()
	private val readModel = SavedDevicesReadModel()
	private val dismissedEligibility = mutableSetOf<String>()
	private var receiveFolder: ReceiveFolder? = null

	init {
		viewModelScope.launch {
			combine(
				preferencesRepository.preferences,
				repository.state.map { it.isInitialized },
			) { preferences, initialized -> preferences.receiveFolder to initialized }
				.distinctUntilChanged()
				.collectLatest { (configuredFolder, initialized) ->
					receiveFolder = fileSystemService.effectiveReceiveFolder(configuredFolder)
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

	fun acceptPairingPrompt() {
		val prompt = _state.value.pairingPrompt.prompt ?: return
		if (_state.value.pairingPrompt.busy) return
		_state.update { it.copy(pairingPrompt = it.pairingPrompt.copy(busy = true)) }
		viewModelScope.launch {
			val result = when (prompt) {
				is PairingPrompt.Eligibility -> repository.requestSavedDevicePairing(prompt.peerEndpointId)
				is PairingPrompt.IncomingRequest -> repository.respondToDevicePairing(prompt.peerEndpointId, true)
			}
			_state.update { it.copy(pairingPrompt = it.pairingPrompt.copy(busy = false)) }
			result.fold(onSuccess = { refresh() }, onFailure = messages::error)
		}
	}

	fun declinePairingPrompt() {
		val prompt = _state.value.pairingPrompt.prompt ?: return
		if (_state.value.pairingPrompt.busy) return
		_state.update { it.copy(pairingPrompt = it.pairingPrompt.copy(busy = true)) }
		viewModelScope.launch {
			val result = when (prompt) {
				is PairingPrompt.Eligibility -> repository.declinePairingEligibility(prompt.peerEndpointId)
				is PairingPrompt.IncomingRequest -> repository.respondToDevicePairing(prompt.peerEndpointId, false)
			}
			_state.update { it.copy(pairingPrompt = it.pairingPrompt.copy(busy = false)) }
			result.fold(onSuccess = { refresh() }, onFailure = messages::error)
		}
	}

	fun dismissPairingPrompt() {
		val prompt = _state.value.pairingPrompt.prompt ?: return
		if (prompt is PairingPrompt.Eligibility) dismissedEligibility += prompt.peerEndpointId
		_state.update { it.copy(pairingPrompt = it.pairingPrompt.copy(prompt = null)) }
	}

	fun acceptTargetedOffer(transferId: String) = respondToTargetedOffer(transferId, accepted = true)

	fun declineTargetedOffer(transferId: String) = respondToTargetedOffer(transferId, accepted = false)

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

	fun receiveTargetedTransfer(transferId: String) = runTransfer(transferId, resume = false)

	fun resumeTargetedTransfer(transferId: String) = runTransfer(transferId, resume = true)

	fun cancelTargetedTransfer(transferId: String) = mutateTransfer(transferId) {
		repository.cancelTargetedTransfer(transferId)
	}

	fun deleteTargetedTransfer(transferId: String) = mutateTransfer(transferId) {
		repository.deleteTargetedTransfer(transferId)
	}

	fun openLabelEditor(peerEndpointId: String) {
		if (_state.value.isSavingLabel || peerEndpointId in _state.value.busyPeerIds) return
		val current = _state.value.savedDevices.firstOrNull { it.endpointId == peerEndpointId }
		_state.update {
			it.copy(labelingPeerId = peerEndpointId, labelDraft = current?.localLabel.orEmpty())
		}
	}

	fun setLabelDraft(value: String) = _state.update {
		if (it.isSavingLabel) it else it.copy(labelDraft = value)
	}

	fun dismissLabelEditor() {
		if (!_state.value.isSavingLabel) {
			_state.update { it.copy(labelingPeerId = null, labelDraft = "") }
		}
	}

	fun saveLabel() {
		commitLabel(_state.value.labelDraft.trim().ifBlank { null })
	}

	fun clearLabel() {
		commitLabel(null)
	}

	private fun commitLabel(label: String?) {
		val peerId = _state.value.labelingPeerId ?: return
		if (_state.value.isSavingLabel || peerId in _state.value.busyPeerIds) return
		_state.update { it.copy(isSavingLabel = true, busyPeerIds = it.busyPeerIds + peerId) }
		viewModelScope.launch {
			val result = repository.setSavedDeviceLabel(peerId, label)
			if (result.isSuccess) {
				refresh()
				messages.tryShow(UiMessage(UiText.Resource(Res.string.saved_devices_labeled), UiMessageTone.Success))
			} else {
				result.exceptionOrNull()?.let(messages::error)
			}
			_state.update { current ->
				val editorMatches = current.labelingPeerId == peerId
				current.copy(
					busyPeerIds = current.busyPeerIds - peerId,
					isSavingLabel = false,
					labelingPeerId = if (result.isSuccess && editorMatches) null else current.labelingPeerId,
					labelDraft = if (result.isSuccess && editorMatches) "" else current.labelDraft,
				)
			}
		}
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

	private fun respondToTargetedOffer(transferId: String, accepted: Boolean) {
		if (transferId in _state.value.targetedOffers.respondingIds) return
		_state.update {
			it.copy(targetedOffers = it.targetedOffers.copy(respondingIds = it.targetedOffers.respondingIds + transferId))
		}
		viewModelScope.launch {
			val response = repository.respondToTargetedOffer(transferId, accepted)
			response.fold(
				onSuccess = { result ->
					if (accepted && result is TargetedOfferResponseModel.Approved) {
						pullTargetedTransfer(result.transferId, resume = false).onFailure(messages::error)
					}
					refresh()
				},
				onFailure = messages::error,
			)
			_state.update {
				it.copy(targetedOffers = it.targetedOffers.copy(respondingIds = it.targetedOffers.respondingIds - transferId))
			}
		}
	}

	private fun runTransfer(transferId: String, resume: Boolean) = mutateTransfer(transferId) {
		pullTargetedTransfer(transferId, resume)
	}

	private suspend fun pullTargetedTransfer(transferId: String, resume: Boolean): Result<Unit> {
		val folder = receiveFolder ?: return Result.failure(IllegalStateException())
		val sink = fileSystemService.createReceiveOutputSink(folder)
		val result = when {
			resume && sink != null -> repository.resumeTargetedTransferWithOutputSinkV2(transferId, sink)
			resume -> repository.resumeTargetedTransfer(transferId, folder.value)
			sink != null -> repository.receiveTargetedTransferWithOutputSinkV2(transferId, sink)
			else -> repository.receiveTargetedTransfer(transferId, folder.value)
		}
		result.onSuccess {
			messages.tryShow(UiMessage(UiText.Resource(Res.string.receive_completed), UiMessageTone.Success))
		}
		return result
	}

	private fun mutatePeer(peerEndpointId: String, block: suspend () -> Result<*>) {
		if (peerEndpointId in _state.value.busyPeerIds) return
		_state.update { it.copy(busyPeerIds = it.busyPeerIds + peerEndpointId) }
		viewModelScope.launch {
			block().fold(onSuccess = { refresh() }, onFailure = messages::error)
			_state.update { it.copy(busyPeerIds = it.busyPeerIds - peerEndpointId) }
		}
	}

	private fun mutateTransfer(transferId: String, block: suspend () -> Result<Unit>) {
		if (transferId in _state.value.busyTransferIds) return
		_state.update { it.copy(busyTransferIds = it.busyTransferIds + transferId) }
		viewModelScope.launch {
			block().fold(onSuccess = { refresh() }, onFailure = messages::error)
			_state.update { it.copy(busyTransferIds = it.busyTransferIds - transferId) }
		}
	}

	private suspend fun refresh() {
		refreshMutex.withLock {
			_state.update { it.copy(isLoading = true, loadFailed = false) }
			val eligibilities = repository.listPairingEligibilities().getOrElse {
				refreshFailed(it)
				return@withLock
			}
			val relationships = repository.listDeviceRelationships().getOrElse {
				refreshFailed(it)
				return@withLock
			}
			val savedDevices = repository.listSavedDevices().getOrElse {
				refreshFailed(it)
				return@withLock
			}
			val pendingOffers = repository.listPendingTargetedOffers().getOrElse {
				refreshFailed(it)
				return@withLock
			}
			val targetedTransfers = repository.listTargetedTransfers().getOrElse {
				refreshFailed(it)
				return@withLock
			}
			val snapshot = readModel.derive(
				SavedDevicesReadInputs(
					eligibilities = eligibilities,
					relationships = relationships,
					savedDevices = savedDevices,
					pendingOffers = pendingOffers,
					targetedTransfers = targetedTransfers,
				),
				dismissedEligibilityIds = dismissedEligibility,
			)
			val pairingPrompt = if (_state.value.pairingPrompt.busy) {
				_state.value.pairingPrompt
			} else {
				PairingPromptState(prompt = snapshot.nextPairingPrompt)
			}
			_state.update {
				it.copy(
					isLoading = false,
					loadFailed = false,
					eligibilities = snapshot.eligibilities,
					pendingRelationships = snapshot.pendingRelationships,
					savedDevices = snapshot.savedDevices,
					targetedTransfers = snapshot.targetedTransfers,
					pairingPrompt = pairingPrompt,
					targetedOffers = it.targetedOffers.copy(
						pending = snapshot.pendingOffers,
						senderDisplayNames = snapshot.senderDisplayNames,
					),
				)
			}
		}
	}

	private fun refreshFailed(error: Throwable) {
		_state.update { it.copy(isLoading = false, loadFailed = true) }
		messages.error(error)
	}

}
