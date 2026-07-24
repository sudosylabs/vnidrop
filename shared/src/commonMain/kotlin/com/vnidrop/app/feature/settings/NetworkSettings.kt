package com.vnidrop.app.feature.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.RelayMode
import com.vnidrop.app.core.usesCustomRelayUrls
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.approval_endpoint_id
import vnidrop.shared.generated.resources.relay_add_url
import vnidrop.shared.generated.resources.relay_apply
import vnidrop.shared.generated.resources.relay_apply_active_transfers
import vnidrop.shared.generated.resources.relay_apply_failed
import vnidrop.shared.generated.resources.relay_apply_restart_description
import vnidrop.shared.generated.resources.relay_applying
import vnidrop.shared.generated.resources.relay_custom_urls_help
import vnidrop.shared.generated.resources.relay_custom_urls_label
import vnidrop.shared.generated.resources.relay_mode_automatic
import vnidrop.shared.generated.resources.relay_mode_automatic_description
import vnidrop.shared.generated.resources.relay_mode_custom
import vnidrop.shared.generated.resources.relay_mode_custom_description
import vnidrop.shared.generated.resources.relay_mode_custom_direct_fallback
import vnidrop.shared.generated.resources.relay_mode_custom_direct_fallback_description
import vnidrop.shared.generated.resources.relay_mode_local_only
import vnidrop.shared.generated.resources.relay_mode_local_only_description
import vnidrop.shared.generated.resources.relay_privacy_description
import vnidrop.shared.generated.resources.relay_remove_url
import vnidrop.shared.generated.resources.relay_restore_failed
import vnidrop.shared.generated.resources.relay_strict_warning
import vnidrop.shared.generated.resources.relay_validation_duplicate_url
import vnidrop.shared.generated.resources.relay_validation_https_required
import vnidrop.shared.generated.resources.relay_validation_invalid_url
import vnidrop.shared.generated.resources.relay_validation_missing_url
import vnidrop.shared.generated.resources.relay_validation_too_many_urls
import vnidrop.shared.generated.resources.settings_network_title

@Composable
internal fun NetworkSettings(
	state: SettingsState,
	onModeChanged: (RelayMode) -> Unit,
	onUrlChanged: (Int, String) -> Unit,
	onAddUrl: () -> Unit,
	onRemoveUrl: (Int) -> Unit,
	onApply: () -> Unit,
	onBack: () -> Unit,
	showBack: Boolean,
) {
	val colors = LocalVniDropColors.current
	Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
		SettingsTopBar(stringResource(Res.string.settings_network_title), onBack, showBack)
		SettingsGroup {
			RelayModeRow(
				icon = AppIcon.Globe,
				title = stringResource(Res.string.relay_mode_automatic),
				description = stringResource(Res.string.relay_mode_automatic_description),
				selected = state.relayMode == RelayMode.Automatic,
				enabled = !state.isApplyingRelaySettings,
				onClick = { onModeChanged(RelayMode.Automatic) },
			)
			SettingsDivider()
			RelayModeRow(
				icon = AppIcon.Shield,
				title = stringResource(Res.string.relay_mode_custom),
				description = stringResource(Res.string.relay_mode_custom_description),
				selected = state.relayMode == RelayMode.StrictCustom,
				enabled = !state.isApplyingRelaySettings,
				onClick = { onModeChanged(RelayMode.StrictCustom) },
			)
			SettingsDivider()
			RelayModeRow(
				icon = AppIcon.Radio,
				title = stringResource(Res.string.relay_mode_custom_direct_fallback),
				description = stringResource(Res.string.relay_mode_custom_direct_fallback_description),
				selected = state.relayMode == RelayMode.CustomWithDirectFallback,
				enabled = !state.isApplyingRelaySettings,
				onClick = { onModeChanged(RelayMode.CustomWithDirectFallback) },
			)
			SettingsDivider()
			RelayModeRow(
				icon = AppIcon.CloudOff,
				title = stringResource(Res.string.relay_mode_local_only),
				description = stringResource(Res.string.relay_mode_local_only_description),
				selected = state.relayMode == RelayMode.LocalOnly,
				enabled = !state.isApplyingRelaySettings,
				onClick = { onModeChanged(RelayMode.LocalOnly) },
			)
		}
		Text(
			stringResource(Res.string.relay_privacy_description),
			color = colors.foregroundLighter,
			style = MaterialTheme.typography.bodySmall,
		)
		state.endpointId?.takeIf(String::isNotBlank)?.let { endpointId ->
			SelectionContainer {
				Text(
					stringResource(Res.string.approval_endpoint_id, endpointId),
					color = colors.foregroundLighter,
					style = MaterialTheme.typography.bodySmall,
				)
			}
		}
		if (state.relayMode.usesCustomRelayUrls) {
			Text(
				stringResource(Res.string.relay_custom_urls_label),
				style = MaterialTheme.typography.titleSmall,
				fontWeight = FontWeight.SemiBold,
			)
			SettingsGroup {
				state.relayUrls.forEachIndexed { index, url ->
					RelayUrlRow(
						value = url,
						error = relayUrlErrorText(state.relayInputError, index),
						enabled = !state.isApplyingRelaySettings,
						onValueChange = { onUrlChanged(index, it) },
						onRemove = { onRemoveUrl(index) },
					)
					if (index < state.relayUrls.lastIndex) {
						SettingsDivider(startPadding = 14.dp)
					}
				}
				if (state.relayUrls.isNotEmpty()) {
					SettingsDivider(startPadding = 14.dp)
				}
				TextButton(
					onClick = onAddUrl,
					enabled = state.relayUrls.size < MaximumRelayUrls && !state.isApplyingRelaySettings,
					modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
				) {
					PlatformIcon(AppIcon.Add, contentDescription = null, modifier = Modifier.size(18.dp))
					Spacer(Modifier.width(8.dp))
					Text(stringResource(Res.string.relay_add_url))
				}
			}
			Text(
				stringResource(Res.string.relay_custom_urls_help),
				color = colors.foregroundLighter,
				style = MaterialTheme.typography.bodySmall,
			)
			if (state.relayMode == RelayMode.StrictCustom) {
				Text(
					stringResource(Res.string.relay_strict_warning),
					color = colors.foregroundLight,
					style = MaterialTheme.typography.bodySmall,
					fontWeight = FontWeight.Medium,
				)
			}
		}
		relayGlobalErrorText(state)?.let { error ->
			Text(
				error,
				color = colors.destructiveDefault,
				style = MaterialTheme.typography.bodySmall,
				fontWeight = FontWeight.Medium,
			)
		}
		Text(
			stringResource(Res.string.relay_apply_restart_description),
			color = colors.foregroundLighter,
			style = MaterialTheme.typography.bodySmall,
		)
		PrimaryButton(
			text = stringResource(
				if (state.isApplyingRelaySettings) Res.string.relay_applying else Res.string.relay_apply,
			),
			onClick = onApply,
			modifier = Modifier.fillMaxWidth(),
			enabled = state.hasRelaySettingsChanges &&
				!state.isApplyingRelaySettings &&
				!state.hasActiveNetworkWork,
		)
	}
}

@Composable
private fun RelayModeRow(
	icon: AppIcon,
	title: String,
	description: String,
	selected: Boolean,
	enabled: Boolean,
	onClick: () -> Unit,
) {
	SettingsRow(
		icon = icon,
		title = title,
		subtitle = description,
		selected = selected,
		onClick = onClick.takeIf { enabled },
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

@Composable
private fun RelayUrlRow(
	value: String,
	error: String?,
	enabled: Boolean,
	onValueChange: (String) -> Unit,
	onRemove: () -> Unit,
) {
	val colors = LocalVniDropColors.current
	Row(
		modifier = Modifier
			.fillMaxWidth()
			.padding(start = 14.dp, end = 6.dp, top = 10.dp, bottom = 10.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		OutlinedTextField(
			value = value,
			onValueChange = onValueChange,
			modifier = Modifier.weight(1f),
			enabled = enabled,
			singleLine = true,
			isError = error != null,
			placeholder = { Text("https://relay.example.com") },
			supportingText = error?.let {
				{
					Text(it, color = colors.destructiveDefault)
				}
			},
			shape = RoundedCornerShape(8.dp),
		)
		IconButton(onClick = onRemove, enabled = enabled) {
			PlatformIcon(
				AppIcon.Delete,
				contentDescription = stringResource(Res.string.relay_remove_url),
				tint = colors.destructiveDefault,
			)
		}
	}
}

@Composable
private fun relayUrlErrorText(error: RelaySettingsInputError?, index: Int): String? = when (error) {
	is RelaySettingsInputError.HttpsRequired -> if (error.line == index + 1) {
		stringResource(Res.string.relay_validation_https_required, error.line)
	} else {
		null
	}
	is RelaySettingsInputError.InvalidUrl -> if (error.line == index + 1) {
		stringResource(Res.string.relay_validation_invalid_url, error.line)
	} else {
		null
	}
	is RelaySettingsInputError.DuplicateUrl -> if (error.line == index + 1) {
		stringResource(Res.string.relay_validation_duplicate_url, error.line)
	} else {
		null
	}
	RelaySettingsInputError.MissingUrl,
	is RelaySettingsInputError.TooManyUrls,
	null,
	-> null
}

@Composable
private fun relayGlobalErrorText(state: SettingsState): String? {
	if (state.hasActiveNetworkWork || state.relayApplyError == RelaySettingsApplyError.ActiveTransfers) {
		return stringResource(Res.string.relay_apply_active_transfers)
	}
	state.relayInputError?.let { error ->
		return when (error) {
			RelaySettingsInputError.MissingUrl -> stringResource(Res.string.relay_validation_missing_url)
			is RelaySettingsInputError.TooManyUrls -> stringResource(
				Res.string.relay_validation_too_many_urls,
				error.maximum,
			)
			is RelaySettingsInputError.HttpsRequired,
			is RelaySettingsInputError.InvalidUrl,
			is RelaySettingsInputError.DuplicateUrl,
			-> null
		}
	}
	return when (state.relayApplyError) {
		RelaySettingsApplyError.ApplyFailed -> stringResource(Res.string.relay_apply_failed)
		RelaySettingsApplyError.RestoreFailed -> stringResource(Res.string.relay_restore_failed)
		RelaySettingsApplyError.ActiveTransfers,
		null,
		-> null
	}
}
