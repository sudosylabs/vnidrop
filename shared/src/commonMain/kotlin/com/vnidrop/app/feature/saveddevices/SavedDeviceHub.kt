package com.vnidrop.app.feature.saveddevices

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_cancel
import vnidrop.shared.generated.resources.button_retry
import vnidrop.shared.generated.resources.offer_accept
import vnidrop.shared.generated.resources.offer_body
import vnidrop.shared.generated.resources.offer_decline
import vnidrop.shared.generated.resources.offer_title
import vnidrop.shared.generated.resources.saved_devices_accept_pairing_action
import vnidrop.shared.generated.resources.saved_devices_attention_title
import vnidrop.shared.generated.resources.saved_devices_authenticated_name
import vnidrop.shared.generated.resources.saved_devices_block_action
import vnidrop.shared.generated.resources.saved_devices_block_confirm_body
import vnidrop.shared.generated.resources.saved_devices_block_confirm_title
import vnidrop.shared.generated.resources.saved_devices_decline_action
import vnidrop.shared.generated.resources.saved_devices_devices_title
import vnidrop.shared.generated.resources.saved_devices_eligibility_title
import vnidrop.shared.generated.resources.saved_devices_empty
import vnidrop.shared.generated.resources.saved_devices_empty_title
import vnidrop.shared.generated.resources.saved_devices_endpoint
import vnidrop.shared.generated.resources.saved_devices_forget_action
import vnidrop.shared.generated.resources.saved_devices_forget_confirm_body
import vnidrop.shared.generated.resources.saved_devices_forget_confirm_title
import vnidrop.shared.generated.resources.saved_devices_label_action
import vnidrop.shared.generated.resources.saved_devices_load_failed
import vnidrop.shared.generated.resources.saved_devices_more_actions
import vnidrop.shared.generated.resources.saved_devices_pending_incoming
import vnidrop.shared.generated.resources.saved_devices_pending_outgoing
import vnidrop.shared.generated.resources.saved_devices_remember_action
import vnidrop.shared.generated.resources.saved_devices_send_action
import vnidrop.shared.generated.resources.saved_devices_transfer_empty
import vnidrop.shared.generated.resources.saved_devices_transfers_title
import vnidrop.shared.generated.resources.saved_devices_unnamed

@Composable
internal fun CompactSavedDevicesHub(
	state: SavedDevicesState,
	onRetry: () -> Unit,
	onRememberEligible: (String) -> Unit,
	onDeclineEligible: (String) -> Unit,
	onAcceptIncoming: (String) -> Unit,
	onDeclineIncoming: (String) -> Unit,
	onAcceptOffer: (String) -> Unit,
	onDeclineOffer: (String) -> Unit,
	onSend: (String) -> Unit,
	onOpenLabel: (String) -> Unit,
	onForget: (String) -> Unit,
	onBlock: (String) -> Unit,
	onTransferAction: (String, SavedDeviceTransferAction) -> Unit,
) {
	val hasAttention = state.attentionCount > 0
	val isEmpty = !hasAttention && state.savedDevices.isEmpty() && state.targetedTransfers.isEmpty()
	LazyColumn(
		modifier = Modifier.fillMaxSize(),
		contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 28.dp),
		verticalArrangement = Arrangement.spacedBy(20.dp),
	) {
		if (state.isLoading) item(key = "loading") { androidx.compose.material3.LinearProgressIndicator(Modifier.fillMaxWidth()) }
		if (state.loadFailed) item(key = "load-failed") { InlineLoadFailure(onRetry) }
		if (hasAttention) {
			item(key = "attention") {
				AttentionSection(
					state,
					onRememberEligible,
					onDeclineEligible,
					onAcceptIncoming,
					onDeclineIncoming,
					onAcceptOffer,
					onDeclineOffer,
				)
			}
		}
		if (state.savedDevices.isNotEmpty()) {
			item(key = "saved-devices") {
				SavedDeviceSection(state, onSend, onOpenLabel, onForget, onBlock)
			}
		}
		if (state.targetedTransfers.isNotEmpty()) {
			targetedTransferItems(state.targetedTransfers, state.busyTransferIds, onTransferAction)
		}
		if (isEmpty) item(key = "empty") { SavedDevicesEmptyState(Modifier.fillParentMaxHeight(0.72f)) }
	}
}

@Composable
internal fun DesktopSavedDevicesHub(
	state: SavedDevicesState,
	onRetry: () -> Unit,
	onRememberEligible: (String) -> Unit,
	onDeclineEligible: (String) -> Unit,
	onAcceptIncoming: (String) -> Unit,
	onDeclineIncoming: (String) -> Unit,
	onAcceptOffer: (String) -> Unit,
	onDeclineOffer: (String) -> Unit,
	onSend: (String) -> Unit,
	onOpenLabel: (String) -> Unit,
	onForget: (String) -> Unit,
	onBlock: (String) -> Unit,
	onTransferAction: (String, SavedDeviceTransferAction) -> Unit,
) {
	val hasAttention = state.attentionCount > 0
	val isEmpty = !hasAttention && state.savedDevices.isEmpty() && state.targetedTransfers.isEmpty()
	if (isEmpty && !state.isLoading && !state.loadFailed) {
		Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
			SavedDevicesEmptyState(Modifier.widthIn(max = 440.dp))
		}
		return
	}
	Row(
		modifier = Modifier.fillMaxSize().padding(horizontal = 24.dp),
		horizontalArrangement = Arrangement.spacedBy(32.dp),
	) {
		LazyColumn(
			modifier = Modifier.weight(0.9f).fillMaxHeight(),
			verticalArrangement = Arrangement.spacedBy(20.dp),
			contentPadding = PaddingValues(bottom = 24.dp),
		) {
			if (state.isLoading) item(key = "loading") { androidx.compose.material3.LinearProgressIndicator(Modifier.fillMaxWidth()) }
			if (state.loadFailed) item(key = "load-failed") { InlineLoadFailure(onRetry) }
			if (hasAttention) {
				item(key = "attention") {
					AttentionSection(
						state,
						onRememberEligible,
						onDeclineEligible,
						onAcceptIncoming,
						onDeclineIncoming,
						onAcceptOffer,
						onDeclineOffer,
					)
				}
			}
			if (state.savedDevices.isNotEmpty()) {
				item(key = "saved-devices") {
					SavedDeviceSection(state, onSend, onOpenLabel, onForget, onBlock)
				}
			}
		}
		LazyColumn(
			modifier = Modifier.weight(1.1f).fillMaxHeight(),
			verticalArrangement = Arrangement.spacedBy(12.dp),
			contentPadding = PaddingValues(bottom = 24.dp),
		) {
			if (state.targetedTransfers.isNotEmpty()) {
				targetedTransferItems(
					state.targetedTransfers,
					state.busyTransferIds,
					onTransferAction,
					presentation = TargetedTransferPresentation.DesktopRow,
				)
			} else {
				item(key = "targeted-transfer-title") { SectionTitle(stringResource(Res.string.saved_devices_transfers_title)) }
				item(key = "targeted-transfer-empty") {
					Text(
						stringResource(Res.string.saved_devices_transfer_empty),
						style = MaterialTheme.typography.bodyMedium,
						color = LocalVniDropColors.current.foregroundLight,
					)
				}
			}
		}
	}
}

private val SavedDevicesState.attentionCount: Int
	get() = targetedOffers.pending.size + eligibilities.size + pendingRelationships.size

@Composable
private fun AttentionSection(
	state: SavedDevicesState,
	onRememberEligible: (String) -> Unit,
	onDeclineEligible: (String) -> Unit,
	onAcceptIncoming: (String) -> Unit,
	onDeclineIncoming: (String) -> Unit,
	onAcceptOffer: (String) -> Unit,
	onDeclineOffer: (String) -> Unit,
) {
	Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
		SectionTitle(stringResource(Res.string.saved_devices_attention_title))
		Surface(
			shape = RoundedCornerShape(12.dp),
			color = LocalVniDropColors.current.backgroundSurface200,
		) {
			Column {
				var hasPrevious = false
				state.targetedOffers.pending.forEach { offer ->
					if (hasPrevious) GroupDivider()
					TargetedOfferRow(
						offer,
						state.targetedOffers.senderDisplayNames[offer.senderEndpointId],
						offer.transferId in state.targetedOffers.respondingIds,
						{ onAcceptOffer(offer.transferId) },
						{ onDeclineOffer(offer.transferId) },
					)
					hasPrevious = true
				}
				state.eligibilities.forEach { eligibility ->
					if (hasPrevious) GroupDivider()
					EligibilityRow(
						eligibility,
						eligibility.peerEndpointId in state.busyPeerIds,
						{ onRememberEligible(eligibility.peerEndpointId) },
						{ onDeclineEligible(eligibility.peerEndpointId) },
					)
					hasPrevious = true
				}
				state.pendingRelationships.forEach { relationship ->
					if (hasPrevious) GroupDivider()
					PendingPairingRow(
						relationship,
						state.eligibilities.firstOrNull { it.peerEndpointId == relationship.remoteEndpointId }?.remoteDisplayName,
						relationship.remoteEndpointId in state.busyPeerIds,
						{ onAcceptIncoming(relationship.remoteEndpointId) },
						{ onDeclineIncoming(relationship.remoteEndpointId) },
					)
					hasPrevious = true
				}
			}
		}
	}
}

@Composable
private fun TargetedOfferRow(
	offer: PendingTargetedOfferModel,
	senderName: String?,
	busy: Boolean,
	onAccept: () -> Unit,
	onDecline: () -> Unit,
) {
	val deviceName = senderName?.takeIf(String::isNotBlank) ?: stringResource(Res.string.saved_devices_unnamed)
	DecisionRow(
		icon = AppIcon.Download,
		title = stringResource(Res.string.offer_title),
		body = stringResource(Res.string.offer_body, deviceName, offer.transferName),
		endpointId = offer.senderEndpointId,
		busy = busy,
	) {
		PrimaryButton(stringResource(Res.string.offer_accept), onAccept, enabled = !busy)
		SecondaryButton(stringResource(Res.string.offer_decline), onDecline, enabled = !busy)
	}
}

@Composable
private fun EligibilityRow(
	eligibility: PairingEligibilityModel,
	busy: Boolean,
	onRemember: () -> Unit,
	onDecline: () -> Unit,
) {
	DecisionRow(
		icon = AppIcon.Shield,
		title = eligibility.remoteDisplayName?.takeIf(String::isNotBlank) ?: stringResource(Res.string.saved_devices_unnamed),
		body = stringResource(Res.string.saved_devices_eligibility_title),
		endpointId = eligibility.peerEndpointId,
		busy = busy,
	) {
		PrimaryButton(stringResource(Res.string.saved_devices_remember_action), onRemember, enabled = !busy)
		SecondaryButton(stringResource(Res.string.saved_devices_decline_action), onDecline, enabled = !busy)
	}
}

@Composable
private fun PendingPairingRow(
	relationship: DeviceRelationshipModel,
	remoteDisplayName: String?,
	busy: Boolean,
	onAccept: () -> Unit,
	onDecline: () -> Unit,
) {
	DecisionRow(
		icon = AppIcon.Shield,
		title = remoteDisplayName?.takeIf(String::isNotBlank) ?: stringResource(Res.string.saved_devices_unnamed),
		body = stringResource(
			if (relationship.state == DeviceRelationshipStateModel.PendingIncoming) {
				Res.string.saved_devices_pending_incoming
			} else {
				Res.string.saved_devices_pending_outgoing
			},
		),
		endpointId = relationship.remoteEndpointId,
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
private fun DecisionRow(
	icon: AppIcon,
	title: String,
	body: String,
	endpointId: String,
	busy: Boolean,
	actions: (@Composable RowScope.() -> Unit)?,
) {
	val colors = LocalVniDropColors.current
	Column(Modifier.padding(horizontal = 16.dp, vertical = 14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
		Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
			PlatformIcon(icon, contentDescription = null, tint = colors.brandLink, modifier = Modifier.padding(top = 2.dp).size(22.dp))
			Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
				Text(title, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold)
				Text(body, style = MaterialTheme.typography.bodyMedium, color = colors.foregroundLight)
				DiagnosticEndpoint(endpointId)
			}
			if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
		}
		if (actions != null) Row(horizontalArrangement = Arrangement.spacedBy(8.dp), content = actions)
	}
}

@Composable
private fun SavedDeviceSection(
	state: SavedDevicesState,
	onSend: (String) -> Unit,
	onOpenLabel: (String) -> Unit,
	onForget: (String) -> Unit,
	onBlock: (String) -> Unit,
) {
	Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
		SectionTitle(stringResource(Res.string.saved_devices_devices_title))
		Surface(shape = RoundedCornerShape(12.dp), color = LocalVniDropColors.current.backgroundSurface200) {
			Column {
				state.savedDevices.forEachIndexed { index, device ->
					if (index > 0) GroupDivider()
					SavedDeviceRow(
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

private enum class DeviceDestructiveAction { Forget, Block }

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
	val title = device.displayName()
	var menuExpanded by remember(device.endpointId) { mutableStateOf(false) }
	var pendingAction by remember(device.endpointId) { mutableStateOf<DeviceDestructiveAction?>(null) }
	Row(
		modifier = Modifier.fillMaxWidth().testTag("saved-device-${device.endpointId}").padding(start = 16.dp, end = 6.dp, top = 12.dp, bottom = 12.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		PlatformIcon(AppIcon.ShieldCheck, contentDescription = null, tint = colors.foregroundLight, modifier = Modifier.size(24.dp))
		Spacer(Modifier.width(14.dp))
		Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
			Text(title, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
			device.remoteDisplayName
				?.takeIf { device.localLabel?.isNotBlank() == true && it.isNotBlank() }
				?.let { Text(stringResource(Res.string.saved_devices_authenticated_name, it), style = MaterialTheme.typography.bodySmall, color = colors.foregroundLight) }
			DiagnosticEndpoint(device.endpointId)
		}
		if (busy) {
			CircularProgressIndicator(Modifier.padding(horizontal = 14.dp).size(20.dp), strokeWidth = 2.dp)
		} else {
			val sendLabel = stringResource(Res.string.saved_devices_send_action)
			IconButton(onClick = onSend) {
				PlatformIcon(AppIcon.Send, contentDescription = sendLabel)
			}
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
	pendingAction?.let { action ->
		val isBlock = action == DeviceDestructiveAction.Block
		AlertDialog(
			onDismissRequest = { pendingAction = null },
			title = { Text(stringResource(if (isBlock) Res.string.saved_devices_block_confirm_title else Res.string.saved_devices_forget_confirm_title)) },
			text = {
				Text(stringResource(if (isBlock) Res.string.saved_devices_block_confirm_body else Res.string.saved_devices_forget_confirm_body, title))
			},
			confirmButton = {
				TextButton(onClick = { pendingAction = null; if (isBlock) onBlock() else onForget() }) {
					Text(stringResource(if (isBlock) Res.string.saved_devices_block_action else Res.string.saved_devices_forget_action))
				}
			},
			dismissButton = { TextButton(onClick = { pendingAction = null }) { Text(stringResource(Res.string.button_cancel)) } },
		)
	}
}

@Composable
private fun SectionTitle(title: String) {
	Text(
		title,
		style = MaterialTheme.typography.titleMedium,
		fontWeight = FontWeight.SemiBold,
		modifier = Modifier.semantics { heading() },
	)
}

@Composable
private fun GroupDivider() {
	HorizontalDivider(
		modifier = Modifier.padding(start = 54.dp),
		color = LocalVniDropColors.current.borderDefault.copy(alpha = 0.7f),
	)
}

@Composable
private fun DiagnosticEndpoint(endpointId: String) {
	Text(
		stringResource(Res.string.saved_devices_endpoint, shortEndpoint(endpointId)),
		style = MaterialTheme.typography.bodySmall,
		color = LocalVniDropColors.current.foregroundLighter,
		maxLines = 1,
		overflow = TextOverflow.Ellipsis,
	)
}

@Composable
private fun SavedDevicesEmptyState(modifier: Modifier = Modifier) {
	val colors = LocalVniDropColors.current
	Box(modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
		Column(
			modifier = Modifier.padding(horizontal = 24.dp),
			horizontalAlignment = Alignment.CenterHorizontally,
			verticalArrangement = Arrangement.spacedBy(10.dp),
		) {
			PlatformIcon(AppIcon.ShieldCheck, contentDescription = null, tint = colors.foregroundLighter, modifier = Modifier.size(36.dp))
			Text(stringResource(Res.string.saved_devices_empty_title), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
			Text(
				stringResource(Res.string.saved_devices_empty),
				style = MaterialTheme.typography.bodyMedium,
				color = colors.foregroundLight,
				textAlign = TextAlign.Center,
			)
		}
	}
}

@Composable
private fun InlineLoadFailure(onRetry: () -> Unit) {
	Row(
		modifier = Modifier.fillMaxWidth(),
		verticalAlignment = Alignment.CenterVertically,
		horizontalArrangement = Arrangement.spacedBy(12.dp),
	) {
		Text(stringResource(Res.string.saved_devices_load_failed), Modifier.weight(1f), color = LocalVniDropColors.current.foregroundLight)
		TextButton(onClick = onRetry) { Text(stringResource(Res.string.button_retry)) }
	}
}

@Composable
private fun SavedDeviceModel.displayName(): String = localLabel?.takeIf(String::isNotBlank)
	?: remoteDisplayName?.takeIf(String::isNotBlank)
	?: stringResource(Res.string.saved_devices_unnamed)

private fun shortEndpoint(endpointId: String): String =
	if (endpointId.length <= 20) endpointId else endpointId.take(16) + "…"
