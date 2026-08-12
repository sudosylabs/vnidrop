package com.vnidrop.app.feature.saveddevices

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.vnidrop.app.feature.send.TransferDraftViewModel
import com.vnidrop.app.ui.state.WindowClass
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.saved_devices_unnamed

@Composable
internal fun SavedDevicesRoute(
	viewModel: SavedDevicesViewModel,
	targetedDraftViewModel: TransferDraftViewModel,
	windowClass: WindowClass,
) {
	val state by viewModel.state.collectAsStateWithLifecycle()
	val unnamedDeviceName = stringResource(Res.string.saved_devices_unnamed)
	SavedDevicesScreen(
		state = state,
		windowClass = windowClass,
		onRetry = viewModel::retry,
		onRememberEligible = viewModel::rememberEligible,
		onDeclineEligible = viewModel::declineEligible,
		onAcceptIncoming = viewModel::acceptIncoming,
		onDeclineIncoming = viewModel::declineIncoming,
		onSend = { peerEndpointId ->
			state.savedDevices.firstOrNull { it.endpointId == peerEndpointId }
				?.let { targetedDraftViewModel.openTargeted(it, unnamedDeviceName) }
		},
		onOpenLabel = viewModel::openLabelEditor,
		onForget = viewModel::forget,
		onBlock = viewModel::block,
		onLabelDraftChanged = viewModel::setLabelDraft,
		onSaveLabel = viewModel::saveLabel,
		onClearLabel = viewModel::clearLabel,
		onDismissLabel = viewModel::dismissLabelEditor,
	)
}
