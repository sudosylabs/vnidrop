package com.vnidrop.app.feature.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.RelayMode
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.theme.ThemeMode
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.about_title
import vnidrop.shared.generated.resources.appearance_dark_mode
import vnidrop.shared.generated.resources.appearance_light_mode
import vnidrop.shared.generated.resources.appearance_system_mode
import vnidrop.shared.generated.resources.appearance_title
import vnidrop.shared.generated.resources.notifications_title
import vnidrop.shared.generated.resources.preferences_title
import vnidrop.shared.generated.resources.relay_mode_automatic
import vnidrop.shared.generated.resources.relay_mode_custom
import vnidrop.shared.generated.resources.relay_mode_custom_direct_fallback
import vnidrop.shared.generated.resources.relay_mode_local_only
import vnidrop.shared.generated.resources.settings_title
import vnidrop.shared.generated.resources.settings_network_title
import vnidrop.shared.generated.resources.storage_title

@Composable
internal fun SettingsOverview(
	state: SettingsState,
	onSectionSelected: (SettingsSection) -> Unit,
	largeTitle: Boolean,
) {
	Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
		Text(
			stringResource(Res.string.settings_title),
			style = if (largeTitle) MaterialTheme.typography.headlineLarge else MaterialTheme.typography.headlineMedium,
			fontWeight = FontWeight.Bold,
		)
		SettingsGroup {
			SettingsRow(
				icon = AppIcon.User,
				title = stringResource(Res.string.preferences_title),
				value = state.username,
				selected = state.selectedSection == SettingsSection.Preferences,
				onClick = { onSectionSelected(SettingsSection.Preferences) },
			)
			SettingsDivider()
			SettingsRow(
				icon = AppIcon.Sun,
				title = stringResource(Res.string.appearance_title),
				value = themeModeLabel(state.themeMode),
				selected = state.selectedSection == SettingsSection.Appearance,
				onClick = { onSectionSelected(SettingsSection.Appearance) },
			)
		}
		SettingsGroup {
			SettingsRow(
				icon = AppIcon.Bell,
				title = stringResource(Res.string.notifications_title),
				selected = state.selectedSection == SettingsSection.Notifications,
				onClick = { onSectionSelected(SettingsSection.Notifications) },
			)
			SettingsDivider()
			SettingsRow(
				icon = AppIcon.Storage,
				title = stringResource(Res.string.storage_title),
				selected = state.selectedSection == SettingsSection.Storage,
				onClick = { onSectionSelected(SettingsSection.Storage) },
			)
		}
		SettingsGroup {
			SettingsRow(
				icon = AppIcon.Globe,
				title = stringResource(Res.string.settings_network_title),
				value = relayModeLabel(state.savedRelaySettings.mode),
				selected = state.selectedSection == SettingsSection.Network,
				onClick = { onSectionSelected(SettingsSection.Network) },
			)
		}
		SettingsGroup {
			SettingsRow(
				icon = AppIcon.Info,
				title = stringResource(Res.string.about_title),
				selected = state.selectedSection == SettingsSection.About,
				iconTone = SettingsIconTone.Neutral,
				onClick = { onSectionSelected(SettingsSection.About) },
			)
		}
	}
}

@Composable
private fun relayModeLabel(mode: RelayMode): String = when (mode) {
	RelayMode.Automatic -> stringResource(Res.string.relay_mode_automatic)
	RelayMode.StrictCustom -> stringResource(Res.string.relay_mode_custom)
	RelayMode.CustomWithDirectFallback -> stringResource(Res.string.relay_mode_custom_direct_fallback)
	RelayMode.LocalOnly -> stringResource(Res.string.relay_mode_local_only)
}

@Composable
private fun themeModeLabel(mode: ThemeMode): String = when (mode) {
	ThemeMode.System -> stringResource(Res.string.appearance_system_mode)
	ThemeMode.Light -> stringResource(Res.string.appearance_light_mode)
	ThemeMode.Dark -> stringResource(Res.string.appearance_dark_mode)
}
