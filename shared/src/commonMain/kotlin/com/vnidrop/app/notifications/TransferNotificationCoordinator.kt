package com.vnidrop.app.notifications

import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.ReceiverDeliveryStatus
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.platform.AppVisibility
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import org.jetbrains.compose.resources.getString
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.approval_nearby_device
import vnidrop.shared.generated.resources.notifications_receive_completed_body
import vnidrop.shared.generated.resources.notifications_receive_completed_title
import vnidrop.shared.generated.resources.notifications_receive_failed_body
import vnidrop.shared.generated.resources.notifications_receive_failed_title
import vnidrop.shared.generated.resources.notifications_receiver_completed_body
import vnidrop.shared.generated.resources.notifications_receiver_completed_title
import vnidrop.shared.generated.resources.notifications_receiver_failed_body
import vnidrop.shared.generated.resources.notifications_receiver_failed_title
import vnidrop.shared.generated.resources.notifications_send_failed_body
import vnidrop.shared.generated.resources.notifications_send_failed_title
import vnidrop.shared.generated.resources.receive_unknown_transfer
import vnidrop.shared.generated.resources.targeted_offer_body
import vnidrop.shared.generated.resources.targeted_offer_title

internal enum class TransferNotificationKind {
	SendFailed,
	ReceiveCompleted,
	ReceiveFailed,
	ReceiverCompleted,
	ReceiverFailed,
	TargetedOffer,
}

internal data class PlannedTransferNotification(
	val id: String,
	val kind: TransferNotificationKind,
	val transferName: String?,
	val receiver: String? = null,
)

internal fun plannedTransferNotifications(
	transfers: List<Transfer>,
	published: Set<String>,
): List<PlannedTransferNotification> = transfers.mapNotNull { transfer ->
	val kind = when {
		transfer.direction == TransferDirection.Send && transfer.status == TransferStatus.Failed ->
			TransferNotificationKind.SendFailed
		transfer.direction == TransferDirection.Receive && transfer.status == TransferStatus.Done ->
			TransferNotificationKind.ReceiveCompleted
		transfer.direction == TransferDirection.Receive && transfer.status == TransferStatus.Failed ->
			TransferNotificationKind.ReceiveFailed
		else -> return@mapNotNull null
	}
	val id = "${kind.idPrefix}-${transfer.transferId}"
	PlannedTransferNotification(id, kind, transfer.transferName).takeUnless { id in published }
}

internal fun plannedReceiverNotifications(
	requests: List<ReceiverRequestModel>,
	published: Set<String>,
): List<PlannedTransferNotification> = requests.mapNotNull { request ->
	val kind = when (request.status) {
		ReceiverDeliveryStatus.Completed -> TransferNotificationKind.ReceiverCompleted
		ReceiverDeliveryStatus.Failed -> TransferNotificationKind.ReceiverFailed
		else -> return@mapNotNull null
	}
	val id = "${kind.idPrefix}-${request.id}"
	PlannedTransferNotification(
		id = id,
		kind = kind,
		transferName = request.transferName,
		receiver = request.receiverName ?: request.receiverDeviceName,
	).takeUnless { id in published }
}

internal fun plannedTargetedOfferNotifications(
	offers: List<PendingTargetedOfferModel>,
	savedDeviceNames: Map<String, String>,
	published: Set<String>,
): List<PlannedTransferNotification> = offers.mapNotNull { offer ->
	val id = "${TransferNotificationKind.TargetedOffer.idPrefix}-${offer.transferId}"
	PlannedTransferNotification(
		id = id,
		kind = TransferNotificationKind.TargetedOffer,
		transferName = offer.transferName,
		receiver = savedDeviceNames[offer.senderEndpointId],
	).takeUnless { id in published }
}

class TransferNotificationCoordinator(
	private val repository: CoreGateway,
	private val preferencesRepository: PreferencesRepository,
	private val notifications: LocalNotificationService,
	private val visibility: AppVisibility,
	private val messages: UiMessageController,
	private val scope: CoroutineScope,
) {
	private val published = mutableSetOf<String>()
	private var transfersPrimed = false
	private var notificationsEnabled = false

	init {
		scope.launch {
			preferencesRepository.preferences.collectLatest { preferences ->
				notificationsEnabled = preferences.notificationsEnabled
			}
		}
		scope.launch {
			repository.state.collect { core ->
				if (core.isInitialized) syncTransfers(core.transfers)
			}
		}
		scope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					is CoreSignal.ReceiverHistoryChanged -> syncReceivers(signal.transferId)
					is CoreSignal.TransfersChanged -> syncReceivers(signal.transferId)
					is CoreSignal.ApprovalChanged,
					CoreSignal.PairingChanged -> Unit
					CoreSignal.TargetedTransferChanged -> syncTargetedOffers()
				}
			}
		}
	}

	private suspend fun syncTransfers(transfers: List<Transfer>) {
		val planned = plannedTransferNotifications(transfers, published)
		if (!transfersPrimed) {
			transfersPrimed = true
			published += planned.map(PlannedTransferNotification::id)
			return
		}
		planned.forEach { deliver(it) }
	}

	private suspend fun syncReceivers(transferId: ULong) {
		val isOutgoing = repository.state.value.transfers.any {
			it.transferId == transferId && it.direction == TransferDirection.Send
		}
		if (!isOutgoing) return
		repository.receiverRequests(transferId).fold(
			onSuccess = { requests ->
				plannedReceiverNotifications(requests, published).forEach { deliver(it) }
			},
			onFailure = messages::error,
		)
	}

	private suspend fun syncTargetedOffers() {
		val offers = repository.listPendingTargetedOffers().getOrElse {
			messages.error(it)
			return
		}
		if (offers.isEmpty()) return
		val savedDevices = repository.listSavedDevices().getOrElse {
			messages.error(it)
			return
		}
		val savedDeviceNames = savedDevices.mapNotNull { device ->
			device.displayNameOrNull()?.let { device.endpointId to it }
		}.toMap()
		plannedTargetedOfferNotifications(offers, savedDeviceNames, published).forEach { deliver(it) }
	}

	private suspend fun deliver(plan: PlannedTransferNotification) {
		published += plan.id
		if (
			!notificationsEnabled ||
			visibility.isForeground.value ||
			notifications.permission.value != NotificationPermission.Granted
		) return
		val transferName = plan.transferName ?: getString(Res.string.receive_unknown_transfer)
		val notification = when (plan.kind) {
			TransferNotificationKind.SendFailed -> LocalNotification(
				plan.id,
				getString(Res.string.notifications_send_failed_title),
				getString(Res.string.notifications_send_failed_body, transferName),
			)
			TransferNotificationKind.ReceiveCompleted -> LocalNotification(
				plan.id,
				getString(Res.string.notifications_receive_completed_title),
				getString(Res.string.notifications_receive_completed_body, transferName),
			)
			TransferNotificationKind.ReceiveFailed -> LocalNotification(
				plan.id,
				getString(Res.string.notifications_receive_failed_title),
				getString(Res.string.notifications_receive_failed_body, transferName),
			)
			TransferNotificationKind.ReceiverCompleted -> {
				val receiver = plan.receiver ?: getString(Res.string.approval_nearby_device)
				LocalNotification(
					plan.id,
					getString(Res.string.notifications_receiver_completed_title),
					getString(Res.string.notifications_receiver_completed_body, receiver, transferName),
				)
			}
			TransferNotificationKind.ReceiverFailed -> {
				val receiver = plan.receiver ?: getString(Res.string.approval_nearby_device)
				LocalNotification(
					plan.id,
					getString(Res.string.notifications_receiver_failed_title),
					getString(Res.string.notifications_receiver_failed_body, receiver, transferName),
				)
			}
			TransferNotificationKind.TargetedOffer -> {
				val sender = plan.receiver ?: getString(Res.string.approval_nearby_device)
				LocalNotification(
					plan.id,
					getString(Res.string.targeted_offer_title),
					getString(Res.string.targeted_offer_body, sender, transferName),
				)
			}
		}
		notifications.publish(notification).onFailure(messages::error)
	}
}

private val TransferNotificationKind.idPrefix: String
	get() = when (this) {
		TransferNotificationKind.SendFailed -> "send-failed"
		TransferNotificationKind.ReceiveCompleted -> "receive-completed"
		TransferNotificationKind.ReceiveFailed -> "receive-failed"
		TransferNotificationKind.ReceiverCompleted -> "receiver-completed"
		TransferNotificationKind.ReceiverFailed -> "receiver-failed"
		TransferNotificationKind.TargetedOffer -> "targeted-offer"
	}

private fun SavedDeviceModel.displayNameOrNull(): String? =
	localLabel?.takeIf(String::isNotBlank) ?: remoteDisplayName?.takeIf(String::isNotBlank)
