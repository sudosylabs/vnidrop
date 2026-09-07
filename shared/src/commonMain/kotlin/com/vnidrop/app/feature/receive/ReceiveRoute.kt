package com.vnidrop.app.feature.receive

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.VniDropSnackbarHost
import com.vnidrop.app.ui.state.WindowClass

@Composable
fun ReceiveRoute(viewModel: ReceiveViewModel, windowClass: WindowClass, messages: UiMessageController? = null) {
	val state by viewModel.state.collectAsStateWithLifecycle()
	val coreState by viewModel.coreState.collectAsStateWithLifecycle()
	val actions = rememberReceiveInvitationActions()
	DisposableEffect(actions) { onDispose(actions::cancel) }

	ReceiveScreen(
		coreState = coreState,
		state = state,
		windowClass = windowClass,
		actions = actions,
		messages = messages,
		onOpenAcquisition = viewModel::openAcquisition,
		onDismissAcquisition = {
			actions.cancel()
			viewModel.dismissAcquisition()
		},
		onReceiverNameChanged = viewModel::setReceiverName,
		onInvitationResult = viewModel::onInvitationResult,
		onReceive = viewModel::receive,
		onCancelReceive = viewModel::cancelActiveReceive,
		onRequestDeleteHistoryItem = viewModel::requestDeleteHistoryItem,
		onRequestClearHistory = viewModel::requestClearHistory,
		onDismissHistoryDelete = viewModel::dismissHistoryDelete,
		onConfirmHistoryDelete = viewModel::confirmHistoryDelete,
	)
}
