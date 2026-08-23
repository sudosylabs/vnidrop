package com.vnidrop.app.feature.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.vnidrop.app.isDesktop
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.about_bug_report
import vnidrop.shared.generated.resources.about_description
import vnidrop.shared.generated.resources.about_is_direct
import vnidrop.shared.generated.resources.about_is_encrypted
import vnidrop.shared.generated.resources.about_is_in_control
import vnidrop.shared.generated.resources.about_is_no_account
import vnidrop.shared.generated.resources.about_is_open
import vnidrop.shared.generated.resources.about_is_title
import vnidrop.shared.generated.resources.about_isnt_cloud
import vnidrop.shared.generated.resources.about_isnt_public
import vnidrop.shared.generated.resources.about_isnt_sync
import vnidrop.shared.generated.resources.about_isnt_title
import vnidrop.shared.generated.resources.about_license_label
import vnidrop.shared.generated.resources.about_privacy_capability
import vnidrop.shared.generated.resources.about_privacy_deny
import vnidrop.shared.generated.resources.about_privacy_local
import vnidrop.shared.generated.resources.about_privacy_policy_label
import vnidrop.shared.generated.resources.about_privacy_relay
import vnidrop.shared.generated.resources.about_privacy_title
import vnidrop.shared.generated.resources.about_tagline
import vnidrop.shared.generated.resources.about_title
import vnidrop.shared.generated.resources.device_model_title
import vnidrop.shared.generated.resources.os_version_title
import vnidrop.shared.generated.resources.value_unavailable
import vnidrop.shared.generated.resources.version_title

private val PrivacyPolicyUrl = com.vnidrop.app.AppConfig.PRIVACY_POLICY_URL

@Composable
internal fun AboutSettings(
	state: SettingsState,
	onReportBug: () -> Unit,
	onBack: () -> Unit,
	showBack: Boolean,
) {
	val colors = LocalVniDropColors.current
	val unavailable = stringResource(Res.string.value_unavailable)
	val info = state.deviceInfo
	val uriHandler = LocalUriHandler.current
	Column(verticalArrangement = Arrangement.spacedBy(24.dp)) {
		SettingsTopBar(stringResource(Res.string.about_title), onBack, showBack)
		Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
			Text(
				stringResource(Res.string.about_tagline),
				style = MaterialTheme.typography.titleLarge,
				fontWeight = FontWeight.SemiBold,
			)
			Text(
				stringResource(Res.string.about_description),
				color = colors.foregroundLight,
				style = MaterialTheme.typography.bodyLarge,
			)
		}

		AboutSection(
			title = stringResource(Res.string.about_is_title),
			points = listOf(
				AppIcon.Send to stringResource(Res.string.about_is_direct),
				AppIcon.UserOff to stringResource(Res.string.about_is_no_account),
				AppIcon.ShieldCheck to stringResource(Res.string.about_is_in_control),
				AppIcon.Lock to stringResource(Res.string.about_is_encrypted),
				AppIcon.Code to stringResource(Res.string.about_is_open),
			),
		)
		AboutSection(
			title = stringResource(Res.string.about_isnt_title),
			points = listOf(
				AppIcon.CloudOff to stringResource(Res.string.about_isnt_cloud),
				AppIcon.Sync to stringResource(Res.string.about_isnt_sync),
				AppIcon.Megaphone to stringResource(Res.string.about_isnt_public),
			),
		)
		AboutSection(
			title = stringResource(Res.string.about_privacy_title),
			points = listOf(
				AppIcon.QrCode to stringResource(Res.string.about_privacy_capability),
				AppIcon.Hand to stringResource(Res.string.about_privacy_deny),
				AppIcon.Radio to stringResource(Res.string.about_privacy_relay),
				AppIcon.Storage to stringResource(Res.string.about_privacy_local),
			),
		)

		SettingsGroup {
			AboutInfoItem(stringResource(Res.string.version_title), state.appVersion)
			SettingsDivider(startPadding = 16.dp)
			AboutInfoItem(stringResource(Res.string.device_model_title), info?.deviceModel.orUnavailable(unavailable))
			SettingsDivider(startPadding = 16.dp)
			AboutInfoItem(stringResource(Res.string.os_version_title), info?.operatingSystem ?: unavailable)
			SettingsDivider(startPadding = 16.dp)
			AboutInfoItem(stringResource(Res.string.about_license_label), "Apache 2.0")
			SettingsDivider()
			SettingsRow(
				icon = AppIcon.Shield,
				title = stringResource(Res.string.about_privacy_policy_label),
				iconTone = SettingsIconTone.Brand,
				onClick = { uriHandler.openUri(PrivacyPolicyUrl) },
			)
		}

		SettingsGroup {
			SettingsRow(
				icon = AppIcon.Bug,
				title = stringResource(Res.string.about_bug_report),
				iconTone = SettingsIconTone.Neutral,
				onClick = onReportBug,
			)
		}
	}
}

@Composable
private fun AboutSection(
	title: String,
	points: List<Pair<AppIcon, String>>,
) {
	Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
		Text(
			title,
			color = LocalVniDropColors.current.foregroundLighter,
			style = MaterialTheme.typography.titleSmall,
			fontWeight = FontWeight.SemiBold,
		)
		SettingsGroup {
			points.forEachIndexed { index, (icon, text) ->
				if (index > 0) SettingsDivider(startPadding = 54.dp)
				AboutPoint(icon, text)
			}
		}
	}
}

@Composable
private fun AboutPoint(icon: AppIcon, text: String) {
	val colors = LocalVniDropColors.current
	Row(
		modifier = Modifier
			.fillMaxWidth()
			.heightIn(min = 64.dp)
			.padding(horizontal = 16.dp, vertical = 12.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		PlatformIcon(icon, contentDescription = null, tint = colors.foregroundLight, modifier = Modifier.size(24.dp))
		Spacer(Modifier.width(16.dp))
		Text(
			text,
			modifier = Modifier.weight(1f),
			style = MaterialTheme.typography.bodyLarge,
			fontWeight = FontWeight.Normal,
		)
	}
}

@Composable
private fun AboutInfoItem(title: String, value: String) {
	val colors = LocalVniDropColors.current
	val desktop = LocalUiPlatform.current.isDesktop
	Row(
		modifier = Modifier
			.fillMaxWidth()
			.heightIn(min = 56.dp)
			.padding(horizontal = 16.dp, vertical = 12.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		if (desktop) {
			Text(title, modifier = Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
			Spacer(Modifier.width(16.dp))
			Text(
				value,
				modifier = Modifier.weight(0.7f),
				color = colors.foregroundLighter,
				style = MaterialTheme.typography.bodyLarge,
				textAlign = TextAlign.End,
			)
		} else {
			Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
				Text(title, style = MaterialTheme.typography.bodyLarge)
				Text(value, color = colors.foregroundLighter, style = MaterialTheme.typography.bodyMedium)
			}
		}
	}
}

private fun String?.orUnavailable(fallback: String): String = this?.takeIf(String::isNotBlank) ?: fallback
