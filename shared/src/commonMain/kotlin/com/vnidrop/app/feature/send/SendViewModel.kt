package com.vnidrop.app.feature.send

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.ui.feedback.UiMessage
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiMessageTone
import com.vnidrop.app.ui.feedback.UiText
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.transfer_deleted
import vnidrop.shared.generated.resources.transfer_invitation_saved
import vnidrop.shared.generated.resources.transfer_nfc_written

data class SendState(
	val selectedTransferId: ULong? = null,
	val transferThumbnails: Map<ULong, ByteArray> = emptyMap(),
	val detailPanel: TransferDetailPanel? = null,
	val receiverHistory: List<ReceiverRequestModel> = emptyList(),
	val receiversByTransfer: Map<ULong, List<ReceiverRequestModel>> = emptyMap(),
	val isLoadingReceivers: Boolean = false,
	val isDeleteConfirmationOpen: Boolean = false,
	val deleteTargetTransferId: ULong? = null,
	val isDeleting: Boolean = false,
)

enum class TransferDetailPanel { Activity, Receivers, Share }

sealed interface SendEffect {
	data class CopyTicket(val ticket: String) : SendEffect
}

class SendViewModel(
	private val repository: CoreGateway,
	private val filePreviewRepository: FilePreviewRepository,
	private val messages: UiMessageController,
) : ViewModel() {
	private val _state = MutableStateFlow(SendState())
	val state: StateFlow<SendState> = _state.asStateFlow()
	val coreState = repository.state

	private val effects = Channel<SendEffect>(Channel.BUFFERED)
	val effectFlow = effects.receiveAsFlow()

	init {
		viewModelScope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					is CoreSignal.TransfersChanged -> repository.refresh()
					is CoreSignal.ReceiverHistoryChanged -> {
						if (signal.transferId == _state.value.selectedTransferId) {
							refreshReceivers(signal.transferId)
						}
						refreshReceiverStatuses(signal.transferId)
					}
					is CoreSignal.ApprovalChanged -> {
						if (signal.transferId == _state.value.selectedTransferId) {
							refreshReceivers(signal.transferId)
						}
						refreshReceiverStatuses(signal.transferId)
					}
					CoreSignal.PairingChanged,
					CoreSignal.TargetedTransferChanged -> Unit
				}
			}
		}
		viewModelScope.launch {
			coreState.map { core ->
				core.transfers
					.filter { it.direction == TransferDirection.Send && it.status in setOf(TransferStatus.Importing, TransferStatus.Sharing) }
					.mapTo(mutableSetOf()) { it.transferId }
			}.distinctUntilChanged().collect(::syncSharingReceivers)
		}
		viewModelScope.launch {
			filePreviewRepository.previews.collect { previews ->
				_state.update { it.copy(transferThumbnails = previews) }
			}
		}
		viewModelScope.launch {
			coreState.map { core ->
				core.takeIf { it.isInitialized }?.transfers?.map { it.transferId }?.toSet()
			}.distinctUntilChanged().collect { activeIds ->
				if (activeIds != null) filePreviewRepository.restore(activeIds)
			}
		}
	}

	fun onDraftCreated(creation: TransferDraftCreation) {
		if (creation !is TransferDraftCreation.Invitation) return
		_state.update {
			it.copy(
				selectedTransferId = creation.transferId,
				detailPanel = TransferDetailPanel.Share,
			)
		}
		refreshReceivers(creation.transferId)
	}
	fun openTransfer(transferId: ULong) {
		_state.update { it.copy(selectedTransferId = transferId, detailPanel = null) }
		refreshReceivers(transferId)
	}
	fun closeTransferDetails() = _state.update {
		it.copy(
			selectedTransferId = null,
			detailPanel = null,
			receiverHistory = emptyList(),
			isDeleteConfirmationOpen = false,
		)
	}
	fun openActivity() = _state.update { it.copy(detailPanel = TransferDetailPanel.Activity) }
	fun openShare() {
		val selectedId = _state.value.selectedTransferId ?: return
		val selected = coreState.value.transfers.firstOrNull { it.transferId == selectedId } ?: return
		if (selected.status !in setOf(TransferStatus.Importing, TransferStatus.Sharing)) return
		_state.update { it.copy(detailPanel = TransferDetailPanel.Share) }
	}
	fun openReceivers() {
		val transferId = _state.value.selectedTransferId ?: return
		_state.update { it.copy(detailPanel = TransferDetailPanel.Receivers) }
		refreshReceivers(transferId)
	}
	fun closeDetailPanel() = _state.update { it.copy(detailPanel = null) }
	fun requestDeleteTransfer(transferId: ULong? = null) = _state.update {
		it.copy(
			isDeleteConfirmationOpen = true,
			deleteTargetTransferId = transferId ?: it.selectedTransferId,
		)
	}
	fun dismissDeleteTransfer() {
		if (!_state.value.isDeleting) {
			_state.update { it.copy(isDeleteConfirmationOpen = false, deleteTargetTransferId = null) }
		}
	}
	fun confirmDeleteTransfer() {
		val transferId = _state.value.deleteTargetTransferId ?: return
		if (_state.value.isDeleting) return
		viewModelScope.launch {
			_state.update { it.copy(isDeleting = true) }
			repository.delete(transferId).fold(
				onSuccess = {
					filePreviewRepository.remove(transferId)
					_state.update {
						it.copy(
							selectedTransferId = null,
							detailPanel = null,
							receiverHistory = emptyList(),
							isDeleteConfirmationOpen = false,
							deleteTargetTransferId = null,
							isDeleting = false,
						)
					}
					messages.tryShow(UiMessage(UiText.Resource(Res.string.transfer_deleted), UiMessageTone.Success))
				},
				onFailure = { error ->
					_state.update { it.copy(isDeleting = false) }
					messages.error(error)
				},
			)
		}
	}
	fun copyTicket(ticket: String) = sendEffect(SendEffect.CopyTicket(ticket))
	fun onInvitationResult(action: InvitationAction, result: Result<Unit>) {
		result.fold(
			onSuccess = {
				val message = when (action) {
					InvitationAction.Export -> Res.string.transfer_invitation_saved
					InvitationAction.Nfc -> Res.string.transfer_nfc_written
					// System share sheet already confirms the action on most platforms.
					InvitationAction.Share -> null
				}
				message?.let { messages.tryShow(UiMessage(UiText.Resource(it), UiMessageTone.Success)) }
			},
			onFailure = messages::error,
		)
	}

	fun stopSharing(transferId: ULong) {
		viewModelScope.launch {
			repository.cancel(transferId).fold(
				onSuccess = { repository.refresh() },
				onFailure = messages::error,
			)
		}
	}

	private fun sendEffect(effect: SendEffect) {
		viewModelScope.launch { effects.send(effect) }
	}

	private fun refreshReceivers(transferId: ULong) {
		viewModelScope.launch {
			_state.update { it.copy(isLoadingReceivers = true) }
			repository.receiverRequests(transferId).fold(
				onSuccess = { requests -> _state.update { it.copy(receiverHistory = requests, isLoadingReceivers = false) } },
				onFailure = { error ->
					_state.update { it.copy(isLoadingReceivers = false) }
					messages.error(error)
				},
			)
		}
	}

	private fun syncSharingReceivers(transferIds: Set<ULong>) {
		_state.update { current ->
			current.copy(receiversByTransfer = current.receiversByTransfer.filterKeys { it in transferIds })
		}
		transferIds.forEach(::refreshReceiverStatuses)
	}

	private fun refreshReceiverStatuses(transferId: ULong) {
		viewModelScope.launch {
			repository.receiverRequests(transferId).onSuccess { requests ->
				_state.update { current ->
					current.copy(receiversByTransfer = current.receiversByTransfer + (transferId to requests))
				}
			}
		}
	}
}
