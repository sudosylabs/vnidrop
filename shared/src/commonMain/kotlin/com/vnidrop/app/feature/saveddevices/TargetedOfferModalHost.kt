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
import com.vnidrop.app.ui.components.NotificationEnableCallout
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.offer_accept
import vnidrop.shared.generated.resources.offer_body
import vnidrop.shared.generated.resources.offer_decline
import vnidrop.shared.generated.resources.offer_title
import vnidrop.shared.generated.resources.saved_devices_unnamed

@Composable
fun TargetedOfferModalHost(
	state: TargetedOfferState,
	onAccept: (String) -> Unit,
	onDecline: (String) -> Unit,
	showNotificationPrompt: Boolean = false,
	onEnableNotifications: () -> Unit = {},
) {
	val offer = state.current ?: return
	val busy = offer.transferId in state.respondingIds
	val colors = LocalVniDropColors.current
	val device = state.currentSenderDisplayName?.takeIf(String::isNotBlank)
		?: shortEndpoint(offer.senderEndpointId)
	Dialog(
		onDismissRequest = {},
		properties = DialogProperties(
			dismissOnBackPress = false,
			dismissOnClickOutside = false,
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
						AppIcon.Download,
						contentDescription = null,
						tint = colors.brandLink,
						modifier = Modifier.padding(11.dp).size(24.dp),
					)
				}
				Text(
					stringResource(Res.string.offer_title),
					style = MaterialTheme.typography.headlineSmall,
					fontWeight = FontWeight.Bold,
				)
				Text(
					stringResource(
						Res.string.offer_body,
						device.ifBlank { stringResource(Res.string.saved_devices_unnamed) },
						offer.transferName.ifBlank { stringResource(Res.string.saved_devices_unnamed) },
					),
					style = MaterialTheme.typography.bodyLarge,
					color = colors.foregroundLight,
				)
				if (showNotificationPrompt) {
					NotificationEnableCallout(onEnable = onEnableNotifications)
				}
				BoxWithConstraints(Modifier.fillMaxWidth()) {
					if (maxWidth < 330.dp) {
						Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
							PrimaryButton(
								stringResource(Res.string.offer_accept),
								{ onAccept(offer.transferId) },
								Modifier.fillMaxWidth(),
								!busy,
							)
							SecondaryButton(
								stringResource(Res.string.offer_decline),
								{ onDecline(offer.transferId) },
								Modifier.fillMaxWidth(),
								!busy,
							)
						}
					} else {
						Row(verticalAlignment = Alignment.CenterVertically) {
							if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
							Spacer(Modifier.weight(1f))
							SecondaryButton(
								stringResource(Res.string.offer_decline),
								{ onDecline(offer.transferId) },
								enabled = !busy,
							)
							Spacer(Modifier.width(10.dp))
							PrimaryButton(
								stringResource(Res.string.offer_accept),
								{ onAccept(offer.transferId) },
								enabled = !busy,
							)
						}
					}
				}
			}
		}
	}
}

private fun shortEndpoint(endpointId: String): String =
	if (endpointId.length <= 12) endpointId else endpointId.take(8) + "…"
