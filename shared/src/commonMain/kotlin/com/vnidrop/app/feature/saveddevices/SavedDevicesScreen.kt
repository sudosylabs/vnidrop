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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
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
import vnidrop.shared.generated.resources.saved_devices_description
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
	val hasContent = state.eligibilities.isNotEmpty() || state.pendingRelationships.isNotEmpty() ||
		state.savedDevices.isNotEmpty() || state.targetedTransfers.isNotEmpty() ||
		state.targetedOffers.pending.isNotEmpty()
	Column(
		modifier = modifier.fillMaxSize().statusBarsPadding().padding(top = 16.dp),
	) {
		SavedDevicesHeader(Modifier.padding(horizontal = if (windowClass == WindowClass.Desktop) 24.dp else 16.dp))
		Spacer(Modifier.height(20.dp))
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
				onSend = onSend,
				onOpenLabel = onOpenLabel,
				onForget = onForget,
				onBlock = onBlock,
				onTransferAction = onTransferAction,
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
				onSend = onSend,
				onOpenLabel = onOpenLabel,
				onForget = onForget,
				onBlock = onBlock,
				onTransferAction = onTransferAction,
			)
		}
	}
	SavedDeviceLabelDialog(
		visible = state.labelingPeerId != null,
		label = state.labelDraft,
		onLabelChanged = onLabelDraftChanged,
		onSave = onSaveLabel,
		onClear = onClearLabel,
		onDismiss = onDismissLabel,
	)
}

@Composable
private fun SavedDevicesHeader(modifier: Modifier = Modifier) {
	Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
		Text(
			text = stringResource(Res.string.saved_devices_list_title),
			style = MaterialTheme.typography.headlineMedium,
			fontWeight = FontWeight.Bold,
			modifier = Modifier.semantics { heading() },
		)
		Text(
			text = stringResource(Res.string.saved_devices_description),
			style = MaterialTheme.typography.bodyMedium,
			color = LocalVniDropColors.current.foregroundLight,
		)
	}
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
	onLabelChanged: (String) -> Unit,
	onSave: () -> Unit,
	onClear: () -> Unit,
	onDismiss: () -> Unit,
) {
	if (!visible) return
	AlertDialog(
		onDismissRequest = onDismiss,
		title = { Text(stringResource(Res.string.saved_devices_label_title)) },
		text = {
			OutlinedTextField(
				value = label,
				onValueChange = onLabelChanged,
				modifier = Modifier.fillMaxWidth(),
				singleLine = true,
				placeholder = { Text(stringResource(Res.string.saved_devices_label_placeholder)) },
			)
		},
		confirmButton = {
			TextButton(onClick = onSave) { Text(stringResource(Res.string.saved_devices_label_save)) }
		},
		dismissButton = {
			androidx.compose.foundation.layout.Row {
				TextButton(onClick = onClear) { Text(stringResource(Res.string.saved_devices_label_clear)) }
				TextButton(onClick = onDismiss) { Text(stringResource(Res.string.button_cancel)) }
			}
		},
	)
}
