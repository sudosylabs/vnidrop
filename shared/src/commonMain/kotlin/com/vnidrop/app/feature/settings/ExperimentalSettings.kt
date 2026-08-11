package com.vnidrop.app.feature.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp
import com.vnidrop.app.feature.saveddevices.SavedDevicesPanel
import com.vnidrop.app.feature.saveddevices.SavedDevicesState
import com.vnidrop.app.ui.icons.AppIcon
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.experimental_saved_devices_description
import vnidrop.shared.generated.resources.experimental_saved_devices_title
import vnidrop.shared.generated.resources.experimental_settings_title

@Composable
internal fun ExperimentalSettings(
	state: SettingsState,
	savedDevicesState: SavedDevicesState,
	onSavedDevicesEnabledChanged: (Boolean) -> Unit,
	onRememberEligible: (String) -> Unit,
	onDeclineEligible: (String) -> Unit,
	onAcceptIncoming: (String) -> Unit,
	onDeclineIncoming: (String) -> Unit,
	onSendToDevice: (String) -> Unit,
	onOpenDeviceLabel: (String) -> Unit,
	onForgetDevice: (String) -> Unit,
	onBlockDevice: (String) -> Unit,
	onLabelDraftChanged: (String) -> Unit,
	onSaveDeviceLabel: () -> Unit,
	onClearDeviceLabel: () -> Unit,
	onDismissDeviceLabel: () -> Unit,
	onBack: () -> Unit,
	showBack: Boolean,
) {
	Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
		SettingsTopBar(stringResource(Res.string.experimental_settings_title), onBack, showBack)
		SettingsGroup {
			SettingsToggleRow(
				icon = AppIcon.Lock,
				title = stringResource(Res.string.experimental_saved_devices_title),
				description = stringResource(Res.string.experimental_saved_devices_description),
				checked = state.experimentalSavedDevicesEnabled,
				enabled = true,
				onCheckedChange = onSavedDevicesEnabledChanged,
			)
		}
		if (state.experimentalSavedDevicesEnabled) {
			SavedDevicesPanel(
				state = savedDevicesState,
				onRememberEligible = onRememberEligible,
				onDeclineEligible = onDeclineEligible,
				onAcceptIncoming = onAcceptIncoming,
				onDeclineIncoming = onDeclineIncoming,
				onSend = onSendToDevice,
				onOpenLabel = onOpenDeviceLabel,
				onForget = onForgetDevice,
				onBlock = onBlockDevice,
				onLabelDraftChanged = onLabelDraftChanged,
				onSaveLabel = onSaveDeviceLabel,
				onClearLabel = onClearDeviceLabel,
				onDismissLabel = onDismissDeviceLabel,
			)
		}
	}
}
