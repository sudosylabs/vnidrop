package com.vnidrop.app.feature.saveddevices

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.components.FeatureEmptyState
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_retry
import vnidrop.shared.generated.resources.offer_accept
import vnidrop.shared.generated.resources.offer_body
import vnidrop.shared.generated.resources.offer_decline
import vnidrop.shared.generated.resources.offer_title
import vnidrop.shared.generated.resources.saved_devices_accept_pairing_action
import vnidrop.shared.generated.resources.saved_devices_attention_title
import vnidrop.shared.generated.resources.saved_devices_decline_action
import vnidrop.shared.generated.resources.saved_devices_eligibility_title
import vnidrop.shared.generated.resources.saved_devices_empty
import vnidrop.shared.generated.resources.saved_devices_empty_title
import vnidrop.shared.generated.resources.saved_devices_load_failed
import vnidrop.shared.generated.resources.saved_devices_pending_incoming
import vnidrop.shared.generated.resources.saved_devices_pending_outgoing
import vnidrop.shared.generated.resources.saved_devices_remember_action
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
	onOpenDevice: (String) -> Unit,
) {
	SavedDevicesHub(
		state = state,
		contentPadding = PaddingValues(start = 16.dp, end = 16.dp, bottom = 28.dp),
		onRetry = onRetry,
		onRememberEligible = onRememberEligible,
		onDeclineEligible = onDeclineEligible,
		onAcceptIncoming = onAcceptIncoming,
		onDeclineIncoming = onDeclineIncoming,
		onAcceptOffer = onAcceptOffer,
		onDeclineOffer = onDeclineOffer,
		onOpenDevice = onOpenDevice,
	)
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
	onOpenDevice: (String) -> Unit,
) {
	Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
		SavedDevicesHub(
			state = state,
			modifier = Modifier.widthIn(max = 720.dp),
			contentPadding = PaddingValues(start = 24.dp, end = 24.dp, bottom = 32.dp),
			onRetry = onRetry,
			onRememberEligible = onRememberEligible,
			onDeclineEligible = onDeclineEligible,
			onAcceptIncoming = onAcceptIncoming,
			onDeclineIncoming = onDeclineIncoming,
			onAcceptOffer = onAcceptOffer,
			onDeclineOffer = onDeclineOffer,
			onOpenDevice = onOpenDevice,
		)
	}
}

@Composable
private fun SavedDevicesHub(
	state: SavedDevicesState,
	contentPadding: PaddingValues,
	onRetry: () -> Unit,
	onRememberEligible: (String) -> Unit,
	onDeclineEligible: (String) -> Unit,
	onAcceptIncoming: (String) -> Unit,
	onDeclineIncoming: (String) -> Unit,
	onAcceptOffer: (String) -> Unit,
	onDeclineOffer: (String) -> Unit,
	onOpenDevice: (String) -> Unit,
	modifier: Modifier = Modifier,
) {
	val hasAttention = state.attentionCount > 0
	val showEmptyState = !hasAttention && state.savedDevices.isEmpty()
	LazyColumn(
		modifier = modifier.fillMaxSize(),
		contentPadding = contentPadding,
		verticalArrangement = Arrangement.spacedBy(20.dp),
	) {
		if (state.isLoading) {
			item(key = "loading") { androidx.compose.material3.LinearProgressIndicator(Modifier.fillMaxWidth()) }
		}
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
			item(key = "saved-devices") { SavedDeviceSection(state, onOpenDevice) }
		}
		if (showEmptyState) {
			item(key = "empty") { SavedDevicesEmptyState(Modifier.fillParentMaxHeight(0.72f)) }
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
		Surface(shape = RoundedCornerShape(12.dp), color = LocalVniDropColors.current.backgroundSurface200) {
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
		icon = AppIcon.Device,
		title = eligibility.remoteDisplayName?.takeIf(String::isNotBlank) ?: stringResource(Res.string.saved_devices_unnamed),
		body = stringResource(Res.string.saved_devices_eligibility_title),
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
		icon = AppIcon.Device,
		title = remoteDisplayName?.takeIf(String::isNotBlank) ?: stringResource(Res.string.saved_devices_unnamed),
		body = stringResource(
			if (relationship.state == DeviceRelationshipStateModel.PendingIncoming) {
				Res.string.saved_devices_pending_incoming
			} else {
				Res.string.saved_devices_pending_outgoing
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
private fun DecisionRow(
	icon: AppIcon,
	title: String,
	body: String,
	busy: Boolean,
	actions: (@Composable RowScope.() -> Unit)?,
) {
	val colors = LocalVniDropColors.current
	Column(Modifier.padding(horizontal = 16.dp, vertical = 14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
		Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
			PlatformIcon(icon, contentDescription = null, tint = colors.foregroundLight, modifier = Modifier.padding(top = 2.dp).size(22.dp))
			Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
				Text(title, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold)
				Text(body, style = MaterialTheme.typography.bodyMedium, color = colors.foregroundLight)
			}
			if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
		}
		if (actions != null) Row(horizontalArrangement = Arrangement.spacedBy(8.dp), content = actions)
	}
}

@Composable
private fun SavedDeviceSection(state: SavedDevicesState, onOpenDevice: (String) -> Unit) {
	Column {
		state.savedDevices.forEachIndexed { index, device ->
			if (index > 0) GroupDivider()
			SavedDeviceRow(
				device = device,
				busy = device.endpointId in state.busyPeerIds,
				onOpen = { onOpenDevice(device.endpointId) },
			)
		}
	}
}

@Composable
private fun SavedDeviceRow(device: SavedDeviceModel, busy: Boolean, onOpen: () -> Unit) {
	val colors = LocalVniDropColors.current
	Row(
		modifier = Modifier
			.fillMaxWidth()
			.testTag("saved-device-${device.endpointId}")
			.clickable(onClick = onOpen)
			.padding(horizontal = 4.dp, vertical = 14.dp),
		verticalAlignment = Alignment.CenterVertically,
	) {
		PlatformIcon(AppIcon.Device, contentDescription = null, tint = colors.foregroundLight, modifier = Modifier.size(24.dp))
		Spacer(Modifier.width(14.dp))
		Column(Modifier.weight(1f)) {
			Text(
				device.displayName(),
				style = MaterialTheme.typography.titleSmall,
				fontWeight = FontWeight.SemiBold,
				maxLines = 1,
				overflow = TextOverflow.Ellipsis,
			)
		}
		if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
		else PlatformIcon(AppIcon.ChevronRight, contentDescription = null, tint = colors.foregroundLighter)
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
private fun SavedDevicesEmptyState(modifier: Modifier = Modifier) {
	Box(modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
		FeatureEmptyState(
			icon = AppIcon.Device,
			title = stringResource(Res.string.saved_devices_empty_title),
			description = stringResource(Res.string.saved_devices_empty),
			iconTestTag = "saved-devices-empty-icon",
		)
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
internal fun SavedDeviceModel.displayName(): String = localLabel?.takeIf(String::isNotBlank)
	?: remoteDisplayName?.takeIf(String::isNotBlank)
	?: stringResource(Res.string.saved_devices_unnamed)

internal fun shortDeviceEndpoint(endpointId: String): String =
	if (endpointId.length <= 20) endpointId else endpointId.take(16) + "…"
