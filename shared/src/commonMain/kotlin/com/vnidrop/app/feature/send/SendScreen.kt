package com.vnidrop.app.feature.send

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.ReceiverDeliveryStatus
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.ui.components.AdaptiveDrawer
import com.vnidrop.app.ui.state.WindowClass

@Composable
fun SendScreen(
	coreState: CoreState,
	state: SendState,
	windowClass: WindowClass,
	shareActions: TransferShareActions = UnavailableTransferShareActions,
	onOpenComposer: () -> Unit,
	onDismissComposer: () -> Unit,
	onSelectFile: () -> Unit,
	onSelectFolder: () -> Unit = {},
	onClearFile: () -> Unit,
	onRemoveFile: (String) -> Unit = {},
	onTransferNameChanged: (String) -> Unit,
	onSenderNameChanged: (String) -> Unit,
	onAccessPolicyChanged: (ShareAccessPolicy) -> Unit,
	onCreateShare: () -> Unit,
	onTransferSelected: (ULong) -> Unit,
	onCloseTransferDetails: () -> Unit,
	onCopyTicket: (String) -> Unit,
	onActivity: () -> Unit = {},
	onReceivers: () -> Unit = {},
	onShare: () -> Unit = {},
	onCloseDetailPanel: () -> Unit = {},
	onInvitationResult: (InvitationAction, Result<Unit>) -> Unit = { _, _ -> },
	onRequestDelete: () -> Unit = {},
	onDismissDelete: () -> Unit = {},
	onConfirmDelete: () -> Unit = {},
) {
	val outgoingTransfers = coreState.transfers.filter { it.direction == TransferDirection.Send }
	val selectedTransfer = state.selectedTransferId?.let { id -> outgoingTransfers.firstOrNull { it.transferId == id } }
	val qrCache = remember { mutableStateMapOf<String, ImageBitmap>() }
	LaunchedEffect(outgoingTransfers.mapNotNull { it.ticket }) {
		qrCache.keys.retainAll(outgoingTransfers.mapNotNull { it.ticket }.toSet())
	}

	Box(Modifier.fillMaxSize()) {
		if (selectedTransfer != null) {
			TransferDetails(
				transfer = selectedTransfer,
				events = coreState.events,
				pendingReceivers = state.receiverHistory.count {
					it.status == ReceiverDeliveryStatus.Requested || it.status == ReceiverDeliveryStatus.Accepted
				},
				completedReceivers = state.receiverHistory.count { it.status == ReceiverDeliveryStatus.Completed },
				onBack = onCloseTransferDetails,
				onActivity = onActivity,
				onReceivers = onReceivers,
				onShare = onShare,
				onDelete = onRequestDelete,
			)
		} else {
			TransferCatalog(
				transfers = outgoingTransfers,
				transferThumbnails = state.transferThumbnails,
				receiversByTransfer = state.receiversByTransfer,
				events = coreState.events,
				windowClass = windowClass,
				onOpenComposer = onOpenComposer,
				onTransferSelected = onTransferSelected,
			)
		}
	}

	if (state.isComposerOpen) {
		AdaptiveDrawer(windowClass = windowClass, onDismissRequest = onDismissComposer) {
			TransferComposer(
				coreInitialized = coreState.isInitialized,
				state = state,
				windowClass = windowClass,
				onSelectFile = onSelectFile,
				onSelectFolder = onSelectFolder,
				onClearFile = onClearFile,
				onRemoveFile = onRemoveFile,
				onTransferNameChanged = onTransferNameChanged,
				onSenderNameChanged = onSenderNameChanged,
				onAccessPolicyChanged = onAccessPolicyChanged,
				onCreateShare = onCreateShare,
			)
		}
	}

	val canShowDetailPanel = selectedTransfer != null && when (state.detailPanel) {
		TransferDetailPanel.Share -> selectedTransfer.status in setOf(TransferStatus.Importing, TransferStatus.Sharing)
		TransferDetailPanel.Activity, TransferDetailPanel.Receivers -> true
		null -> false
	}
	if (selectedTransfer != null && state.detailPanel != null && canShowDetailPanel) {
		AdaptiveDrawer(windowClass = windowClass, onDismissRequest = onCloseDetailPanel) {
			when (state.detailPanel) {
				TransferDetailPanel.Activity -> TransferActivityPanel(coreState.events, selectedTransfer.transferId)
				TransferDetailPanel.Receivers -> ReceiverHistoryPanel(
					receivers = state.receiverHistory,
					loading = state.isLoadingReceivers,
					events = coreState.events,
					transferTotalSize = selectedTransfer.totalSize,
				)
				TransferDetailPanel.Share -> TransferSharePanel(
					selectedTransfer,
					shareActions,
					qrBitmap = selectedTransfer.ticket?.let(qrCache::get),
					onQrRendered = { ticket, bitmap -> qrCache[ticket] = bitmap },
					onResult = onInvitationResult,
				)
			}
		}
	}

	if (selectedTransfer != null && state.isDeleteConfirmationOpen) {
		AdaptiveDrawer(windowClass = windowClass, onDismissRequest = onDismissDelete) {
			DeleteTransferPanel(
				transferName = selectedTransfer.transferName,
				isDeleting = state.isDeleting,
				onCancel = onDismissDelete,
				onConfirm = onConfirmDelete,
			)
		}
	}
}
