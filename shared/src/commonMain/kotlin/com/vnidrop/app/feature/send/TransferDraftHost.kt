package com.vnidrop.app.feature.send

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.vnidrop.app.core.rememberShareFilePicker
import com.vnidrop.app.ui.components.AdaptiveDrawer
import com.vnidrop.app.ui.state.WindowClass

@Composable
internal fun TransferDraftHost(
	viewModel: TransferDraftViewModel,
	windowClass: WindowClass,
	onCreated: (TransferDraftCreation) -> Unit,
) {
	val state by viewModel.state.collectAsStateWithLifecycle()
	val coreState by viewModel.coreState.collectAsStateWithLifecycle()
	var activePickerRequestId by remember { mutableStateOf<Long?>(null) }
	val picker = rememberShareFilePicker(
		onFilesPicked = { files -> activePickerRequestId?.let { viewModel.onFilesPicked(it, files) } },
		onError = { reason -> activePickerRequestId?.let { viewModel.onFilePickFailed(it, reason) } },
	)

	LaunchedEffect(viewModel) {
		viewModel.effectFlow.collect { effect ->
			when (effect) {
				is TransferDraftEffect.OpenPicker -> {
					activePickerRequestId = effect.requestId
					when (effect.kind) {
						TransferDraftPickKind.Files -> picker.pickFiles()
						TransferDraftPickKind.Folder -> picker.pickFolder()
					}
				}
				is TransferDraftEffect.Created -> onCreated(effect.creation)
			}
		}
	}

	if (state.isOpen) {
		AdaptiveDrawer(windowClass = windowClass, onDismissRequest = viewModel::dismiss) {
			TransferComposer(
				coreInitialized = coreState.isInitialized,
				state = state,
				windowClass = windowClass,
				onSelectFile = viewModel::chooseFiles,
				onSelectFolder = viewModel::chooseFolder,
				onClearFile = viewModel::clearSources,
				onRemoveFile = viewModel::removeSource,
				onTransferNameChanged = viewModel::changeTransferName,
				onSenderNameChanged = viewModel::changeSenderName,
				onAccessPolicyChanged = viewModel::changeAccessPolicy,
				onSubmit = viewModel::submit,
			)
		}
	}
}
