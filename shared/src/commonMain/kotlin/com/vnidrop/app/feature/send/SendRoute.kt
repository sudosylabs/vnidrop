package com.vnidrop.app.feature.send

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.vnidrop.app.core.rememberShareFilePicker
import com.vnidrop.app.ui.state.WindowClass

@Composable
fun SendRoute(
	viewModel: SendViewModel,
	windowClass: WindowClass,
) {
	val state by viewModel.state.collectAsStateWithLifecycle()
	val coreState by viewModel.coreState.collectAsStateWithLifecycle()
	val clipboard = LocalClipboardManager.current
	val picker = rememberShareFilePicker(viewModel::onFilesPicked, viewModel::onFilePickFailed)
	val shareActions = rememberTransferShareActions()

	LaunchedEffect(viewModel) {
		viewModel.effectFlow.collect { effect ->
			when (effect) {
				SendEffect.OpenFilePicker -> picker.pickFiles()
				SendEffect.OpenFolderPicker -> picker.pickFolder()
				is SendEffect.CopyTicket -> clipboard.setText(AnnotatedString(effect.ticket))
			}
		}
	}

	SendScreen(
		coreState = coreState,
		state = state,
		windowClass = windowClass,
		shareActions = shareActions,
		onOpenComposer = viewModel::openComposer,
		onDismissComposer = viewModel::dismissComposer,
		onSelectFile = viewModel::selectFile,
		onSelectFolder = viewModel::selectFolder,
		onClearFile = viewModel::clearSelectedSource,
		onRemoveFile = viewModel::removeSelectedFile,
		onTransferNameChanged = viewModel::setTransferName,
		onSenderNameChanged = viewModel::setSenderName,
		onAccessPolicyChanged = viewModel::setAccessPolicy,
		onCreateShare = viewModel::createShare,
		onTransferSelected = viewModel::openTransfer,
		onShareTransfer = { transferId ->
			viewModel.openTransfer(transferId)
			viewModel.openShare()
		},
		onStopSharing = viewModel::stopSharing,
		onCloseTransferDetails = viewModel::closeTransferDetails,
		onCopyTicket = viewModel::copyTicket,
		onActivity = viewModel::openActivity,
		onReceivers = viewModel::openReceivers,
		onShare = viewModel::openShare,
		onCloseDetailPanel = viewModel::closeDetailPanel,
		onInvitationResult = viewModel::onInvitationResult,
		onRequestDelete = { viewModel.requestDeleteTransfer() },
		onRequestDeleteTransfer = { viewModel.requestDeleteTransfer(it) },
		onDismissDelete = viewModel::dismissDeleteTransfer,
		onConfirmDelete = viewModel::confirmDeleteTransfer,
	)
}
