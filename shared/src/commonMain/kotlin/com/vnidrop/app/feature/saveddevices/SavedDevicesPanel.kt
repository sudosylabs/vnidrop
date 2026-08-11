package com.vnidrop.app.feature.saveddevices

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_cancel
import vnidrop.shared.generated.resources.saved_devices_accept_pairing_action
import vnidrop.shared.generated.resources.saved_devices_block_action
import vnidrop.shared.generated.resources.saved_devices_decline_action
import vnidrop.shared.generated.resources.saved_devices_eligibility_title
import vnidrop.shared.generated.resources.saved_devices_empty
import vnidrop.shared.generated.resources.saved_devices_forget_action
import vnidrop.shared.generated.resources.saved_devices_label_action
import vnidrop.shared.generated.resources.saved_devices_label_clear
import vnidrop.shared.generated.resources.saved_devices_label_placeholder
import vnidrop.shared.generated.resources.saved_devices_label_save
import vnidrop.shared.generated.resources.saved_devices_label_title
import vnidrop.shared.generated.resources.saved_devices_list_title
import vnidrop.shared.generated.resources.saved_devices_pending_incoming
import vnidrop.shared.generated.resources.saved_devices_pending_outgoing
import vnidrop.shared.generated.resources.saved_devices_pending_title
import vnidrop.shared.generated.resources.saved_devices_remember_action
import vnidrop.shared.generated.resources.saved_devices_send_action
import vnidrop.shared.generated.resources.saved_devices_unnamed

@Composable
fun SavedDevicesPanel(
	state: SavedDevicesState,
	onRememberEligible: (String) -> Unit,
	onDeclineEligible: (String) -> Unit,
	onAcceptIncoming: (String) -> Unit,
	onDeclineIncoming: (String) -> Unit,
	onSend: (String) -> Unit,
	onOpenLabel: (String) -> Unit,
	onForget: (String) -> Unit,
	onBlock: (String) -> Unit,
	onLabelDraftChanged: (String) -> Unit,
	onSaveLabel: () -> Unit,
	onClearLabel: () -> Unit,
	onDismissLabel: () -> Unit,
) {
	if (!state.enabled) return
	val colors = LocalVniDropColors.current
	Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
		if (state.eligibilities.isNotEmpty()) {
			Text(
				stringResource(Res.string.saved_devices_eligibility_title),
				style = MaterialTheme.typography.titleMedium,
				fontWeight = FontWeight.SemiBold,
			)
			PanelGroup {
				state.eligibilities.forEach { eligibility ->
					val busy = eligibility.peerEndpointId in state.busyPeerIds
					Column(
						Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
						verticalArrangement = Arrangement.spacedBy(10.dp),
					) {
						Text(shortEndpoint(eligibility.peerEndpointId), style = MaterialTheme.typography.bodyLarge)
						Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
							PrimaryButton(
								stringResource(Res.string.saved_devices_remember_action),
								{ onRememberEligible(eligibility.peerEndpointId) },
								enabled = !busy,
							)
							SecondaryButton(
								stringResource(Res.string.saved_devices_decline_action),
								{ onDeclineEligible(eligibility.peerEndpointId) },
								enabled = !busy,
							)
						}
					}
				}
			}
		}

		if (state.pendingRelationships.isNotEmpty()) {
			Text(
				stringResource(Res.string.saved_devices_pending_title),
				style = MaterialTheme.typography.titleMedium,
				fontWeight = FontWeight.SemiBold,
			)
			PanelGroup {
				state.pendingRelationships.forEach { relationship ->
					val busy = relationship.remoteEndpointId in state.busyPeerIds
					Column(
						Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
						verticalArrangement = Arrangement.spacedBy(10.dp),
					) {
						Text(shortEndpoint(relationship.remoteEndpointId), style = MaterialTheme.typography.bodyLarge)
						Text(
							stringResource(
								when (relationship.state) {
									DeviceRelationshipStateModel.PendingIncoming ->
										Res.string.saved_devices_pending_incoming
									else -> Res.string.saved_devices_pending_outgoing
								},
							),
							style = MaterialTheme.typography.bodySmall,
							color = colors.foregroundLighter,
						)
						if (relationship.state == DeviceRelationshipStateModel.PendingIncoming) {
							Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
								PrimaryButton(
									stringResource(Res.string.saved_devices_accept_pairing_action),
									{ onAcceptIncoming(relationship.remoteEndpointId) },
									enabled = !busy,
								)
								SecondaryButton(
									stringResource(Res.string.saved_devices_decline_action),
									{ onDeclineIncoming(relationship.remoteEndpointId) },
									enabled = !busy,
								)
							}
						}
					}
				}
			}
		}

		Text(
			stringResource(Res.string.saved_devices_list_title),
			style = MaterialTheme.typography.titleMedium,
			fontWeight = FontWeight.SemiBold,
		)
		if (state.savedDevices.isEmpty() && state.eligibilities.isEmpty() && state.pendingRelationships.isEmpty()) {
			Text(
				stringResource(Res.string.saved_devices_empty),
				style = MaterialTheme.typography.bodyMedium,
				color = colors.foregroundLight,
			)
		} else if (state.savedDevices.isNotEmpty()) {
			PanelGroup {
				state.savedDevices.forEach { device ->
					SavedDeviceRow(
						device = device,
						busy = device.endpointId in state.busyPeerIds || state.isSending,
						onSend = { onSend(device.endpointId) },
						onLabel = { onOpenLabel(device.endpointId) },
						onForget = { onForget(device.endpointId) },
						onBlock = { onBlock(device.endpointId) },
					)
				}
			}
		}
	}

	val labelingPeerId = state.labelingPeerId
	if (labelingPeerId != null) {
		AlertDialog(
			onDismissRequest = onDismissLabel,
			title = { Text(stringResource(Res.string.saved_devices_label_title)) },
			text = {
				OutlinedTextField(
					value = state.labelDraft,
					onValueChange = onLabelDraftChanged,
					modifier = Modifier.fillMaxWidth(),
					singleLine = true,
					placeholder = { Text(stringResource(Res.string.saved_devices_label_placeholder)) },
				)
			},
			confirmButton = {
				TextButton(onClick = onSaveLabel) {
					Text(stringResource(Res.string.saved_devices_label_save))
				}
			},
			dismissButton = {
				Row {
					TextButton(onClick = onClearLabel) {
						Text(stringResource(Res.string.saved_devices_label_clear))
					}
					TextButton(onClick = onDismissLabel) {
						Text(stringResource(Res.string.button_cancel))
					}
				}
			},
		)
	}
}

@Composable
private fun PanelGroup(content: @Composable ColumnScope.() -> Unit) {
	val colors = LocalVniDropColors.current
	Card(
		modifier = Modifier.fillMaxWidth(),
		shape = RoundedCornerShape(16.dp),
		colors = CardDefaults.cardColors(containerColor = colors.backgroundSurface200),
		border = BorderStroke(1.dp, colors.borderDefault.copy(alpha = 0.72f)),
		content = { Column(content = content) },
	)
}

@Composable
private fun SavedDeviceRow(
	device: SavedDeviceModel,
	busy: Boolean,
	onSend: () -> Unit,
	onLabel: () -> Unit,
	onForget: () -> Unit,
	onBlock: () -> Unit,
) {
	val colors = LocalVniDropColors.current
	val title = device.localLabel?.takeIf { it.isNotBlank() }
		?: device.remoteDisplayName?.takeIf { it.isNotBlank() }
		?: stringResource(Res.string.saved_devices_unnamed)
	Column(
		Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
		verticalArrangement = Arrangement.spacedBy(10.dp),
	) {
		Text(title, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
		Text(
			shortEndpoint(device.endpointId),
			style = MaterialTheme.typography.bodySmall,
			color = colors.foregroundLighter,
		)
		Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
			PrimaryButton(stringResource(Res.string.saved_devices_send_action), onSend, enabled = !busy)
			SecondaryButton(stringResource(Res.string.saved_devices_label_action), onLabel, enabled = !busy)
		}
		Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
			SecondaryButton(stringResource(Res.string.saved_devices_forget_action), onForget, enabled = !busy)
			SecondaryButton(stringResource(Res.string.saved_devices_block_action), onBlock, enabled = !busy)
		}
	}
}

private fun shortEndpoint(endpointId: String): String =
	if (endpointId.length <= 16) endpointId else endpointId.take(12) + "…"
