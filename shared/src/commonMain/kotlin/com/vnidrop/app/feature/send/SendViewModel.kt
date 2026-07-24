package com.vnidrop.app.feature.send

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.FileSystemService
import com.vnidrop.app.core.PickedShareFile
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.preferences.PreferencesRepository
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
import vnidrop.shared.generated.resources.send_transfer_created
import vnidrop.shared.generated.resources.transfer_deleted
import vnidrop.shared.generated.resources.transfer_invitation_saved
import vnidrop.shared.generated.resources.transfer_nfc_written

data class SendState(
	val isComposerOpen: Boolean = false,
	val selectedFiles: List<PickedShareFile> = emptyList(),
	val transferName: String = "",
	val senderName: String = "",
	val accessPolicy: ShareAccessPolicy = ShareAccessPolicy.RequireApproval,
	val isSharing: Boolean = false,
	val selectedTransferId: ULong? = null,
	val transferThumbnails: Map<ULong, ByteArray> = emptyMap(),
	val detailPanel: TransferDetailPanel? = null,
	val receiverHistory: List<ReceiverRequestModel> = emptyList(),
	val receiversByTransfer: Map<ULong, List<ReceiverRequestModel>> = emptyMap(),
	val isLoadingReceivers: Boolean = false,
	val isDeleteConfirmationOpen: Boolean = false,
	val isDeleting: Boolean = false,
) {
	val selectedFile: PickedShareFile? get() = selectedFiles.singleOrNull()
	val totalSelectedBytes: ULong
		get() = selectedFiles.fold(0UL) { acc, file -> acc + (file.sizeBytes ?: 0UL) }

	fun canCreateShare(coreInitialized: Boolean): Boolean =
		coreInitialized && selectedFiles.isNotEmpty() && transferName.isNotBlank() && !isSharing
}

enum class TransferDetailPanel { Activity, Receivers, Share }

sealed interface SendEffect {
	data object OpenFilePicker : SendEffect
	data object OpenFolderPicker : SendEffect
	data class CopyTicket(val ticket: String) : SendEffect
}

class SendViewModel(
	private val repository: CoreGateway,
	private val fileSystemService: FileSystemService,
	preferencesRepository: PreferencesRepository,
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
		viewModelScope.launch {
			preferencesRepository.preferences.collect { preferences ->
				_state.update { current ->
					if (current.senderName.isBlank()) current.copy(senderName = preferences.username) else current
				}
			}
		}
	}

	fun openComposer() {
		if (_state.value.isSharing) return
		val discardedFiles = _state.value.selectedFiles
		_state.update {
			it.copy(
				isComposerOpen = true,
				selectedFiles = emptyList(),
				transferName = "",
				accessPolicy = ShareAccessPolicy.RequireApproval,
			)
		}
		discardPickedFiles(discardedFiles)
	}

	fun dismissComposer() {
		if (_state.value.isSharing) return
		val discardedFiles = _state.value.selectedFiles
		_state.update {
			it.copy(
				isComposerOpen = false,
				selectedFiles = emptyList(),
				transferName = "",
				accessPolicy = ShareAccessPolicy.RequireApproval,
			)
		}
		discardPickedFiles(discardedFiles)
	}

	fun selectFile() = sendEffect(SendEffect.OpenFilePicker)
	fun selectFolder() = sendEffect(SendEffect.OpenFolderPicker)

	fun onFilesPicked(files: List<PickedShareFile>) {
		if (files.isEmpty()) return
		val selectedValues = files.mapTo(mutableSetOf(), PickedShareFile::value)
		val discardedFiles = _state.value.selectedFiles.filterNot { it.value in selectedValues }
		_state.update {
			it.copy(
				isComposerOpen = true,
				selectedFiles = files,
				transferName = defaultTransferName(files),
			)
		}
		discardPickedFiles(discardedFiles)
	}

	fun onFilePickFailed(reason: String) = messages.error(IllegalStateException(reason.takeIf(String::isNotBlank) ?: "selection failed"))

	fun clearSelectedSource() {
		val discardedFiles = _state.value.selectedFiles
		_state.update { it.copy(selectedFiles = emptyList(), transferName = "") }
		discardPickedFiles(discardedFiles)
	}

	fun removeSelectedFile(value: String) {
		val discardedFiles = _state.value.selectedFiles.filter { it.value == value }
		_state.update { current ->
			val remaining = current.selectedFiles.filterNot { it.value == value }
			current.copy(
				selectedFiles = remaining,
				transferName = when {
					remaining.isEmpty() -> ""
					current.transferName == defaultTransferName(current.selectedFiles) -> defaultTransferName(remaining)
					else -> current.transferName
				},
			)
		}
		discardPickedFiles(discardedFiles)
	}

	fun setTransferName(value: String) = _state.update { it.copy(transferName = value) }
	fun setSenderName(value: String) = _state.update { it.copy(senderName = value) }
	fun setAccessPolicy(value: ShareAccessPolicy) = _state.update { it.copy(accessPolicy = value) }
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
	fun requestDeleteTransfer() = _state.update { it.copy(isDeleteConfirmationOpen = true) }
	fun dismissDeleteTransfer() {
		if (!_state.value.isDeleting) _state.update { it.copy(isDeleteConfirmationOpen = false) }
	}
	fun confirmDeleteTransfer() {
		val transferId = _state.value.selectedTransferId ?: return
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

	fun createShare() {
		val current = state.value
		if (current.selectedFiles.isEmpty()) return
		if (!current.canCreateShare(coreState.value.isInitialized)) return
		viewModelScope.launch {
			_state.update { it.copy(isSharing = true) }
			val result = fileSystemService.sharePickedFiles(
				repository = repository,
				files = current.selectedFiles,
				transferName = current.transferName.trim(),
				senderName = current.senderName.trim(),
				accessPolicy = current.accessPolicy,
			)
			if (result.isSuccess) fileSystemService.discardPickedFiles(current.selectedFiles)
			result.fold(
				onSuccess = { share ->
					current.selectedFiles.firstNotNullOfOrNull { it.thumbnailBytes }
						?.let { filePreviewRepository.save(share.transferId, it) }
					_state.update {
						it.copy(
							isComposerOpen = false,
							selectedFiles = emptyList(),
							transferName = "",
							accessPolicy = ShareAccessPolicy.RequireApproval,
							isSharing = false,
						)
					}
					messages.show(UiMessage(UiText.Resource(Res.string.send_transfer_created), UiMessageTone.Success))
				},
				onFailure = { error ->
					_state.update { it.copy(isSharing = false) }
					messages.error(error)
				},
			)
		}
	}

	private fun defaultTransferName(files: List<PickedShareFile>): String = when {
		files.isEmpty() -> ""
		files.size == 1 && files.first().isDirectory -> files.first().displayName
		files.size == 1 -> files.first().displayName
		files.all { it.isDirectory } -> "${files.size} folders"
		else -> "${files.size} files"
	}

	private fun sendEffect(effect: SendEffect) {
		viewModelScope.launch { effects.send(effect) }
	}

	private fun discardPickedFiles(files: List<PickedShareFile>) {
		if (files.isEmpty()) return
		viewModelScope.launch { fileSystemService.discardPickedFiles(files) }
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
