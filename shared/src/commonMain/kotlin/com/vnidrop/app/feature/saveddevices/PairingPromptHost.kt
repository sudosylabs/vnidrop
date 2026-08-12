package com.vnidrop.app.feature.saveddevices

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.pairing_accept
import vnidrop.shared.generated.resources.pairing_allow_body
import vnidrop.shared.generated.resources.pairing_allow_confirm
import vnidrop.shared.generated.resources.pairing_allow_title
import vnidrop.shared.generated.resources.pairing_decline
import vnidrop.shared.generated.resources.pairing_request_body
import vnidrop.shared.generated.resources.pairing_request_title

@Composable
fun PairingPromptHost(
	state: PairingPromptState,
	onAccept: () -> Unit,
	onDecline: () -> Unit,
	onDismiss: () -> Unit,
) {
	val prompt = state.prompt ?: return
	val colors = LocalVniDropColors.current
	val deviceLabel = prompt.remoteDisplayName()?.takeIf(String::isNotBlank)
		?: shortDeviceLabel(prompt.peerEndpointId())
	Dialog(
		onDismissRequest = onDismiss,
		properties = DialogProperties(
			dismissOnBackPress = true,
			dismissOnClickOutside = true,
			usePlatformDefaultWidth = false,
		),
	) {
		Surface(
			modifier = Modifier.padding(24.dp).widthIn(max = 440.dp).fillMaxWidth(),
			shape = RoundedCornerShape(24.dp),
			color = colors.backgroundDialog,
			shadowElevation = 16.dp,
		) {
			Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
				Surface(shape = RoundedCornerShape(14.dp), color = colors.backgroundSelection) {
					PlatformIcon(
						AppIcon.ShieldCheck,
						contentDescription = null,
						tint = colors.brandLink,
						modifier = Modifier.padding(11.dp).size(24.dp),
					)
				}
				when (prompt) {
					is PairingPrompt.Eligibility -> {
						Text(
							stringResource(Res.string.pairing_allow_title),
							style = MaterialTheme.typography.headlineSmall,
							fontWeight = FontWeight.Bold,
						)
						Text(
							stringResource(Res.string.pairing_allow_body),
							style = MaterialTheme.typography.bodyLarge,
							color = colors.foregroundLight,
						)
						Text(
							deviceLabel,
							style = MaterialTheme.typography.bodySmall,
							color = colors.foregroundLighter,
						)
						PromptActions(
							primary = stringResource(Res.string.pairing_allow_confirm),
							secondary = stringResource(Res.string.pairing_decline),
							busy = state.busy,
							onPrimary = onAccept,
							onSecondary = onDecline,
						)
					}
					is PairingPrompt.IncomingRequest -> {
						Text(
							stringResource(Res.string.pairing_request_title),
							style = MaterialTheme.typography.headlineSmall,
							fontWeight = FontWeight.Bold,
						)
						Text(
							stringResource(Res.string.pairing_request_body, deviceLabel),
							style = MaterialTheme.typography.bodyLarge,
							color = colors.foregroundLight,
						)
						PromptActions(
							primary = stringResource(Res.string.pairing_accept),
							secondary = stringResource(Res.string.pairing_decline),
							busy = state.busy,
							onPrimary = onAccept,
							onSecondary = onDecline,
						)
					}
				}
			}
		}
	}
}

@Composable
private fun PromptActions(
	primary: String,
	secondary: String,
	busy: Boolean,
	onPrimary: () -> Unit,
	onSecondary: () -> Unit,
) {
	BoxWithConstraints(Modifier.fillMaxWidth()) {
		if (maxWidth < 330.dp) {
			Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
				PrimaryButton(primary, onPrimary, Modifier.fillMaxWidth(), !busy)
				SecondaryButton(secondary, onSecondary, Modifier.fillMaxWidth(), !busy)
			}
		} else {
			Row(verticalAlignment = Alignment.CenterVertically) {
				if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
				Spacer(Modifier.weight(1f))
				SecondaryButton(secondary, onSecondary, enabled = !busy)
				Spacer(Modifier.width(10.dp))
				PrimaryButton(primary, onPrimary, enabled = !busy)
			}
		}
	}
}

private fun PairingPrompt.peerEndpointId(): String = when (this) {
	is PairingPrompt.Eligibility -> peerEndpointId
	is PairingPrompt.IncomingRequest -> peerEndpointId
}

private fun PairingPrompt.remoteDisplayName(): String? = when (this) {
	is PairingPrompt.Eligibility -> remoteDisplayName
	is PairingPrompt.IncomingRequest -> remoteDisplayName
}

private fun shortDeviceLabel(endpointId: String): String =
	if (endpointId.length <= 12) endpointId else endpointId.take(8) + "…"
