package com.vnidrop.app.feature.saveddevices

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_cancel
import vnidrop.shared.generated.resources.button_retry
import vnidrop.shared.generated.resources.saved_devices_label_clear
import vnidrop.shared.generated.resources.saved_devices_label_placeholder
import vnidrop.shared.generated.resources.saved_devices_label_save
import vnidrop.shared.generated.resources.saved_devices_label_title
import vnidrop.shared.generated.resources.saved_devices_list_title
import vnidrop.shared.generated.resources.saved_devices_load_failed
import vnidrop.shared.generated.resources.saved_devices_loading

@Composable
internal fun SavedDevicesScreen(
	state: SavedDevicesState,
	windowClass: WindowClass,
	modifier: Modifier = Modifier,
	onRetry: () -> Unit,
	onRememberEligible: (String) -> Unit,
	onDeclineEligible: (String) -> Unit,
	onAcceptIncoming: (String) -> Unit,
	onDeclineIncoming: (String) -> Unit,
	onAcceptOffer: (String) -> Unit,
	onDeclineOffer: (String) -> Unit,
	onSend: (String) -> Unit,
	onOpenLabel: (String) -> Unit,
	onForget: (String) -> Unit,
	onBlock: (String) -> Unit,
	onTransferAction: (String, SavedDeviceTransferAction) -> Unit,
	onLabelDraftChanged: (String) -> Unit,
	onSaveLabel: () -> Unit,
	onClearLabel: () -> Unit,
	onDismissLabel: () -> Unit,
) {
	var selectedDeviceId by remember { mutableStateOf<String?>(null) }
	val selectedDevice = state.savedDevices.firstOrNull { it.endpointId == selectedDeviceId }
	val hasContent = state.eligibilities.isNotEmpty() || state.pendingRelationships.isNotEmpty() ||
		state.savedDevices.isNotEmpty() || state.targetedTransfers.isNotEmpty() ||
		state.targetedOffers.pending.isNotEmpty()
	Column(
		modifier = modifier.fillMaxSize().statusBarsPadding().padding(top = 16.dp),
	) {
		Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
			SavedDevicesHeader(
				Modifier.fillMaxWidth().widthIn(max = 720.dp)
					.padding(horizontal = if (windowClass == WindowClass.Desktop) 24.dp else 16.dp),
			)
		}
		Spacer(Modifier.height(14.dp))
		when {
			state.isLoading && !hasContent -> SavedDevicesLoading(Modifier.weight(1f))
			state.loadFailed && !hasContent -> SavedDevicesLoadFailure(onRetry, Modifier.weight(1f))
			windowClass == WindowClass.Desktop -> DesktopSavedDevicesHub(
				state = state,
				onRetry = onRetry,
				onRememberEligible = onRememberEligible,
				onDeclineEligible = onDeclineEligible,
				onAcceptIncoming = onAcceptIncoming,
				onDeclineIncoming = onDeclineIncoming,
				onAcceptOffer = onAcceptOffer,
				onDeclineOffer = onDeclineOffer,
				onOpenDevice = { selectedDeviceId = it },
			)
			else -> CompactSavedDevicesHub(
				state = state,
				onRetry = onRetry,
				onRememberEligible = onRememberEligible,
				onDeclineEligible = onDeclineEligible,
				onAcceptIncoming = onAcceptIncoming,
				onDeclineIncoming = onDeclineIncoming,
				onAcceptOffer = onAcceptOffer,
				onDeclineOffer = onDeclineOffer,
				onOpenDevice = { selectedDeviceId = it },
			)
		}
	}
	if (selectedDevice != null) {
		SavedDeviceDetailsDrawer(
			device = selectedDevice,
			transfers = state.targetedTransfers.filter { it.peerEndpointId == selectedDevice.endpointId },
			busy = selectedDevice.endpointId in state.busyPeerIds,
			busyTransferIds = state.busyTransferIds,
			windowClass = windowClass,
			onDismiss = { selectedDeviceId = null },
			onSend = {
				selectedDeviceId = null
				onSend(selectedDevice.endpointId)
			},
			onOpenLabel = { onOpenLabel(selectedDevice.endpointId) },
			onForget = { onForget(selectedDevice.endpointId) },
			onBlock = { onBlock(selectedDevice.endpointId) },
			onTransferAction = onTransferAction,
		)
	}
	SavedDeviceLabelDialog(
		visible = state.labelingPeerId != null,
		label = state.labelDraft,
		saving = state.isSavingLabel,
		onLabelChanged = onLabelDraftChanged,
		onSave = onSaveLabel,
		onClear = onClearLabel,
		onDismiss = onDismissLabel,
	)
}

@Composable
private fun SavedDevicesHeader(modifier: Modifier = Modifier) {
	Text(
		text = stringResource(Res.string.saved_devices_list_title),
		style = MaterialTheme.typography.headlineSmall,
		fontWeight = FontWeight.Bold,
		modifier = modifier.semantics { heading() },
	)
}

@Composable
private fun SavedDevicesLoading(modifier: Modifier = Modifier) {
	Box(modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
		Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
			CircularProgressIndicator()
			Text(stringResource(Res.string.saved_devices_loading), color = LocalVniDropColors.current.foregroundLight)
		}
	}
}

@Composable
private fun SavedDevicesLoadFailure(onRetry: () -> Unit, modifier: Modifier = Modifier) {
	Box(modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
		Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
			PlatformIcon(AppIcon.CloudOff, contentDescription = null, modifier = Modifier.size(32.dp))
			Text(stringResource(Res.string.saved_devices_load_failed), color = LocalVniDropColors.current.foregroundLight)
			SecondaryButton(stringResource(Res.string.button_retry), onRetry)
		}
	}
}

@Composable
private fun SavedDeviceLabelDialog(
	visible: Boolean,
	label: String,
	saving: Boolean,
	onLabelChanged: (String) -> Unit,
	onSave: () -> Unit,
	onClear: () -> Unit,
	onDismiss: () -> Unit,
) {
	if (!visible) return
	AlertDialog(
		onDismissRequest = { if (!saving) onDismiss() },
		title = { Text(stringResource(Res.string.saved_devices_label_title)) },
		text = {
			OutlinedTextField(
				value = label,
				onValueChange = onLabelChanged,
				modifier = Modifier.fillMaxWidth(),
				enabled = !saving,
				singleLine = true,
				placeholder = { Text(stringResource(Res.string.saved_devices_label_placeholder)) },
			)
		},
		confirmButton = {
			TextButton(onClick = onSave, enabled = !saving) {
				if (saving) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
				else Text(stringResource(Res.string.saved_devices_label_save))
			}
		},
		dismissButton = {
			androidx.compose.foundation.layout.Row {
				TextButton(onClick = onClear, enabled = !saving) { Text(stringResource(Res.string.saved_devices_label_clear)) }
				TextButton(onClick = onDismiss, enabled = !saving) { Text(stringResource(Res.string.button_cancel)) }
			}
		},
	)
}
