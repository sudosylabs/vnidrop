package com.vnidrop.app.feature.send

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.vnidrop.app.ui.state.WindowClass

@Composable
internal fun SendRoute(
	viewModel: SendViewModel,
	draftViewModel: TransferDraftViewModel,
	defaultSenderName: String,
	windowClass: WindowClass,
	onTransferCreated: (TransferDraftCreation) -> Unit,
) {
	val state by viewModel.state.collectAsStateWithLifecycle()
	val coreState by viewModel.coreState.collectAsStateWithLifecycle()
	val clipboard = LocalClipboardManager.current
	val shareActions = rememberTransferShareActions()

	LaunchedEffect(viewModel) {
		viewModel.effectFlow.collect { effect ->
			when (effect) {
				is SendEffect.CopyTicket -> clipboard.setText(AnnotatedString(effect.ticket))
			}
		}
	}

	SendScreen(
		coreState = coreState,
		state = state,
		windowClass = windowClass,
		shareActions = shareActions,
		onOpenComposer = { draftViewModel.openInvitation(defaultSenderName) },
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
	TransferDraftHost(draftViewModel, windowClass) { creation ->
		viewModel.onDraftCreated(creation)
		onTransferCreated(creation)
	}
}
