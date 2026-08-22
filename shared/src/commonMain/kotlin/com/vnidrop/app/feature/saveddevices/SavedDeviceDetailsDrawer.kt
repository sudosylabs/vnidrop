package com.vnidrop.app.feature.saveddevices

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.ui.components.AdaptiveDrawer
import com.vnidrop.app.ui.components.DestructiveQuietButton
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.components.QuietButton
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_cancel
import vnidrop.shared.generated.resources.saved_devices_authenticated_name
import vnidrop.shared.generated.resources.saved_devices_block_action
import vnidrop.shared.generated.resources.saved_devices_block_confirm_body
import vnidrop.shared.generated.resources.saved_devices_block_confirm_title
import vnidrop.shared.generated.resources.saved_devices_endpoint
import vnidrop.shared.generated.resources.saved_devices_forget_action
import vnidrop.shared.generated.resources.saved_devices_forget_confirm_body
import vnidrop.shared.generated.resources.saved_devices_forget_confirm_title
import vnidrop.shared.generated.resources.saved_devices_label_action
import vnidrop.shared.generated.resources.saved_devices_more_actions
import vnidrop.shared.generated.resources.saved_devices_send_action
import vnidrop.shared.generated.resources.saved_devices_transfer_empty
import vnidrop.shared.generated.resources.saved_devices_transfers_title

private enum class DeviceDestructiveAction { Forget, Block }

@Composable
internal fun SavedDeviceDetailsDrawer(
	device: SavedDeviceModel,
	transfers: List<SavedDeviceTransferItem>,
	busy: Boolean,
	busyTransferIds: Set<String>,
	windowClass: WindowClass,
	onDismiss: () -> Unit,
	onSend: () -> Unit,
	onOpenLabel: () -> Unit,
	onForget: () -> Unit,
	onBlock: () -> Unit,
	onTransferAction: (String, SavedDeviceTransferAction) -> Unit,
) {
	var menuExpanded by remember(device.endpointId) { mutableStateOf(false) }
	var pendingAction by remember(device.endpointId) { mutableStateOf<DeviceDestructiveAction?>(null) }
	val colors = LocalVniDropColors.current
	val title = device.displayName()

	AdaptiveDrawer(windowClass = windowClass, onDismissRequest = onDismiss) {
		LazyColumn(
			modifier = Modifier.fillMaxWidth().heightIn(min = 320.dp, max = 720.dp),
			contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 20.dp, bottom = 28.dp),
			verticalArrangement = Arrangement.spacedBy(18.dp),
		) {
			item(key = "device-header") {
				Row(Modifier.fillMaxWidth().padding(end = 44.dp), verticalAlignment = Alignment.CenterVertically) {
					PlatformIcon(AppIcon.Device, contentDescription = null, modifier = Modifier.size(30.dp), tint = colors.foregroundLight)
					Spacer(Modifier.width(14.dp))
					Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
						Text(
							title,
							style = MaterialTheme.typography.titleLarge,
							fontWeight = FontWeight.Bold,
							maxLines = 1,
							overflow = TextOverflow.Ellipsis,
							modifier = Modifier.semantics { heading() },
						)
						device.remoteDisplayName
							?.takeIf { device.localLabel?.isNotBlank() == true && it.isNotBlank() }
							?.let {
								Text(
									stringResource(Res.string.saved_devices_authenticated_name, it),
									style = MaterialTheme.typography.bodySmall,
									color = colors.foregroundLight,
								)
							}
					}
					if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
				}
			}
			item(key = "device-identity") {
				Text(
					stringResource(Res.string.saved_devices_endpoint, shortDeviceEndpoint(device.endpointId)),
					style = MaterialTheme.typography.bodySmall,
					color = colors.foregroundLighter,
				)
			}
			item(key = "send") {
				Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
					PrimaryButton(
						stringResource(Res.string.saved_devices_send_action),
						onClick = onSend,
						modifier = Modifier.weight(1f),
						enabled = !busy,
						leadingIcon = { PlatformIcon(AppIcon.Send, contentDescription = null, modifier = Modifier.size(18.dp)) },
					)
					Spacer(Modifier.width(8.dp))
					Box {
						IconButton(onClick = { menuExpanded = true }, enabled = !busy) {
							PlatformIcon(
								AppIcon.MoreVertical,
								contentDescription = stringResource(Res.string.saved_devices_more_actions, title),
							)
						}
						DropdownMenu(expanded = menuExpanded, onDismissRequest = { menuExpanded = false }) {
							DropdownMenuItem(
								text = { Text(stringResource(Res.string.saved_devices_label_action)) },
								onClick = { menuExpanded = false; onOpenLabel() },
								leadingIcon = { PlatformIcon(AppIcon.User, contentDescription = null) },
							)
							DropdownMenuItem(
								text = { Text(stringResource(Res.string.saved_devices_forget_action)) },
								onClick = { menuExpanded = false; pendingAction = DeviceDestructiveAction.Forget },
								leadingIcon = { PlatformIcon(AppIcon.UserOff, contentDescription = null) },
							)
							DropdownMenuItem(
								text = { Text(stringResource(Res.string.saved_devices_block_action)) },
								onClick = { menuExpanded = false; pendingAction = DeviceDestructiveAction.Block },
								leadingIcon = { PlatformIcon(AppIcon.Lock, contentDescription = null) },
							)
						}
					}
				}
			}
			item(key = "transfer-heading") {
				Text(
					stringResource(Res.string.saved_devices_transfers_title),
					style = MaterialTheme.typography.titleMedium,
					fontWeight = FontWeight.SemiBold,
					modifier = Modifier.padding(top = 6.dp).semantics { heading() },
				)
			}
			if (transfers.isEmpty()) {
				item(key = "transfer-empty") {
					Text(
						stringResource(Res.string.saved_devices_transfer_empty),
						style = MaterialTheme.typography.bodyMedium,
						color = colors.foregroundLight,
					)
				}
			} else {
				targetedTransferItems(
					transfers = transfers,
					busyTransferIds = busyTransferIds,
					onAction = onTransferAction,
					showHeading = false,
				)
			}
		}
	}

	pendingAction?.let { action ->
		val isBlock = action == DeviceDestructiveAction.Block
		AlertDialog(
			onDismissRequest = { pendingAction = null },
			title = {
				Text(stringResource(if (isBlock) Res.string.saved_devices_block_confirm_title else Res.string.saved_devices_forget_confirm_title))
			},
			text = {
				Text(
					stringResource(
						if (isBlock) Res.string.saved_devices_block_confirm_body else Res.string.saved_devices_forget_confirm_body,
						title,
					),
				)
			},
			confirmButton = {
				DestructiveQuietButton(
					text = stringResource(if (isBlock) Res.string.saved_devices_block_action else Res.string.saved_devices_forget_action),
					onClick = {
						pendingAction = null
						onDismiss()
						if (isBlock) onBlock() else onForget()
					},
				)
			},
			dismissButton = {
				QuietButton(stringResource(Res.string.button_cancel), onClick = { pendingAction = null })
			},
		)
	}
}
