package com.vnidrop.app.feature.send

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.PickedShareFile
import com.vnidrop.app.core.PickedShareSourceAdapter
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.ui.feedback.UiMessage
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiMessageTone
import com.vnidrop.app.ui.feedback.UiText
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.getString
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.saved_devices_send_started
import vnidrop.shared.generated.resources.send_default_transfer_name
import vnidrop.shared.generated.resources.send_transfer_created

sealed interface TransferDraftDestination {
	data object Invitation : TransferDraftDestination

	data class Targeted(
		val receiver: LockedSavedDevice,
	) : TransferDraftDestination
}

data class LockedSavedDevice(
	val endpointId: String,
	val displayName: String,
)

@JvmInline
value class DraftSourceId(val value: String)

data class TransferDraftSource(
	val id: DraftSourceId,
	val displayName: String,
	val sizeBytes: ULong?,
	val thumbnailBytes: ByteArray?,
	val isDirectory: Boolean,
)

enum class TransferDraftPickKind { Files, Folder }

data class TransferDraftState(
	val destination: TransferDraftDestination? = null,
	val sources: List<TransferDraftSource> = emptyList(),
	val transferName: String = "",
	val senderName: String = "",
	val accessPolicy: ShareAccessPolicy = ShareAccessPolicy.RequireApproval,
	val pickerRequestId: Long? = null,
	val isPreparingSources: Boolean = false,
	val isSubmitting: Boolean = false,
) {
	val isOpen: Boolean get() = destination != null
	val isPicking: Boolean get() = pickerRequestId != null || isPreparingSources
	val totalSelectedBytes: ULong
		get() = sources.fold(0UL) { total, source -> total + (source.sizeBytes ?: 0UL) }

	fun canSubmit(coreInitialized: Boolean): Boolean =
		coreInitialized && sources.isNotEmpty() && transferName.isNotBlank() && !isPicking && !isSubmitting
}

sealed interface TransferDraftCreation {
	val awaitsRemoteApproval: Boolean

	data class Invitation(
		val transferId: ULong,
		val accessPolicy: ShareAccessPolicy,
	) : TransferDraftCreation {
		override val awaitsRemoteApproval: Boolean = accessPolicy == ShareAccessPolicy.RequireApproval
	}
	data class Targeted(val transferId: String, val receiverEndpointId: String) : TransferDraftCreation {
		override val awaitsRemoteApproval: Boolean = true
	}
}

sealed interface TransferDraftEffect {
	data class OpenPicker(val requestId: Long, val kind: TransferDraftPickKind) : TransferDraftEffect
	data class Created(val creation: TransferDraftCreation) : TransferDraftEffect
}

internal class TransferDraftViewModel(
	private val repository: CoreGateway,
	private val sourceAdapter: PickedShareSourceAdapter,
	private val filePreviewRepository: FilePreviewRepository,
	private val messages: UiMessageController,
	private val multipleFilesName: suspend (Int) -> String = { count ->
		getString(Res.string.send_default_transfer_name, count)
	},
) : ViewModel() {
	private data class SelectedSource(
		val source: TransferDraftSource,
		val picked: PickedShareFile,
	)

	private val _state = MutableStateFlow(TransferDraftState())
	val state: StateFlow<TransferDraftState> = _state.asStateFlow()
	val coreState = repository.state

	private val effects = Channel<TransferDraftEffect>(Channel.BUFFERED)
	val effectFlow = effects.receiveAsFlow()

	private var selectedSources = emptyList<SelectedSource>()
	private var nextPickerRequestId = 1L
	private var nextSourceId = 1L
	private var automaticName = true

	fun openInvitation(defaultSenderName: String) {
		if (_state.value.isOpen) return
		reset(
			TransferDraftState(
				destination = TransferDraftDestination.Invitation,
				senderName = defaultSenderName,
			),
		)
	}

	fun openTargeted(device: SavedDeviceModel, unnamedDeviceName: String) {
		if (_state.value.isOpen) return
		val displayName = device.localLabel?.takeIf(String::isNotBlank)
			?: device.remoteDisplayName?.takeIf(String::isNotBlank)
			?: unnamedDeviceName
		reset(
			TransferDraftState(
				destination = TransferDraftDestination.Targeted(
					LockedSavedDevice(device.endpointId, displayName),
				),
			),
		)
	}

	fun chooseFiles() = requestPicker(TransferDraftPickKind.Files)
	fun chooseFolder() = requestPicker(TransferDraftPickKind.Folder)

	fun onFilesPicked(requestId: Long, files: List<PickedShareFile>) {
		if (_state.value.pickerRequestId != requestId) {
			discard(files)
			return
		}
		if (files.isEmpty()) {
			_state.update { it.copy(pickerRequestId = null) }
			return
		}
		val validSelection = files.none(PickedShareFile::isDirectory) ||
			(files.size == 1 && files.single().isDirectory)
		if (!validSelection) {
			_state.update { it.copy(pickerRequestId = null) }
			discard(files)
			messages.error(IllegalArgumentException("Choose multiple files or one folder"))
			return
		}

		_state.update { it.copy(isPreparingSources = true) }
		viewModelScope.launch {
			val newName = try {
				when {
					files.size == 1 -> files.single().displayName
					else -> multipleFilesName(files.size)
				}
			} catch (error: CancellationException) {
				throw error
			} catch (error: Throwable) {
				_state.update { it.copy(pickerRequestId = null, isPreparingSources = false) }
				discardOwned(files)
				messages.error(error)
				return@launch
			}
			if (_state.value.pickerRequestId != requestId) {
				discardOwned(files)
				return@launch
			}
			val previous = selectedSources
			selectedSources = files.map { file ->
				SelectedSource(
					source = TransferDraftSource(
						id = DraftSourceId("source-${nextSourceId++}"),
						displayName = file.displayName,
						sizeBytes = file.sizeBytes,
						thumbnailBytes = file.thumbnailBytes,
						isDirectory = file.isDirectory,
					),
					picked = file,
				)
			}
			automaticName = true
			_state.update {
				it.copy(
					sources = selectedSources.map(SelectedSource::source),
					transferName = newName,
					pickerRequestId = null,
					isPreparingSources = false,
				)
			}
			val replacementValues = files.mapTo(mutableSetOf(), PickedShareFile::value)
			discardOwned(previous.map(SelectedSource::picked).filterNot { it.value in replacementValues })
		}
	}

	fun onFilePickFailed(requestId: Long, reason: String) {
		if (_state.value.pickerRequestId != requestId) return
		_state.update { it.copy(pickerRequestId = null) }
		messages.error(IllegalStateException(reason.ifBlank { "selection failed" }))
	}

	fun clearSources() {
		if (!editable()) return
		val discarded = selectedSources.map(SelectedSource::picked)
		selectedSources = emptyList()
		automaticName = true
		_state.update { it.copy(sources = emptyList(), transferName = "") }
		discard(discarded)
	}

	fun removeSource(id: DraftSourceId) {
		if (!editable()) return
		val discarded = selectedSources.filter { it.source.id == id }.map(SelectedSource::picked)
		if (discarded.isEmpty()) return
		selectedSources = selectedSources.filterNot { it.source.id == id }
		_state.update {
			it.copy(
				sources = selectedSources.map(SelectedSource::source),
				isPreparingSources = true,
			)
		}
		viewModelScope.launch {
			val replacementName = try {
				when {
					!automaticName -> _state.value.transferName
					selectedSources.isEmpty() -> ""
					selectedSources.size == 1 -> selectedSources.single().source.displayName
					else -> multipleFilesName(selectedSources.size)
				}
			} catch (error: CancellationException) {
				throw error
			} catch (error: Throwable) {
				_state.update { it.copy(isPreparingSources = false) }
				discardOwned(discarded)
				messages.error(error)
				return@launch
			}
			_state.update {
				it.copy(
					transferName = replacementName,
					isPreparingSources = false,
				)
			}
			discardOwned(discarded)
		}
	}

	fun changeTransferName(value: String) {
		if (!editable()) return
		automaticName = false
		_state.update { it.copy(transferName = value) }
	}

	fun changeSenderName(value: String) {
		if (!editable() || _state.value.destination !is TransferDraftDestination.Invitation) return
		_state.update { it.copy(senderName = value) }
	}

	fun changeAccessPolicy(value: ShareAccessPolicy) {
		if (!editable() || _state.value.destination !is TransferDraftDestination.Invitation) return
		_state.update { it.copy(accessPolicy = value) }
	}

	fun submit() {
		val current = _state.value
		if (!current.canSubmit(repository.state.value.isInitialized)) return
		val files = selectedSources.map(SelectedSource::picked)
		val thumbnail = selectedSources.firstNotNullOfOrNull { it.source.thumbnailBytes }
		viewModelScope.launch {
			_state.update { it.copy(isSubmitting = true) }
			val result = runCatching {
				val destination = current.destination ?: error("Transfer draft is closed")
				if (destination is TransferDraftDestination.Targeted) {
					val stillSaved = repository.listSavedDevices().getOrThrow()
						.any { it.endpointId == destination.receiver.endpointId }
					check(stillSaved) { "Saved device is no longer available" }
				}
				sourceAdapter.withShareSources(files) { sources ->
					when (destination) {
						TransferDraftDestination.Invitation -> repository.shareSources(
							sources = sources,
							transferName = current.transferName.trim(),
							senderName = current.senderName.trim(),
							accessPolicy = current.accessPolicy,
						).getOrThrow().let { share ->
							TransferDraftCreation.Invitation(share.transferId, current.accessPolicy)
						}
						is TransferDraftDestination.Targeted -> repository.createTargetedTransfer(
							receiverEndpointId = destination.receiver.endpointId,
							sources = sources,
							transferName = current.transferName.trim(),
						).getOrThrow().let { transfer ->
							TransferDraftCreation.Targeted(transfer.id, destination.receiver.endpointId)
						}
					}
				}.getOrThrow()
			}
			result.fold(
				onSuccess = { creation ->
					if (creation is TransferDraftCreation.Invitation) {
						runCatching {
							thumbnail?.let { filePreviewRepository.save(creation.transferId, it) }
							repository.refresh().getOrThrow()
						}.onFailure(messages::error)
					}
					discardOwned(files)
					selectedSources = emptyList()
					automaticName = true
					_state.value = TransferDraftState()
					effects.send(TransferDraftEffect.Created(creation))
					val message = when (creation) {
						is TransferDraftCreation.Invitation -> Res.string.send_transfer_created
						is TransferDraftCreation.Targeted -> Res.string.saved_devices_send_started
					}
					messages.show(UiMessage(UiText.Resource(message), UiMessageTone.Success))
				},
				onFailure = { error ->
					if (error is CancellationException) throw error
					_state.update { it.copy(isSubmitting = false) }
					messages.error(error)
				},
			)
		}
	}

	fun dismiss() {
		if (_state.value.isSubmitting) return
		val discarded = selectedSources.map(SelectedSource::picked)
		selectedSources = emptyList()
		automaticName = true
		_state.value = TransferDraftState()
		discard(discarded)
	}

	private fun requestPicker(kind: TransferDraftPickKind) {
		if (!editable() || _state.value.isPicking) return
		val requestId = nextPickerRequestId++
		_state.update { it.copy(pickerRequestId = requestId) }
		viewModelScope.launch { effects.send(TransferDraftEffect.OpenPicker(requestId, kind)) }
	}

	private fun editable(): Boolean = _state.value.isOpen && !_state.value.isPicking && !_state.value.isSubmitting

	private fun reset(state: TransferDraftState) {
		selectedSources = emptyList()
		automaticName = true
		_state.value = state
	}

	private fun discard(files: List<PickedShareFile>) {
		if (files.isEmpty()) return
		viewModelScope.launch { discardOwned(files) }
	}

	private suspend fun discardOwned(files: List<PickedShareFile>) {
		if (files.isEmpty()) return
		try {
			sourceAdapter.discardPickedFiles(files)
		} catch (error: CancellationException) {
			throw error
		} catch (error: Throwable) {
			messages.error(error)
		}
	}
}
