package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.FileSystemService
import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.TargetedOfferResponseModel
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessage
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiMessageTone
import com.vnidrop.app.ui.feedback.UiText
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.receive_completed

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

/**
 * Foreground interrupt for pending targeted offers. Approve pulls by transfer id
 * through the platform receive destination (MediaStore Downloads sink on Android,
 * filesystem path on desktop when no sink is available).
 */
class TargetedOfferCoordinator(
	private val repository: CoreGateway,
	private val fileSystemService: FileSystemService,
	private val preferencesRepository: PreferencesRepository,
	private val messages: UiMessageController,
	private val scope: CoroutineScope,
) {
	private val _state = MutableStateFlow(TargetedOfferState())
	val state: StateFlow<TargetedOfferState> = _state.asStateFlow()

	private var receiveFolder: ReceiveFolder? = null

	init {
		scope.launch {
			// Same startup race as PairingPromptCoordinator: prefs can load while
			// core initialize still holds the lifecycle gate / core is null.
			combine(
				preferencesRepository.preferences,
				repository.state.map { it.isInitialized },
			) { preferences, initialized -> preferences to initialized }
				.distinctUntilChanged()
				.collectLatest { (preferences, initialized) ->
					receiveFolder = fileSystemService.effectiveReceiveFolder(preferences.receiveFolder)
					if (initialized) refresh()
				}
		}
		scope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					CoreSignal.TargetedTransferChanged -> {
						if (repository.state.value.isInitialized) refresh()
					}
					CoreSignal.PairingChanged,
					is CoreSignal.ApprovalChanged,
					is CoreSignal.ReceiverHistoryChanged,
					is CoreSignal.TransfersChanged -> Unit
				}
			}
		}
	}

	fun accept(transferId: String) = respond(transferId, accepted = true)

	fun decline(transferId: String) = respond(transferId, accepted = false)

	private fun respond(transferId: String, accepted: Boolean) {
		if (transferId in _state.value.respondingIds) return
		_state.update { it.copy(respondingIds = it.respondingIds + transferId) }
		scope.launch {
			val result = repository.respondToTargetedOffer(transferId, accepted)
			result.fold(
				onSuccess = { response ->
					if (accepted && response is TargetedOfferResponseModel.Approved) {
						pull(response.transferId)
					}
					refresh()
				},
				onFailure = messages::error,
			)
			_state.update { it.copy(respondingIds = it.respondingIds - transferId) }
		}
	}

	private suspend fun pull(transferId: String) {
		val folder = receiveFolder ?: return
		val sink = fileSystemService.createReceiveOutputSink(folder)
		val result = if (sink != null) {
			repository.receiveTargetedTransferWithOutputSinkV2(transferId, sink)
		} else {
			repository.receiveTargetedTransfer(transferId, folder.value)
		}
		result.fold(
			onSuccess = {
				messages.tryShow(
					UiMessage(UiText.Resource(Res.string.receive_completed), UiMessageTone.Success),
				)
			},
			onFailure = messages::error,
		)
	}

	private suspend fun refresh() {
		val offers = repository.listPendingTargetedOffers().getOrElse {
			messages.error(it)
			return
		}
		val savedDevices = repository.listSavedDevices().getOrElse {
			messages.error(it)
			return
		}
		_state.update {
			it.copy(
				pending = offers.sortedBy(PendingTargetedOfferModel::receivedAt),
				senderDisplayNames = savedDevices.associate { device ->
					device.endpointId to (
						device.localLabel?.takeIf(String::isNotBlank)
							?: device.remoteDisplayName?.takeIf(String::isNotBlank)
					).orEmpty()
				}.filterValues(String::isNotBlank),
			)
		}
	}
}
