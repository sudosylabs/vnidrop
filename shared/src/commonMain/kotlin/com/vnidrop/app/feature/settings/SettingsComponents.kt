package com.vnidrop.app.feature.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.vnidrop.app.isDesktop
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_back

internal enum class SettingsIconTone { Brand, Neutral }

@Composable
internal fun SettingsTopBar(title: String, onBack: () -> Unit, showBack: Boolean) {
	Row(
		modifier = Modifier.fillMaxWidth(),
		verticalAlignment = Alignment.CenterVertically,
		horizontalArrangement = Arrangement.spacedBy(8.dp),
	) {
		if (showBack) {
			IconButton(onClick = onBack) {
				PlatformIcon(
					AppIcon.ArrowBack,
					contentDescription = stringResource(Res.string.button_back),
					tint = LocalVniDropColors.current.foregroundDefault,
				)
			}
		}
		Text(title, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
	}
}

@Composable
internal fun SettingsGroup(content: @Composable ColumnScope.() -> Unit) {
	Column(modifier = Modifier.fillMaxWidth(), content = content)
}

@Composable
internal fun SettingsRow(
	icon: AppIcon,
	title: String,
	value: String? = null,
	subtitle: String? = null,
	selected: Boolean = false,
	iconTone: SettingsIconTone = SettingsIconTone.Neutral,
	onClick: (() -> Unit)? = null,
	showsDisclosure: Boolean = onClick != null,
	trailing: @Composable (() -> Unit)? = null,
) {
	val colors = LocalVniDropColors.current
	val desktop = LocalUiPlatform.current.isDesktop
	Row(
		modifier = Modifier
			.fillMaxWidth()
			.heightIn(min = if (desktop) 52.dp else 64.dp)
			.then(
				if (selected) {
					Modifier.background(colors.backgroundSelection, RoundedCornerShape(8.dp))
				} else {
					Modifier
				},
			)
			.then(if (onClick == null) Modifier else Modifier.clickable(onClick = onClick))
			.padding(horizontal = 8.dp, vertical = 10.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		SettingsLeadingIcon(icon, iconTone, selected)
		Spacer(Modifier.width(12.dp))
		Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
			Text(
				title,
				style = MaterialTheme.typography.bodyLarge,
				fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Medium,
				maxLines = 1,
				overflow = TextOverflow.Ellipsis,
			)
			if (!desktop) {
				value?.let {
					Text(
						it,
						color = colors.foregroundLighter,
						style = MaterialTheme.typography.bodySmall,
						maxLines = 1,
						overflow = TextOverflow.Ellipsis,
					)
				}
			}
			subtitle?.let {
				Text(
					it,
					color = colors.foregroundLighter,
					style = MaterialTheme.typography.bodySmall,
					maxLines = 2,
					overflow = TextOverflow.Ellipsis,
				)
			}
		}
		if (desktop) value?.let {
			Text(
				it,
				modifier = Modifier.weight(0.7f).padding(start = 12.dp),
				color = colors.foregroundLighter,
				style = MaterialTheme.typography.bodySmall,
				maxLines = 1,
				overflow = TextOverflow.Ellipsis,
				textAlign = TextAlign.End,
			)
		}
		when {
			trailing != null -> {
				Spacer(Modifier.width(10.dp))
				trailing()
			}
			onClick != null && showsDisclosure -> {
				Spacer(Modifier.width(8.dp))
				PlatformIcon(AppIcon.ChevronRight, contentDescription = null, tint = colors.foregroundLighter, modifier = Modifier.size(18.dp))
			}
		}
	}
}

@Composable
internal fun SettingsToggleRow(
	icon: AppIcon,
	title: String,
	description: String,
	checked: Boolean,
	enabled: Boolean,
	onCheckedChange: (Boolean) -> Unit,
) {
	val colors = LocalVniDropColors.current
	Row(
		modifier = Modifier
			.fillMaxWidth()
			.alpha(if (enabled) 1f else 0.55f)
			.toggleable(
				value = checked,
				enabled = enabled,
				role = Role.Switch,
				onValueChange = onCheckedChange,
			)
			.padding(horizontal = 14.dp, vertical = 12.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		SettingsLeadingIcon(icon, SettingsIconTone.Neutral, selected = false)
		Spacer(Modifier.width(12.dp))
		Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
			Text(title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium)
			Text(description, color = colors.foregroundLighter, style = MaterialTheme.typography.bodySmall)
		}
		Spacer(Modifier.width(16.dp))
		Switch(
			checked = checked,
			onCheckedChange = null,
			enabled = enabled,
			colors = SwitchDefaults.colors(
				checkedThumbColor = Color.White,
				checkedTrackColor = colors.brandButton,
			),
		)
	}
}

@Composable
internal fun SettingsDivider(startPadding: Dp = 52.dp) {
	HorizontalDivider(
		modifier = Modifier.padding(start = startPadding),
		color = LocalVniDropColors.current.borderDefault.copy(alpha = 0.72f),
	)
}

@Composable
internal fun InfoItem(title: String, value: String) {
	Column(
		modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
		verticalArrangement = Arrangement.spacedBy(3.dp),
	) {
		Text(title, color = LocalVniDropColors.current.foregroundLighter, style = MaterialTheme.typography.labelLarge)
		Text(value, style = MaterialTheme.typography.bodyLarge)
	}
}

@Composable
private fun SettingsLeadingIcon(icon: AppIcon, tone: SettingsIconTone, selected: Boolean) {
	val colors = LocalVniDropColors.current
	val foreground = if (selected || tone == SettingsIconTone.Brand) colors.brandLink else colors.foregroundLight
	Box(
		modifier = Modifier.size(32.dp),
		contentAlignment = Alignment.Center,
	) {
		PlatformIcon(icon, contentDescription = null, tint = foreground, modifier = Modifier.size(21.dp))
	}
}
