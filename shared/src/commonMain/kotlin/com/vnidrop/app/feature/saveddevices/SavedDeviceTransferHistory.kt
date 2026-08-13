package com.vnidrop.app.feature.saveddevices

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.TargetedTransferStateModel
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.state.formatBytes
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.saved_devices_transfer_cancel
import vnidrop.shared.generated.resources.saved_devices_transfer_delete
import vnidrop.shared.generated.resources.saved_devices_transfer_direction_incoming
import vnidrop.shared.generated.resources.saved_devices_transfer_direction_outgoing
import vnidrop.shared.generated.resources.saved_devices_transfer_files
import vnidrop.shared.generated.resources.saved_devices_transfer_progress
import vnidrop.shared.generated.resources.saved_devices_transfer_receive
import vnidrop.shared.generated.resources.saved_devices_transfer_resume
import vnidrop.shared.generated.resources.saved_devices_transfers_title
import vnidrop.shared.generated.resources.saved_devices_unnamed
import vnidrop.shared.generated.resources.status_approved
import vnidrop.shared.generated.resources.status_awaiting_approval
import vnidrop.shared.generated.resources.status_cancelled
import vnidrop.shared.generated.resources.status_completed
import vnidrop.shared.generated.resources.status_connecting
import vnidrop.shared.generated.resources.status_declined
import vnidrop.shared.generated.resources.status_failed
import vnidrop.shared.generated.resources.status_interrupted
import vnidrop.shared.generated.resources.status_offering
import vnidrop.shared.generated.resources.status_preparing
import vnidrop.shared.generated.resources.status_transferring

enum class SavedDeviceTransferAction {
	Receive,
	Resume,
	Cancel,
	Delete,
}

internal fun LazyListScope.targetedTransferItems(
	transfers: List<SavedDeviceTransferItem>,
	busyTransferIds: Set<String>,
	onAction: (String, SavedDeviceTransferAction) -> Unit,
	showHeading: Boolean = true,
) {
	if (showHeading) {
		item(key = "targeted-transfer-title") {
			Text(
				text = stringResource(Res.string.saved_devices_transfers_title),
				style = MaterialTheme.typography.titleMedium,
				fontWeight = FontWeight.SemiBold,
				modifier = Modifier.padding(top = 8.dp).semantics { heading() },
			)
		}
	}
	if (transfers.isNotEmpty()) {
		itemsIndexed(transfers, key = { _, transfer -> "targeted-${transfer.id}" }) { index, transfer ->
			if (index > 0) {
				HorizontalDivider(color = LocalVniDropColors.current.borderDefault.copy(alpha = 0.7f))
			}
			TargetedTransferRow(
				transfer = transfer,
				busy = transfer.id in busyTransferIds,
				onAction = { action -> onAction(transfer.id, action) },
			)
		}
	}
}

@Composable
private fun TargetedTransferRow(
	transfer: SavedDeviceTransferItem,
	busy: Boolean,
	onAction: (SavedDeviceTransferAction) -> Unit,
) {
	val colors = LocalVniDropColors.current
	val progress = if (transfer.totalSize == 0UL) 0f else {
		(transfer.verifiedBytes.toDouble() / transfer.totalSize.toDouble()).coerceIn(0.0, 1.0).toFloat()
	}
	Column(
		Modifier.fillMaxWidth().padding(vertical = 14.dp),
		verticalArrangement = Arrangement.spacedBy(8.dp),
	) {
		Row(verticalAlignment = Alignment.CenterVertically) {
			PlatformIcon(
				if (transfer.direction == SavedDeviceTransferDirection.Incoming) AppIcon.Download else AppIcon.Send,
				contentDescription = null,
				tint = colors.foregroundLight,
				modifier = Modifier.size(22.dp),
			)
			Spacer(Modifier.width(12.dp))
			Column(Modifier.weight(1f)) {
				Text(
					transfer.transferName.ifBlank { stringResource(Res.string.saved_devices_unnamed) },
					style = MaterialTheme.typography.titleSmall,
					fontWeight = FontWeight.SemiBold,
					maxLines = 1,
					overflow = TextOverflow.Ellipsis,
				)
				Text(
					stringResource(
						if (transfer.direction == SavedDeviceTransferDirection.Incoming) {
							Res.string.saved_devices_transfer_direction_incoming
						} else {
							Res.string.saved_devices_transfer_direction_outgoing
						},
						transfer.peerDisplayName?.takeIf(String::isNotBlank)
							?: stringResource(Res.string.saved_devices_unnamed),
					),
					style = MaterialTheme.typography.bodyMedium,
					color = colors.foregroundLight,
				)
			}
			if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
		}
		Row(
			modifier = Modifier.fillMaxWidth(),
			verticalAlignment = Alignment.CenterVertically,
			horizontalArrangement = Arrangement.spacedBy(12.dp),
		) {
			Text(
				text = "${stringResource(transfer.state.labelResource())} · " +
					stringResource(
						Res.string.saved_devices_transfer_files,
						transfer.fileCount.toString(),
						formatBytes(transfer.totalSize),
					),
				modifier = Modifier.weight(1f),
				style = MaterialTheme.typography.bodySmall,
				color = colors.foregroundLighter,
			)
		}
		if (
			transfer.direction == SavedDeviceTransferDirection.Incoming &&
			transfer.totalSize > 0UL &&
			transfer.state.showsProgress()
		) {
			LinearProgressIndicator(progress = { progress }, modifier = Modifier.fillMaxWidth())
			Text(
				stringResource(
					Res.string.saved_devices_transfer_progress,
					formatBytes(transfer.verifiedBytes),
					formatBytes(transfer.totalSize),
				),
				style = MaterialTheme.typography.labelSmall,
				color = colors.foregroundLighter,
			)
		}
		TransferActions(transfer, busy, onAction)
	}
}

@Composable
private fun TransferActions(
	transfer: SavedDeviceTransferItem,
	busy: Boolean,
	onAction: (SavedDeviceTransferAction) -> Unit,
) {
	val incoming = transfer.direction == SavedDeviceTransferDirection.Incoming
	Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
		when {
			incoming && transfer.state == TargetedTransferStateModel.Approved -> PrimaryButton(
				stringResource(Res.string.saved_devices_transfer_receive),
				{ onAction(SavedDeviceTransferAction.Receive) },
				enabled = !busy,
				leadingIcon = { PlatformIcon(AppIcon.Download, contentDescription = null, modifier = Modifier.size(18.dp)) },
			)
			incoming && transfer.state == TargetedTransferStateModel.Interrupted -> PrimaryButton(
				stringResource(Res.string.saved_devices_transfer_resume),
				{ onAction(SavedDeviceTransferAction.Resume) },
				enabled = !busy,
				leadingIcon = { PlatformIcon(AppIcon.Sync, contentDescription = null, modifier = Modifier.size(18.dp)) },
			)
		}
		when {
			transfer.state.isCancellable() -> SecondaryButton(
				stringResource(Res.string.saved_devices_transfer_cancel),
				{ onAction(SavedDeviceTransferAction.Cancel) },
				enabled = !busy,
				leadingIcon = { PlatformIcon(AppIcon.StopCircle, contentDescription = null, modifier = Modifier.size(18.dp)) },
			)
			transfer.state.isDeletable() -> SecondaryButton(
				stringResource(Res.string.saved_devices_transfer_delete),
				{ onAction(SavedDeviceTransferAction.Delete) },
				enabled = !busy,
				leadingIcon = { PlatformIcon(AppIcon.Delete, contentDescription = null, modifier = Modifier.size(18.dp)) },
			)
		}
	}
}

private fun TargetedTransferStateModel.labelResource(): StringResource = when (this) {
	TargetedTransferStateModel.Preparing -> Res.string.status_preparing
	TargetedTransferStateModel.Offering -> Res.string.status_offering
	TargetedTransferStateModel.AwaitingApproval -> Res.string.status_awaiting_approval
	TargetedTransferStateModel.Approved -> Res.string.status_approved
	TargetedTransferStateModel.Connecting -> Res.string.status_connecting
	TargetedTransferStateModel.Transferring -> Res.string.status_transferring
	TargetedTransferStateModel.Interrupted -> Res.string.status_interrupted
	TargetedTransferStateModel.Completed -> Res.string.status_completed
	TargetedTransferStateModel.Declined -> Res.string.status_declined
	TargetedTransferStateModel.Cancelled -> Res.string.status_cancelled
	TargetedTransferStateModel.Failed -> Res.string.status_failed
	TargetedTransferStateModel.Deleted -> Res.string.status_cancelled
}

private fun TargetedTransferStateModel.showsProgress(): Boolean = this in setOf(
	TargetedTransferStateModel.Connecting,
	TargetedTransferStateModel.Transferring,
	TargetedTransferStateModel.Interrupted,
	TargetedTransferStateModel.Completed,
)

private fun TargetedTransferStateModel.isCancellable(): Boolean = this in setOf(
	TargetedTransferStateModel.Preparing,
	TargetedTransferStateModel.Offering,
	TargetedTransferStateModel.AwaitingApproval,
	TargetedTransferStateModel.Approved,
	TargetedTransferStateModel.Connecting,
	TargetedTransferStateModel.Transferring,
	TargetedTransferStateModel.Interrupted,
)

private fun TargetedTransferStateModel.isDeletable(): Boolean = this in setOf(
	TargetedTransferStateModel.Completed,
	TargetedTransferStateModel.Declined,
	TargetedTransferStateModel.Cancelled,
	TargetedTransferStateModel.Failed,
)
