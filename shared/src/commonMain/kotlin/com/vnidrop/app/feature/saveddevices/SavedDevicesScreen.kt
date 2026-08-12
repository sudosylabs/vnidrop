package com.vnidrop.app.feature.saveddevices

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_cancel
import vnidrop.shared.generated.resources.button_retry
import vnidrop.shared.generated.resources.saved_devices_accept_pairing_action
import vnidrop.shared.generated.resources.saved_devices_authenticated_name
import vnidrop.shared.generated.resources.saved_devices_block_action
import vnidrop.shared.generated.resources.saved_devices_block_confirm_body
import vnidrop.shared.generated.resources.saved_devices_block_confirm_title
import vnidrop.shared.generated.resources.saved_devices_decline_action
import vnidrop.shared.generated.resources.saved_devices_description
import vnidrop.shared.generated.resources.saved_devices_eligibility_title
import vnidrop.shared.generated.resources.saved_devices_empty
import vnidrop.shared.generated.resources.saved_devices_endpoint
import vnidrop.shared.generated.resources.saved_devices_forget_action
import vnidrop.shared.generated.resources.saved_devices_forget_confirm_body
import vnidrop.shared.generated.resources.saved_devices_forget_confirm_title
import vnidrop.shared.generated.resources.saved_devices_label_action
import vnidrop.shared.generated.resources.saved_devices_label_clear
import vnidrop.shared.generated.resources.saved_devices_label_placeholder
import vnidrop.shared.generated.resources.saved_devices_label_save
import vnidrop.shared.generated.resources.saved_devices_label_title
import vnidrop.shared.generated.resources.saved_devices_list_title
import vnidrop.shared.generated.resources.saved_devices_load_failed
import vnidrop.shared.generated.resources.saved_devices_loading
import vnidrop.shared.generated.resources.saved_devices_more_actions
import vnidrop.shared.generated.resources.saved_devices_no_pending
import vnidrop.shared.generated.resources.saved_devices_pending_incoming
import vnidrop.shared.generated.resources.saved_devices_pending_outgoing
import vnidrop.shared.generated.resources.saved_devices_pending_title
import vnidrop.shared.generated.resources.saved_devices_remember_action
import vnidrop.shared.generated.resources.saved_devices_send_action
import vnidrop.shared.generated.resources.saved_devices_unnamed

@Composable
internal fun SavedDevicesScreen(
	state: SavedDevicesState,
	windowClass: WindowClass,
	modifier: Modifier = Modifier,
	onRetry: () -> Unit,
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
	val hasContent = state.eligibilities.isNotEmpty() || state.pendingRelationships.isNotEmpty() || state.savedDevices.isNotEmpty()
	Column(
		modifier = modifier
			.fillMaxSize()
			.statusBarsPadding()
			.padding(top = 20.dp),
	) {
		SavedDevicesHeader(Modifier.padding(horizontal = if (windowClass == WindowClass.Desktop) 24.dp else 16.dp))
		Spacer(Modifier.height(16.dp))
		when {
			state.isLoading && !hasContent -> SavedDevicesLoading(Modifier.weight(1f))
			state.loadFailed && !hasContent -> SavedDevicesLoadFailure(onRetry, Modifier.weight(1f))
			windowClass == WindowClass.Desktop -> DesktopSavedDevicesContent(
				state = state,
				onRetry = onRetry,
				onRememberEligible = onRememberEligible,
				onDeclineEligible = onDeclineEligible,
				onAcceptIncoming = onAcceptIncoming,
				onDeclineIncoming = onDeclineIncoming,
				onSend = onSend,
				onOpenLabel = onOpenLabel,
				onForget = onForget,
				onBlock = onBlock,
			)
			else -> CompactSavedDevicesContent(
				state = state,
				onRetry = onRetry,
				onRememberEligible = onRememberEligible,
				onDeclineEligible = onDeclineEligible,
				onAcceptIncoming = onAcceptIncoming,
				onDeclineIncoming = onDeclineIncoming,
				onSend = onSend,
				onOpenLabel = onOpenLabel,
				onForget = onForget,
				onBlock = onBlock,
			)
		}
	}
	SavedDeviceLabelDialog(
		visible = state.labelingPeerId != null,
		label = state.labelDraft,
		onLabelChanged = onLabelDraftChanged,
		onSave = onSaveLabel,
		onClear = onClearLabel,
		onDismiss = onDismissLabel,
	)
}

@Composable
private fun SavedDevicesHeader(modifier: Modifier = Modifier) {
	Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
		Text(
			text = stringResource(Res.string.saved_devices_list_title),
			style = MaterialTheme.typography.headlineLarge,
			fontWeight = FontWeight.Bold,
			modifier = Modifier.semantics { heading() },
		)
		Text(
			text = stringResource(Res.string.saved_devices_description),
			style = MaterialTheme.typography.bodyLarge,
			color = LocalVniDropColors.current.foregroundLight,
		)
	}
}

@Composable
private fun CompactSavedDevicesContent(
	state: SavedDevicesState,
	onRetry: () -> Unit,
	onRememberEligible: (String) -> Unit,
	onDeclineEligible: (String) -> Unit,
	onAcceptIncoming: (String) -> Unit,
	onDeclineIncoming: (String) -> Unit,
	onSend: (String) -> Unit,
	onOpenLabel: (String) -> Unit,
	onForget: (String) -> Unit,
	onBlock: (String) -> Unit,
) {
	LazyColumn(
		modifier = Modifier.fillMaxSize(),
		contentPadding = androidx.compose.foundation.layout.PaddingValues(start = 16.dp, end = 16.dp, bottom = 24.dp),
		verticalArrangement = Arrangement.spacedBy(12.dp),
	) {
		if (state.isLoading) item(key = "loading") { LinearProgressIndicator(Modifier.fillMaxWidth()) }
		if (state.loadFailed) item(key = "load-failed") { InlineLoadFailure(onRetry) }
		pairingItems(
			state = state,
			onRememberEligible = onRememberEligible,
			onDeclineEligible = onDeclineEligible,
			onAcceptIncoming = onAcceptIncoming,
			onDeclineIncoming = onDeclineIncoming,
		)
		item(key = "saved-title") { SectionTitle(stringResource(Res.string.saved_devices_list_title)) }
		if (state.savedDevices.isEmpty()) {
			item(key = "empty") { SavedDevicesEmptyCard() }
		} else {
			items(state.savedDevices, key = { "saved-${it.endpointId}" }) { device ->
				SavedDeviceCard(
					device = device,
					busy = device.endpointId in state.busyPeerIds,
					onSend = { onSend(device.endpointId) },
					onLabel = { onOpenLabel(device.endpointId) },
					onForget = { onForget(device.endpointId) },
					onBlock = { onBlock(device.endpointId) },
				)
			}
		}
	}
}

@Composable
private fun DesktopSavedDevicesContent(
	state: SavedDevicesState,
	onRetry: () -> Unit,
	onRememberEligible: (String) -> Unit,
	onDeclineEligible: (String) -> Unit,
	onAcceptIncoming: (String) -> Unit,
	onDeclineIncoming: (String) -> Unit,
	onSend: (String) -> Unit,
	onOpenLabel: (String) -> Unit,
	onForget: (String) -> Unit,
	onBlock: (String) -> Unit,
) {
	Row(
		modifier = Modifier.fillMaxSize().padding(horizontal = 24.dp),
		horizontalArrangement = Arrangement.spacedBy(20.dp),
	) {
		LazyColumn(
			modifier = Modifier.weight(0.8f).fillMaxHeight(),
			verticalArrangement = Arrangement.spacedBy(12.dp),
			contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = 24.dp),
		) {
			if (state.isLoading) item(key = "loading") { LinearProgressIndicator(Modifier.fillMaxWidth()) }
			if (state.loadFailed) item(key = "load-failed") { InlineLoadFailure(onRetry) }
			pairingItems(
				state = state,
				onRememberEligible = onRememberEligible,
				onDeclineEligible = onDeclineEligible,
				onAcceptIncoming = onAcceptIncoming,
				onDeclineIncoming = onDeclineIncoming,
			)
			if (state.eligibilities.isEmpty() && state.pendingRelationships.isEmpty()) {
				item(key = "no-attention") {
					DesktopStatusCard()
				}
			}
		}
		LazyColumn(
			modifier = Modifier.weight(1.2f).fillMaxHeight(),
			verticalArrangement = Arrangement.spacedBy(12.dp),
			contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = 24.dp),
		) {
			item(key = "saved-title") { SectionTitle(stringResource(Res.string.saved_devices_list_title)) }
			if (state.savedDevices.isEmpty()) {
				item(key = "empty") { SavedDevicesEmptyCard() }
			} else {
				items(state.savedDevices, key = { "saved-${it.endpointId}" }) { device ->
					SavedDeviceCard(
						device = device,
						busy = device.endpointId in state.busyPeerIds,
						onSend = { onSend(device.endpointId) },
						onLabel = { onOpenLabel(device.endpointId) },
						onForget = { onForget(device.endpointId) },
						onBlock = { onBlock(device.endpointId) },
					)
				}
			}
		}
	}
}

private fun androidx.compose.foundation.lazy.LazyListScope.pairingItems(
	state: SavedDevicesState,
	onRememberEligible: (String) -> Unit,
	onDeclineEligible: (String) -> Unit,
	onAcceptIncoming: (String) -> Unit,
	onDeclineIncoming: (String) -> Unit,
) {
	if (state.eligibilities.isNotEmpty()) {
		item(key = "eligibility-title") {
			SectionTitle(stringResource(Res.string.saved_devices_eligibility_title))
		}
		items(state.eligibilities, key = { "eligibility-${it.peerEndpointId}" }) { eligibility ->
			EligibilityCard(
				eligibility = eligibility,
				busy = eligibility.peerEndpointId in state.busyPeerIds,
				onRemember = { onRememberEligible(eligibility.peerEndpointId) },
				onDecline = { onDeclineEligible(eligibility.peerEndpointId) },
			)
		}
	}
	if (state.pendingRelationships.isNotEmpty()) {
		item(key = "pending-title") {
			SectionTitle(stringResource(Res.string.saved_devices_pending_title))
		}
		items(state.pendingRelationships, key = { "pending-${it.remoteEndpointId}" }) { relationship ->
			PendingPairingCard(
				relationship = relationship,
				remoteDisplayName = state.eligibilities
					.firstOrNull { it.peerEndpointId == relationship.remoteEndpointId }
					?.remoteDisplayName,
				busy = relationship.remoteEndpointId in state.busyPeerIds,
				onAccept = { onAcceptIncoming(relationship.remoteEndpointId) },
				onDecline = { onDeclineIncoming(relationship.remoteEndpointId) },
			)
		}
	}
}

@Composable
private fun SectionTitle(title: String) {
	Text(
		text = title,
		style = MaterialTheme.typography.titleMedium,
		fontWeight = FontWeight.SemiBold,
		modifier = Modifier.padding(top = 4.dp).semantics { heading() },
	)
}

@Composable
private fun EligibilityCard(
	eligibility: PairingEligibilityModel,
	busy: Boolean,
	onRemember: () -> Unit,
	onDecline: () -> Unit,
) {
	PairingCard(
		name = eligibility.remoteDisplayName,
		endpointId = eligibility.peerEndpointId,
		status = stringResource(Res.string.saved_devices_eligibility_title),
		busy = busy,
		actions = {
			PrimaryButton(stringResource(Res.string.saved_devices_remember_action), onRemember, enabled = !busy)
			SecondaryButton(stringResource(Res.string.saved_devices_decline_action), onDecline, enabled = !busy)
		},
	)
}

@Composable
private fun PendingPairingCard(
	relationship: DeviceRelationshipModel,
	remoteDisplayName: String?,
	busy: Boolean,
	onAccept: () -> Unit,
	onDecline: () -> Unit,
) {
	PairingCard(
		name = remoteDisplayName,
		endpointId = relationship.remoteEndpointId,
		status = stringResource(
			when (relationship.state) {
				DeviceRelationshipStateModel.PendingIncoming -> Res.string.saved_devices_pending_incoming
				else -> Res.string.saved_devices_pending_outgoing
			},
		),
		busy = busy,
		actions = if (relationship.state == DeviceRelationshipStateModel.PendingIncoming) {
			{
				PrimaryButton(stringResource(Res.string.saved_devices_accept_pairing_action), onAccept, enabled = !busy)
				SecondaryButton(stringResource(Res.string.saved_devices_decline_action), onDecline, enabled = !busy)
			}
		} else {
			null
		},
	)
}

@Composable
private fun PairingCard(
	name: String?,
	endpointId: String,
	status: String,
	busy: Boolean,
	actions: (@Composable RowScope.() -> Unit)?,
) {
	val colors = LocalVniDropColors.current
	Card(
		modifier = Modifier.fillMaxWidth(),
		shape = RoundedCornerShape(16.dp),
		colors = CardDefaults.cardColors(containerColor = colors.backgroundSurface200),
		border = BorderStroke(1.dp, colors.borderDefault.copy(alpha = 0.72f)),
	) {
		Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
			Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
				PlatformIcon(AppIcon.Shield, contentDescription = null, tint = colors.brandLink, modifier = Modifier.size(24.dp))
				Column(Modifier.weight(1f)) {
					Text(
						name?.takeIf(String::isNotBlank) ?: stringResource(Res.string.saved_devices_unnamed),
						style = MaterialTheme.typography.titleMedium,
						fontWeight = FontWeight.SemiBold,
					)
					Text(status, style = MaterialTheme.typography.bodyMedium, color = colors.foregroundLight)
				}
				if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
			}
			DiagnosticEndpoint(endpointId)
			if (actions != null) {
				Row(horizontalArrangement = Arrangement.spacedBy(8.dp), content = actions)
			}
		}
	}
}

private enum class DeviceDestructiveAction { Forget, Block }

@Composable
private fun SavedDeviceCard(
	device: SavedDeviceModel,
	busy: Boolean,
	onSend: () -> Unit,
	onLabel: () -> Unit,
	onForget: () -> Unit,
	onBlock: () -> Unit,
) {
	val colors = LocalVniDropColors.current
	val title = device.displayName()
	var menuExpanded by remember(device.endpointId) { mutableStateOf(false) }
	var pendingAction by remember(device.endpointId) { mutableStateOf<DeviceDestructiveAction?>(null) }
	Card(
		modifier = Modifier.fillMaxWidth(),
		shape = RoundedCornerShape(18.dp),
		colors = CardDefaults.cardColors(containerColor = colors.backgroundSurface200),
		border = BorderStroke(1.dp, colors.borderDefault.copy(alpha = 0.72f)),
	) {
		Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				Box(contentAlignment = Alignment.Center) {
					Card(
						shape = RoundedCornerShape(14.dp),
						colors = CardDefaults.cardColors(containerColor = colors.backgroundSelection),
					) {
						PlatformIcon(
							AppIcon.ShieldCheck,
							contentDescription = null,
							tint = colors.brandLink,
							modifier = Modifier.padding(10.dp).size(24.dp),
						)
					}
				}
				Spacer(Modifier.width(12.dp))
				Column(Modifier.weight(1f)) {
					Text(
						text = title,
						style = MaterialTheme.typography.titleMedium,
						fontWeight = FontWeight.SemiBold,
						maxLines = 1,
						overflow = TextOverflow.Ellipsis,
					)
					device.remoteDisplayName
						?.takeIf { device.localLabel?.isNotBlank() == true && it.isNotBlank() }
						?.let { authenticatedName ->
							Text(
								stringResource(Res.string.saved_devices_authenticated_name, authenticatedName),
								style = MaterialTheme.typography.bodySmall,
								color = colors.foregroundLight,
							)
						}
				}
				if (busy) {
					CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
				} else {
					Box {
						val moreLabel = stringResource(Res.string.saved_devices_more_actions, title)
						IconButton(onClick = { menuExpanded = true }) {
							PlatformIcon(AppIcon.MoreVertical, contentDescription = moreLabel)
						}
						DropdownMenu(expanded = menuExpanded, onDismissRequest = { menuExpanded = false }) {
							DropdownMenuItem(
								text = { Text(stringResource(Res.string.saved_devices_label_action)) },
								onClick = { menuExpanded = false; onLabel() },
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
			DiagnosticEndpoint(device.endpointId)
			PrimaryButton(
				text = stringResource(Res.string.saved_devices_send_action),
				onClick = onSend,
				modifier = Modifier.fillMaxWidth(),
				enabled = !busy,
				leadingIcon = { PlatformIcon(AppIcon.Send, contentDescription = null, modifier = Modifier.size(18.dp)) },
			)
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
				TextButton(
					onClick = {
						pendingAction = null
						if (isBlock) onBlock() else onForget()
					},
				) {
					Text(stringResource(if (isBlock) Res.string.saved_devices_block_action else Res.string.saved_devices_forget_action))
				}
			},
			dismissButton = {
				TextButton(onClick = { pendingAction = null }) {
					Text(stringResource(Res.string.button_cancel))
				}
			},
		)
	}
}

@Composable
private fun DiagnosticEndpoint(endpointId: String) {
	Text(
		text = stringResource(Res.string.saved_devices_endpoint, shortEndpoint(endpointId)),
		style = MaterialTheme.typography.bodySmall,
		color = LocalVniDropColors.current.foregroundLighter,
		maxLines = 1,
		overflow = TextOverflow.Ellipsis,
	)
}

@Composable
private fun SavedDevicesEmptyCard() {
	val colors = LocalVniDropColors.current
	Card(
		modifier = Modifier.fillMaxWidth(),
		shape = RoundedCornerShape(18.dp),
		colors = CardDefaults.cardColors(containerColor = colors.backgroundSurface200),
	) {
		Column(
			modifier = Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 32.dp),
			horizontalAlignment = Alignment.CenterHorizontally,
			verticalArrangement = Arrangement.spacedBy(12.dp),
		) {
			PlatformIcon(AppIcon.ShieldCheck, contentDescription = null, tint = colors.foregroundLighter, modifier = Modifier.size(32.dp))
			Text(
				stringResource(Res.string.saved_devices_empty),
				style = MaterialTheme.typography.bodyLarge,
				color = colors.foregroundLight,
			)
		}
	}
}

@Composable
private fun DesktopStatusCard() {
	val colors = LocalVniDropColors.current
	Card(
		modifier = Modifier.fillMaxWidth(),
		colors = CardDefaults.cardColors(containerColor = colors.backgroundSurface200),
		shape = RoundedCornerShape(16.dp),
	) {
		Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
			PlatformIcon(AppIcon.Check, contentDescription = null, tint = colors.brandLink, modifier = Modifier.size(24.dp))
			Text(stringResource(Res.string.saved_devices_pending_title), style = MaterialTheme.typography.titleMedium)
			Text(stringResource(Res.string.saved_devices_no_pending), color = colors.foregroundLight)
		}
	}
}

@Composable
private fun SavedDevicesLoading(modifier: Modifier = Modifier) {
	Box(modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
		Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
			CircularProgressIndicator()
			Text(stringResource(Res.string.saved_devices_loading), color = LocalVniDropColors.current.foregroundLight)
		}
	}
}

@Composable
private fun SavedDevicesLoadFailure(onRetry: () -> Unit, modifier: Modifier = Modifier) {
	Box(modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
		Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
			PlatformIcon(AppIcon.CloudOff, contentDescription = null, modifier = Modifier.size(32.dp))
			Text(stringResource(Res.string.saved_devices_load_failed), color = LocalVniDropColors.current.foregroundLight)
			SecondaryButton(stringResource(Res.string.button_retry), onRetry)
		}
	}
}

@Composable
private fun InlineLoadFailure(onRetry: () -> Unit) {
	Card(
		modifier = Modifier.fillMaxWidth(),
		colors = CardDefaults.cardColors(containerColor = LocalVniDropColors.current.backgroundSurface200),
	) {
		Row(
			modifier = Modifier.fillMaxWidth().padding(14.dp),
			verticalAlignment = Alignment.CenterVertically,
			horizontalArrangement = Arrangement.spacedBy(12.dp),
		) {
			Text(stringResource(Res.string.saved_devices_load_failed), Modifier.weight(1f))
			TextButton(onClick = onRetry) { Text(stringResource(Res.string.button_retry)) }
		}
	}
}

@Composable
private fun SavedDeviceLabelDialog(
	visible: Boolean,
	label: String,
	onLabelChanged: (String) -> Unit,
	onSave: () -> Unit,
	onClear: () -> Unit,
	onDismiss: () -> Unit,
) {
	if (!visible) return
	AlertDialog(
		onDismissRequest = onDismiss,
		title = { Text(stringResource(Res.string.saved_devices_label_title)) },
		text = {
			OutlinedTextField(
				value = label,
				onValueChange = onLabelChanged,
				modifier = Modifier.fillMaxWidth(),
				singleLine = true,
				placeholder = { Text(stringResource(Res.string.saved_devices_label_placeholder)) },
			)
		},
		confirmButton = {
			TextButton(onClick = onSave) { Text(stringResource(Res.string.saved_devices_label_save)) }
		},
		dismissButton = {
			Row {
				TextButton(onClick = onClear) { Text(stringResource(Res.string.saved_devices_label_clear)) }
				TextButton(onClick = onDismiss) { Text(stringResource(Res.string.button_cancel)) }
			}
		},
	)
}

@Composable
private fun SavedDeviceModel.displayName(): String = localLabel?.takeIf(String::isNotBlank)
	?: remoteDisplayName?.takeIf(String::isNotBlank)
	?: stringResource(Res.string.saved_devices_unnamed)

private fun shortEndpoint(endpointId: String): String =
	if (endpointId.length <= 20) endpointId else endpointId.take(16) + "…"
