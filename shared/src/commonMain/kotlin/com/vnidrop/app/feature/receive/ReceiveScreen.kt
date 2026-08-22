package com.vnidrop.app.feature.receive

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.FolderAccessStatus
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.isDesktop
import com.vnidrop.app.ui.components.AppContextMenuItem
import com.vnidrop.app.ui.components.AdaptiveDrawer
import com.vnidrop.app.ui.components.DestructiveButton
import com.vnidrop.app.ui.components.DestructiveQuietButton
import com.vnidrop.app.ui.components.Field
import com.vnidrop.app.ui.components.FeatureEmptyState
import com.vnidrop.app.ui.components.PrimaryButton
import com.vnidrop.app.ui.components.ProgressRow
import com.vnidrop.app.ui.components.PlatformContextMenu
import com.vnidrop.app.ui.components.QuietButton
import com.vnidrop.app.ui.components.SecondaryButton
import com.vnidrop.app.ui.feedback.UiText
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.platform.usesMobilePresentation
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.state.displayNameForStatus
import com.vnidrop.app.ui.state.formatBytes
import com.vnidrop.app.ui.state.progressForTransfer
import com.vnidrop.app.ui.theme.LocalVniDropColors
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.*

@Composable
fun ReceiveFloatingAction(onClick: () -> Unit, modifier: Modifier = Modifier) {
	FloatingActionButton(
		onClick = onClick,
		modifier = modifier,
		containerColor = LocalVniDropColors.current.brandButton,
		contentColor = Color.White,
	) { PlatformIcon(AppIcon.Download, stringResource(Res.string.button_receive_files)) }
}

@Composable
fun ReceiveScreen(
	coreState: CoreState,
	state: ReceiveState,
	windowClass: WindowClass,
	actions: ReceiveInvitationActions,
	onOpenAcquisition: () -> Unit,
	onDismissAcquisition: () -> Unit,
	onReceiverNameChanged: (String) -> Unit,
	onInvitationResult: (ReceiveMethod, Result<String>) -> Unit,
	onWaitingForNfc: (Boolean) -> Unit,
	onReceive: () -> Unit,
	onCancelReceive: () -> Unit = {},
	onRequestDeleteHistoryItem: (ULong) -> Unit,
	onRequestClearHistory: () -> Unit,
	onDismissHistoryDelete: () -> Unit,
	onConfirmHistoryDelete: () -> Unit,
) {
	val transfers = coreState.transfers.filter { it.direction == TransferDirection.Receive }
	val deletableTransfers = transfers.filter { it.status.isTerminalReceiveHistory() }
	val usesFloatingAction = usesMobilePresentation(LocalUiPlatform.current, windowClass)
	LazyColumn(
		modifier = Modifier.fillMaxSize().statusBarsPadding(),
		contentPadding = PaddingValues(16.dp),
		verticalArrangement = Arrangement.spacedBy(14.dp),
	) {
		item { ReceiveHeader(transfers.isNotEmpty() && !usesFloatingAction, onOpenAcquisition) }
		if (transfers.isEmpty()) item { ReceiveEmptyState(onOpenAcquisition) }
		else {
			item {
				Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
					Text(stringResource(Res.string.receive_history_title), modifier = Modifier.weight(1f), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
					if (deletableTransfers.isNotEmpty()) DestructiveQuietButton(stringResource(Res.string.receive_clear_history), onClick = onRequestClearHistory)
				}
			}
			items(transfers, key = Transfer::localId) { transfer ->
				ReceiveTransferRow(
					transfer = transfer,
					progress = progressForTransfer(coreState.events, transfer.transferId),
					onDelete = { onRequestDeleteHistoryItem(transfer.transferId) },
				)
			}
		}
	}

	if (state.isAcquisitionOpen) {
		AdaptiveDrawer(windowClass, onDismissAcquisition) {
			if (state.ticket.isBlank()) {
				ReceiveMethodPanel(
					actions = actions,
					isWaitingForNfc = state.isWaitingForNfc,
					onResult = onInvitationResult,
					onWaitingForNfc = onWaitingForNfc,
				)
			} else {
				InvitationReviewPanel(
					state = state,
					coreInitialized = coreState.isInitialized,
					events = coreState.events,
					onReceiverNameChanged = onReceiverNameChanged,
					onReceive = onReceive,
					onCancelReceive = onCancelReceive,
				)
			}
		}
	}

	state.historyDeleteTarget?.let { target ->
		val transferName = (target as? ReceiveHistoryDeleteTarget.Transfer)?.let { selected ->
			transfers.firstOrNull { it.transferId == selected.transferId }?.transferName
		}
		AdaptiveDrawer(windowClass, onDismissHistoryDelete) {
			ReceiveHistoryDeletePanel(
				clearAll = target == ReceiveHistoryDeleteTarget.All,
				transferName = transferName,
				isDeleting = state.isDeletingHistory,
				onCancel = onDismissHistoryDelete,
				onConfirm = onConfirmHistoryDelete,
			)
		}
	}
}

@Composable
private fun ReceiveHeader(showAction: Boolean, onOpen: () -> Unit) {
	Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
		Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
			Text(stringResource(Res.string.receive_title), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
		}
		if (showAction) {
			Spacer(Modifier.width(16.dp))
			PrimaryButton(stringResource(Res.string.button_receive_files), onClick = onOpen)
		}
	}
}

@Composable
private fun ReceiveEmptyState(onOpen: () -> Unit) {
	Box(Modifier.fillMaxWidth().heightIn(min = 430.dp), contentAlignment = Alignment.Center) {
		FeatureEmptyState(
			icon = AppIcon.Download,
			title = stringResource(Res.string.receive_empty_title),
			description = stringResource(Res.string.receive_empty_body),
			iconTestTag = "receive-empty-icon",
		) {
			PrimaryButton(
				stringResource(Res.string.button_receive_files),
				onClick = onOpen,
				leadingIcon = {
					PlatformIcon(
						AppIcon.Add,
						contentDescription = null,
						modifier = Modifier.size(18.dp).testTag("receive-empty-action-icon"),
					)
				},
			)
		}
	}
}

@Composable
private fun ReceiveMethodPanel(
	actions: ReceiveInvitationActions,
	isWaitingForNfc: Boolean,
	onResult: (ReceiveMethod, Result<String>) -> Unit,
	onWaitingForNfc: (Boolean) -> Unit,
) {
	Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
		Text(stringResource(Res.string.receive_choose_method_title), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
		Text(stringResource(Res.string.receive_choose_method_body), color = LocalVniDropColors.current.foregroundLighter)
		ReceiveMethodRow(
			AppIcon.File, stringResource(Res.string.receive_method_file), stringResource(Res.string.receive_method_file_description),
			actions.fileAvailability,
		) { actions.pickInvitation { onResult(ReceiveMethod.InvitationFile, it) } }
		if (actions.qrAvailability != ReceiveMethodAvailability.Hidden) ReceiveMethodRow(
			AppIcon.Scan, stringResource(Res.string.receive_method_scan), stringResource(Res.string.receive_method_scan_description),
			actions.qrAvailability,
		) { actions.scanQrCode { onResult(ReceiveMethod.QrCode, it) } }
		if (actions.nfcAvailability != ReceiveMethodAvailability.Hidden) ReceiveMethodRow(
			AppIcon.Nfc,
			if (isWaitingForNfc) stringResource(Res.string.receive_nfc_waiting) else stringResource(Res.string.receive_method_nfc),
			stringResource(Res.string.receive_method_nfc_description),
			if (isWaitingForNfc) ReceiveMethodAvailability.Unavailable else actions.nfcAvailability,
		) {
			onWaitingForNfc(true)
			actions.readNfcInvitation { onResult(ReceiveMethod.Nfc, it) }
		}
	}
}

@Composable
private fun ReceiveMethodRow(icon: AppIcon, title: String, description: String, availability: ReceiveMethodAvailability, onClick: () -> Unit) {
	val enabled = availability == ReceiveMethodAvailability.Available
	Surface(
		modifier = Modifier.fillMaxWidth().clickable(enabled = enabled, onClick = onClick),
		shape = RoundedCornerShape(14.dp),
		color = LocalVniDropColors.current.backgroundSurface200,
	) {
		Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
			PlatformIcon(icon, null, tint = if (enabled) LocalVniDropColors.current.brandLink else LocalVniDropColors.current.foregroundLighter, modifier = Modifier.size(24.dp))
			Spacer(Modifier.width(14.dp))
			Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
				Text(title, fontWeight = FontWeight.SemiBold)
				Text(description, color = LocalVniDropColors.current.foregroundLighter, style = MaterialTheme.typography.bodySmall)
			}
			if (availability == ReceiveMethodAvailability.Unavailable) Text(stringResource(Res.string.value_unavailable), color = LocalVniDropColors.current.foregroundLighter, style = MaterialTheme.typography.labelSmall)
		}
	}
}

@Composable
private fun InvitationReviewPanel(
	state: ReceiveState,
	coreInitialized: Boolean,
	events: List<com.vnidrop.app.core.CoreEventModel>,
	onReceiverNameChanged: (String) -> Unit,
	onReceive: () -> Unit,
	onCancelReceive: () -> Unit,
) {
	Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 14.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
		Text(stringResource(Res.string.receive_review_title), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
		if (state.isInspecting) Box(Modifier.fillMaxWidth().padding(40.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
		state.inspection?.let { inspection ->
			val metadata = inspection.metadata
			Surface(shape = RoundedCornerShape(14.dp), color = LocalVniDropColors.current.backgroundSurface200) {
				Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
					Text(metadata.transferName, fontWeight = FontWeight.Bold, maxLines = 2, overflow = TextOverflow.Ellipsis)
					Text("${metadata.fileCount} ${stringResource(Res.string.metadata_files).lowercase()} · ${formatBytes(metadata.totalSize)}", color = LocalVniDropColors.current.foregroundLighter)
				}
			}
			Field(state.receiverName, onReceiverNameChanged, stringResource(Res.string.field_receiver_name))
			Text(
				state.receiveFolder?.displayName ?: stringResource(Res.string.value_unavailable),
				color = if (state.folderAccessStatus == FolderAccessStatus.Writable) LocalVniDropColors.current.foregroundLight else LocalVniDropColors.current.destructiveDefault,
				style = MaterialTheme.typography.bodySmall,
			)
			if (state.isReceiving) {
				val progressId = state.activeReceiveTransferId
					?: events.firstOrNull { it.direction == "receive" && it.transferId != null }?.transferId
				val progress = progressId?.let { progressForTransfer(events, it) }
				ProgressRow(
					label = progress?.label ?: Res.string.progress_receiving,
					progress = progress?.progress,
					detail = progress?.detail,
				)
				SecondaryButton(
					stringResource(Res.string.button_cancel_receive),
					onClick = onCancelReceive,
					modifier = Modifier.fillMaxWidth(),
				)
			} else {
				PrimaryButton(
					stringResource(Res.string.button_receive),
					onClick = onReceive,
					modifier = Modifier.fillMaxWidth(),
					enabled = state.canReceive(coreInitialized),
				)
			}
			state.lastReceiveError?.let { error ->
				Text(
					when (error) {
						is UiText.Dynamic -> error.value
						is UiText.Resource -> stringResource(error.resource, *error.formatArgs.toTypedArray())
					},
					color = LocalVniDropColors.current.destructiveDefault,
					style = MaterialTheme.typography.bodySmall,
				)
			}
		}
	}
}

@Composable
private fun ReceiveTransferRow(
	transfer: Transfer,
	progress: com.vnidrop.app.ui.state.TransferProgress?,
	onDelete: () -> Unit,
) {
	val terminal = transfer.status.isTerminalReceiveHistory()
	val usesDesktopMenu = LocalUiPlatform.current.isDesktop
	val contextMenuItems = if (usesDesktopMenu && terminal) {
		listOf(AppContextMenuItem(stringResource(Res.string.receive_delete_history_item), onDelete))
	} else {
		emptyList()
	}
	PlatformContextMenu(contextMenuItems) {
		Surface(Modifier.fillMaxWidth(), shape = RoundedCornerShape(14.dp), color = LocalVniDropColors.current.backgroundSurface200) {
			Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
			Box(Modifier.size(44.dp).background(LocalVniDropColors.current.backgroundSurface300, RoundedCornerShape(10.dp)), contentAlignment = Alignment.Center) {
				PlatformIcon(AppIcon.File, null, tint = LocalVniDropColors.current.foregroundLighter)
			}
			Spacer(Modifier.width(12.dp))
			Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
				Text(transfer.transferName ?: stringResource(Res.string.receive_unknown_transfer), fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
				Text("${formatBytes(transfer.totalSize)} · ${displayNameForStatus(transfer.status)}", color = LocalVniDropColors.current.foregroundLighter, style = MaterialTheme.typography.bodySmall)
				if (transfer.status == TransferStatus.Receiving && progress != null) {
					ProgressRow(label = progress.label, progress = progress.progress, detail = progress.detail)
				}
			}
			if (terminal && !usesDesktopMenu) {
				IconButton(onClick = onDelete) {
					PlatformIcon(AppIcon.Delete, stringResource(Res.string.receive_delete_history_item), tint = LocalVniDropColors.current.destructiveDefault)
				}
			}
			}
		}
	}
}

@Composable
private fun ReceiveHistoryDeletePanel(
	clearAll: Boolean,
	transferName: String?,
	isDeleting: Boolean,
	onCancel: () -> Unit,
	onConfirm: () -> Unit,
) {
	Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 14.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
		Text(
			stringResource(if (clearAll) Res.string.receive_clear_history_title else Res.string.receive_delete_history_title),
			style = MaterialTheme.typography.titleLarge,
			fontWeight = FontWeight.Bold,
		)
		Text(
			if (clearAll) stringResource(Res.string.receive_clear_history_description)
			else stringResource(Res.string.receive_delete_history_description, transferName ?: stringResource(Res.string.receive_unknown_transfer)),
			color = LocalVniDropColors.current.foregroundLighter,
		)
		Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End)) {
			QuietButton(stringResource(Res.string.button_cancel), onClick = onCancel, enabled = !isDeleting)
			DestructiveQuietButton(
				if (isDeleting) stringResource(Res.string.transfer_deleting)
				else stringResource(if (clearAll) Res.string.receive_clear_history else Res.string.button_delete_transfer),
				onClick = onConfirm,
				enabled = !isDeleting,
			)
		}
	}
}
