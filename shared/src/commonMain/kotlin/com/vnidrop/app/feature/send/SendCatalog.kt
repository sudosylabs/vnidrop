package com.vnidrop.app.feature.send

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.CoreEventModel
import com.vnidrop.app.core.ReceiverDeliveryStatus
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.ui.components.PillTone
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.components.ProgressRow
import com.vnidrop.app.ui.components.StatusPill
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.platform.usesMobilePresentation
import com.vnidrop.app.ui.state.TransferProgress
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.state.activeSendProgress
import com.vnidrop.app.ui.state.displayNameForStatus
import com.vnidrop.app.ui.state.formatBytes
import com.vnidrop.app.ui.state.progressForTransfer
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import org.jetbrains.compose.resources.decodeToImageBitmap
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_create_new_transfer
import vnidrop.shared.generated.resources.button_delete_transfer
import vnidrop.shared.generated.resources.button_more_actions
import vnidrop.shared.generated.resources.send_empty_body
import vnidrop.shared.generated.resources.send_empty_title
import vnidrop.shared.generated.resources.send_new_transfer_description
import vnidrop.shared.generated.resources.send_new_transfer_title
import vnidrop.shared.generated.resources.send_title
import vnidrop.shared.generated.resources.send_stop_sharing
import vnidrop.shared.generated.resources.send_transfers_title
import vnidrop.shared.generated.resources.transfer_share_title

@Composable
internal fun SendFloatingAction(onClick: () -> Unit, modifier: Modifier = Modifier) {
	FloatingActionButton(
		onClick = onClick,
		modifier = modifier,
		containerColor = LocalVniDropColors.current.brandButton,
		contentColor = Color.White,
	) {
		PlatformIcon(AppIcon.Add, contentDescription = stringResource(Res.string.send_new_transfer_description))
	}
}

@Composable
internal fun TransferCatalog(
	transfers: List<Transfer>,
	transferThumbnails: Map<ULong, ByteArray>,
	receiversByTransfer: Map<ULong, List<ReceiverRequestModel>> = emptyMap(),
	events: List<CoreEventModel> = emptyList(),
	windowClass: WindowClass,
	onOpenComposer: () -> Unit,
	onTransferSelected: (ULong) -> Unit,
	onShare: (ULong) -> Unit = {},
	onStopSharing: (ULong) -> Unit = {},
	onDelete: (ULong) -> Unit = {},
) {
	val usesFloatingAction = usesMobilePresentation(LocalUiPlatform.current, windowClass)
	LazyColumn(
		modifier = Modifier.fillMaxSize().statusBarsPadding(),
		contentPadding = PaddingValues(
			start = 16.dp,
			top = 16.dp,
			end = 16.dp,
			bottom = if (usesFloatingAction && transfers.isNotEmpty()) 96.dp else 24.dp,
		),
		verticalArrangement = Arrangement.spacedBy(12.dp),
	) {
		item { CatalogHeader(showAction = !usesFloatingAction && transfers.isNotEmpty(), onOpenComposer) }
		if (transfers.isEmpty()) {
			item { SendEmptyState(onOpenComposer) }
		} else {
			item {
				Text(
					stringResource(Res.string.send_transfers_title),
					style = MaterialTheme.typography.titleMedium,
					fontWeight = FontWeight.SemiBold,
				)
			}
			items(transfers, key = Transfer::localId) { transfer ->
				val activeReceiverEndpointIds = receiversByTransfer[transfer.transferId]
					.orEmpty()
					.filter { it.status == ReceiverDeliveryStatus.Accepted }
					.mapTo(mutableSetOf()) { it.remoteEndpointId }
				val progress = when (transfer.status) {
					TransferStatus.Importing -> progressForTransfer(events, transfer.transferId)
					TransferStatus.Sharing -> activeSendProgress(
						events,
						transfer.transferId,
						activeReceiverEndpointIds,
						transfer.totalSize,
					)
					else -> null
				}
				TransferListItem(
					transfer = transfer,
					thumbnailBytes = transferThumbnails[transfer.transferId],
					progress = progress,
					onClick = { onTransferSelected(transfer.transferId) },
					onShare = { onShare(transfer.transferId) },
					onStopSharing = { onStopSharing(transfer.transferId) },
					onDelete = { onDelete(transfer.transferId) },
				)
			}
		}
	}
}

@Composable
private fun CatalogHeader(showAction: Boolean, onOpenComposer: () -> Unit) {
	Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
		Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
			Text(stringResource(Res.string.send_title), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
		}
		if (showAction) {
			Spacer(Modifier.width(16.dp))
			PrimaryButton(stringResource(Res.string.button_create_new_transfer), onClick = onOpenComposer)
		}
	}
}

@Composable
private fun SendEmptyState(onOpenComposer: () -> Unit) {
	val colors = LocalVniDropColors.current
	Column(
		modifier = Modifier.fillMaxWidth().heightIn(min = 430.dp).padding(horizontal = 20.dp),
		horizontalAlignment = Alignment.CenterHorizontally,
		verticalArrangement = Arrangement.Center,
	) {
		PlatformIcon(
			icon = AppIcon.Send,
			contentDescription = null,
			tint = colors.brandLink,
			modifier = Modifier
				.size(88.dp)
				.testTag("send-empty-icon"),
		)
		Text(
			stringResource(Res.string.send_empty_title),
			modifier = Modifier.padding(top = 12.dp),
			style = MaterialTheme.typography.headlineSmall,
			fontWeight = FontWeight.Bold,
			textAlign = TextAlign.Center,
		)
		Text(
			stringResource(Res.string.send_empty_body),
			modifier = Modifier.padding(top = 8.dp).widthIn(max = 480.dp),
			color = colors.foregroundLighter,
			style = MaterialTheme.typography.bodyMedium,
			textAlign = TextAlign.Center,
		)
		PrimaryButton(
			stringResource(Res.string.button_create_new_transfer),
			onClick = onOpenComposer,
			modifier = Modifier.padding(top = 22.dp),
		)
	}
}

@Composable
private fun TransferListItem(
	transfer: Transfer,
	thumbnailBytes: ByteArray?,
	progress: TransferProgress?,
	onClick: () -> Unit,
	onShare: () -> Unit,
	onStopSharing: () -> Unit,
	onDelete: () -> Unit,
) {
	val colors = LocalVniDropColors.current
	Surface(onClick = onClick, modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp), color = colors.backgroundSurface200) {
		Row(modifier = Modifier.fillMaxWidth().padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
			Box(
				modifier = Modifier.size(44.dp).background(colors.backgroundSurface300, RoundedCornerShape(12.dp)),
				contentAlignment = Alignment.Center,
			) {
				FileArtwork(thumbnailBytes, Modifier.fillMaxSize())
			}
			Spacer(Modifier.width(12.dp))
			Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
				Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
					Text(
						transfer.transferName ?: stringResource(Res.string.send_new_transfer_title),
						modifier = Modifier.weight(1f, fill = false),
						style = MaterialTheme.typography.bodyLarge,
						fontWeight = FontWeight.SemiBold,
						maxLines = 1,
						overflow = TextOverflow.Ellipsis,
					)
					Spacer(Modifier.width(8.dp))
					StatusPill(displayNameForStatus(transfer.status), tone = transfer.status.pillTone())
				}
				Text(
					"${formatBytes(transfer.totalSize)} · ${accessPolicyLabel(transfer.accessPolicy)}",
					color = colors.foregroundLighter,
					style = MaterialTheme.typography.bodySmall,
					maxLines = 1,
					overflow = TextOverflow.Ellipsis,
				)
				if (progress != null && transfer.status in setOf(TransferStatus.Importing, TransferStatus.Sharing)) {
					ProgressRow(label = progress.label, progress = progress.progress, detail = progress.detail)
				}
			}
			TransferActionsMenu(transfer, onShare, onStopSharing, onDelete)
		}
	}
}

@Composable
private fun TransferActionsMenu(
	transfer: Transfer,
	onShare: () -> Unit,
	onStopSharing: () -> Unit,
	onDelete: () -> Unit,
) {
	var expanded by remember { mutableStateOf(false) }
	val moreActionsLabel = stringResource(Res.string.button_more_actions)
	Box {
		IconButton(
			onClick = { expanded = true },
			modifier = Modifier.semantics {
				contentDescription = moreActionsLabel
			},
		) {
			PlatformIcon(
				AppIcon.MoreVertical,
				contentDescription = null,
				tint = LocalVniDropColors.current.foregroundLighter,
			)
		}
		DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
			if (transfer.ticket != null) {
				DropdownMenuItem(
					text = { Text(stringResource(Res.string.transfer_share_title)) },
					onClick = {
						expanded = false
						onShare()
					},
					leadingIcon = { PlatformIcon(AppIcon.Share, contentDescription = null) },
				)
			}
			if (transfer.status == TransferStatus.Sharing) {
				DropdownMenuItem(
					text = { Text(stringResource(Res.string.send_stop_sharing)) },
					onClick = {
						expanded = false
						onStopSharing()
					},
					leadingIcon = { PlatformIcon(AppIcon.StopCircle, contentDescription = null) },
				)
			}
			DropdownMenuItem(
				text = { Text(stringResource(Res.string.button_delete_transfer)) },
				onClick = {
					expanded = false
					onDelete()
				},
				leadingIcon = {
					PlatformIcon(
						AppIcon.Delete,
						contentDescription = null,
						tint = LocalVniDropColors.current.destructiveDefault,
					)
				},
			)
		}
	}
}

@Composable
internal fun FileArtwork(thumbnailBytes: ByteArray?, modifier: Modifier = Modifier) {
	val bitmap = remember(thumbnailBytes) { thumbnailBytes?.let { runCatching { it.decodeToImageBitmap() }.getOrNull() } }
	if (bitmap != null) {
		androidx.compose.foundation.Image(
			bitmap = bitmap,
			contentDescription = null,
			modifier = modifier.clip(RoundedCornerShape(10.dp)),
			contentScale = ContentScale.Crop,
		)
	} else {
		Box(modifier, contentAlignment = Alignment.Center) {
			PlatformIcon(AppIcon.File, contentDescription = null, tint = LocalVniDropColors.current.foregroundLight, modifier = Modifier.size(22.dp))
		}
	}
}

private fun TransferStatus.pillTone(): PillTone = when (this) {
	TransferStatus.Sharing, TransferStatus.Done -> PillTone.Brand
	TransferStatus.Importing, TransferStatus.Receiving -> PillTone.Warning
	TransferStatus.Failed, TransferStatus.Cancelled -> PillTone.Destructive
	TransferStatus.Stopped -> PillTone.Neutral
}
