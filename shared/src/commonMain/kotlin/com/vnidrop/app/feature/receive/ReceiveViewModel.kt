package com.vnidrop.app.feature.receive

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.FileSystemService
import com.vnidrop.app.core.FolderAccessStatus
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.core.TicketInspectionModel
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessage
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiMessageTone
import com.vnidrop.app.ui.feedback.UiText
import com.vnidrop.app.ui.feedback.canRetryWithoutChangingInput
import com.vnidrop.app.ui.feedback.isUserCancellation
import com.vnidrop.app.ui.feedback.toUiText
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_retry
import vnidrop.shared.generated.resources.button_show_in_files
import vnidrop.shared.generated.resources.error_invitation_empty
import vnidrop.shared.generated.resources.receive_open_files_failed
import vnidrop.shared.generated.resources.receive_completed
import vnidrop.shared.generated.resources.receive_history_cleared
import vnidrop.shared.generated.resources.transfer_deleted

sealed interface ReceiveHistoryDeleteTarget {
	data class Transfer(val transferId: ULong) : ReceiveHistoryDeleteTarget
	data object All : ReceiveHistoryDeleteTarget
}

data class ReceiveState(
	val isAcquisitionOpen: Boolean = false,
	val ticket: String = "",
	val method: ReceiveMethod? = null,
	val inspection: TicketInspectionModel? = null,
	val receiverName: String = "",
	val receiveFolder: ReceiveFolder? = null,
	val folderAccessStatus: FolderAccessStatus = FolderAccessStatus.Unavailable,
	val isInspecting: Boolean = false,
	val isReceiving: Boolean = false,
	val activeReceiveTransferId: ULong? = null,
	val lastReceiveError: UiText? = null,
	val isWaitingForNfc: Boolean = false,
	val historyDeleteTarget: ReceiveHistoryDeleteTarget? = null,
	val isDeletingHistory: Boolean = false,
) {
	fun canReceive(coreInitialized: Boolean): Boolean =
		coreInitialized && ticket.isNotBlank() && inspection != null &&
			folderAccessStatus == FolderAccessStatus.Writable && !isReceiving && !isInspecting
}

class ReceiveViewModel(
	private val repository: CoreGateway,
	private val fileSystemService: FileSystemService,
	preferencesRepository: PreferencesRepository,
	private val messages: UiMessageController,
) : ViewModel() {
	private val _state = MutableStateFlow(ReceiveState())
	val state: StateFlow<ReceiveState> = _state.asStateFlow()
	val coreState = repository.state

	init {
		viewModelScope.launch {
			preferencesRepository.preferences.collect { preferences ->
				val receiveFolder = fileSystemService.effectiveReceiveFolder(preferences.receiveFolder)
				val status = fileSystemService.validateReceiveFolder(receiveFolder)
				_state.update { current ->
					current.copy(
						receiverName = current.receiverName.ifBlank { preferences.username },
						receiveFolder = receiveFolder,
						folderAccessStatus = status,
					)
				}
			}
		}
		viewModelScope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					is CoreSignal.TransfersChanged -> {
						repository.refresh()
						if (_state.value.isReceiving && signal.transferId != 0UL) {
							_state.update { it.copy(activeReceiveTransferId = signal.transferId) }
						}
					}
					is CoreSignal.ApprovalChanged,
					is CoreSignal.ReceiverHistoryChanged,
					CoreSignal.PairingChanged,
					CoreSignal.TargetedTransferChanged,
					CoreSignal.RuntimeObligationChanged -> Unit
				}
			}
		}
	}

	fun openAcquisition() = _state.update { it.copy(isAcquisitionOpen = true) }
	fun dismissAcquisition() {
		if (!_state.value.isReceiving && !_state.value.isInspecting) resetAcquisition()
	}
	fun setReceiverName(value: String) = _state.update { it.copy(receiverName = value) }
	fun setWaitingForNfc(waiting: Boolean) = _state.update { it.copy(isWaitingForNfc = waiting) }
	fun requestDeleteHistoryItem(transferId: ULong) {
		val canDelete = coreState.value.transfers.any { transfer ->
			transfer.transferId == transferId && transfer.direction == TransferDirection.Receive && transfer.status.isTerminalReceiveHistory()
		}
		if (canDelete) _state.update { it.copy(historyDeleteTarget = ReceiveHistoryDeleteTarget.Transfer(transferId)) }
	}
	fun requestClearHistory() {
		if (coreState.value.transfers.any { it.direction == TransferDirection.Receive && it.status.isTerminalReceiveHistory() }) {
			_state.update { it.copy(historyDeleteTarget = ReceiveHistoryDeleteTarget.All) }
		}
	}
	fun dismissHistoryDelete() {
		if (!_state.value.isDeletingHistory) _state.update { it.copy(historyDeleteTarget = null) }
	}
	fun confirmHistoryDelete() {
		val target = _state.value.historyDeleteTarget ?: return
		if (_state.value.isDeletingHistory) return
		viewModelScope.launch {
			_state.update { it.copy(isDeletingHistory = true) }
			val result = when (target) {
				is ReceiveHistoryDeleteTarget.Transfer -> repository.delete(target.transferId).map { Unit }
				ReceiveHistoryDeleteTarget.All -> repository.clearReceiveHistory().map { Unit }
			}
			result.fold(
				onSuccess = {
					_state.update { it.copy(historyDeleteTarget = null, isDeletingHistory = false) }
					val message = when (target) {
						is ReceiveHistoryDeleteTarget.Transfer -> Res.string.transfer_deleted
						ReceiveHistoryDeleteTarget.All -> Res.string.receive_history_cleared
					}
					messages.tryShow(UiMessage(UiText.Resource(message), UiMessageTone.Success))
				},
				onFailure = { error ->
					_state.update { it.copy(isDeletingHistory = false) }
					messages.error(error)
				},
			)
		}
	}

	fun onInvitationResult(method: ReceiveMethod, result: Result<String>) {
		_state.update { it.copy(isWaitingForNfc = false) }
		result.fold(
			onSuccess = { raw -> inspectInvitation(method, raw) },
			onFailure = messages::error,
		)
	}

	fun receive() {
		val current = state.value
		val folder = current.receiveFolder ?: return
		if (!current.canReceive(coreState.value.isInitialized)) return
		viewModelScope.launch {
			_state.update {
				it.copy(isReceiving = true, lastReceiveError = null, activeReceiveTransferId = null)
			}
			val outputSink = fileSystemService.createReceiveOutputSink(folder)
			val result = if (outputSink != null) {
				repository.receiveWithOutputSinkV2(current.ticket, outputSink, current.receiverName)
			} else {
				repository.receive(current.ticket, folder.value, current.receiverName)
			}
			result.fold(
				onSuccess = {
					resetAcquisition()
					val canRevealFolder = fileSystemService.canRevealReceiveFolder(folder)
					messages.tryShow(
						UiMessage(
							text = UiText.Resource(Res.string.receive_completed),
							tone = UiMessageTone.Success,
							actionLabel = if (canRevealFolder) UiText.Resource(Res.string.button_show_in_files) else null,
							onAction = if (canRevealFolder) {
								{ revealReceiveFolder(folder) }
							} else {
								null
							},
						),
					)
				},
				onFailure = { error ->
					if (error.isUserCancellation()) {
						_state.update {
							it.copy(
								isReceiving = false,
								activeReceiveTransferId = null,
								lastReceiveError = null,
							)
						}
						return@fold
					}
					val uiText = error.toUiText()
					_state.update {
						it.copy(
							isReceiving = false,
							activeReceiveTransferId = null,
							lastReceiveError = uiText,
						)
					}
					messages.tryShow(
						UiMessage(
							text = uiText,
							tone = UiMessageTone.Error,
							actionLabel = if (error.canRetryWithoutChangingInput()) {
								UiText.Resource(Res.string.button_retry)
							} else {
								null
							},
							onAction = if (error.canRetryWithoutChangingInput()) ({ receive() }) else null,
						),
					)
				},
			)
		}
	}

	fun cancelActiveReceive() {
		val transferId = _state.value.activeReceiveTransferId
			?: coreState.value.transfers.firstOrNull {
				it.direction == TransferDirection.Receive && it.status == TransferStatus.Receiving
			}?.transferId
			?: coreState.value.events.firstOrNull {
				it.direction == "receive" && it.transferId != null
			}?.transferId
			?: return
		viewModelScope.launch {
			repository.cancel(transferId).fold(
				onSuccess = {
					_state.update {
						it.copy(isReceiving = false, activeReceiveTransferId = null, lastReceiveError = null)
					}
					repository.refresh()
				},
				onFailure = messages::error,
			)
		}
	}

	private fun revealReceiveFolder(folder: ReceiveFolder) {
		viewModelScope.launch {
			fileSystemService.revealReceiveFolder(folder).onFailure {
				messages.show(
					UiMessage(UiText.Resource(Res.string.receive_open_files_failed), UiMessageTone.Error),
				)
			}
		}
	}

	private fun inspectInvitation(method: ReceiveMethod, raw: String) {
		val ticket = raw.trim()
		if (ticket.isBlank()) return messages.error(UiText.Resource(Res.string.error_invitation_empty))
		viewModelScope.launch {
			_state.update {
				it.copy(
					isAcquisitionOpen = true,
					ticket = ticket,
					method = method,
					inspection = null,
					isInspecting = true,
				)
			}
			repository.inspectTicket(ticket).fold(
				onSuccess = { inspection -> _state.update { it.copy(inspection = inspection, isInspecting = false) } },
				onFailure = { error ->
					_state.update { it.copy(ticket = "", method = null, inspection = null, isInspecting = false) }
					messages.error(error)
				},
			)
		}
	}

	private fun resetAcquisition() = _state.update {
		it.copy(
			isAcquisitionOpen = false,
			ticket = "",
			method = null,
			inspection = null,
			isInspecting = false,
			isReceiving = false,
			activeReceiveTransferId = null,
			lastReceiveError = null,
			isWaitingForNfc = false,
		)
	}
}

internal fun TransferStatus.isTerminalReceiveHistory(): Boolean =
	this == TransferStatus.Done || this == TransferStatus.Failed || this == TransferStatus.Cancelled
