package com.vnidrop.app.feature.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.theme.LocalVniDropColors
import com.vnidrop.app.ui.theme.ThemeMode
import com.vnidrop.app.ui.theme.supportsDynamicColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.appearance_auto_description
import vnidrop.shared.generated.resources.appearance_dark_mode
import vnidrop.shared.generated.resources.appearance_dynamic_colors
import vnidrop.shared.generated.resources.appearance_dynamic_colors_description
import vnidrop.shared.generated.resources.appearance_light_mode
import vnidrop.shared.generated.resources.appearance_system_mode
import vnidrop.shared.generated.resources.appearance_title

@Composable
internal fun AppearanceSettings(
	mode: ThemeMode,
	onModeChanged: (ThemeMode) -> Unit,
	onBack: () -> Unit,
	showBack: Boolean,
	useDynamicColors: Boolean = false,
	onDynamicColorsChanged: (Boolean) -> Unit = {},
) {
	Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
		SettingsTopBar(stringResource(Res.string.appearance_title), onBack, showBack)
		SettingsGroup {
			ThemeSettingsRow(AppIcon.SystemTheme, stringResource(Res.string.appearance_system_mode), mode == ThemeMode.System) {
				onModeChanged(ThemeMode.System)
			}
			SettingsDivider()
			ThemeSettingsRow(AppIcon.Moon, stringResource(Res.string.appearance_dark_mode), mode == ThemeMode.Dark) {
				onModeChanged(ThemeMode.Dark)
			}
			SettingsDivider()
			ThemeSettingsRow(AppIcon.Sun, stringResource(Res.string.appearance_light_mode), mode == ThemeMode.Light) {
				onModeChanged(ThemeMode.Light)
			}
		}
		if (supportsDynamicColors()) {
			SettingsToggleRow(
				title = stringResource(Res.string.appearance_dynamic_colors),
				icon = AppIcon.SystemTheme,
				enabled = true,
				description = stringResource(Res.string.appearance_dynamic_colors_description),
				checked = useDynamicColors,
				onCheckedChange = onDynamicColorsChanged,
			)
		}
		if (mode == ThemeMode.System) {
			Text(
				stringResource(Res.string.appearance_auto_description),
				color = LocalVniDropColors.current.foregroundLighter,
				style = MaterialTheme.typography.bodySmall,
			)
		}
	}
}

@Composable
private fun ThemeSettingsRow(icon: AppIcon, title: String, selected: Boolean, onClick: () -> Unit) {
	SettingsRow(
		icon = icon,
		title = title,
		selected = selected,
		onClick = onClick,
		showsDisclosure = false,
		trailing = if (selected) {
			{
				PlatformIcon(
					AppIcon.Check,
					contentDescription = null,
					tint = LocalVniDropColors.current.brandLink,
					modifier = Modifier.size(20.dp),
				)
			}
		} else {
			null
		},
	)
}
