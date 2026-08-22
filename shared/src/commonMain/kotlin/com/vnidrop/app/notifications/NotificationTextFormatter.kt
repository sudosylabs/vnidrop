package com.vnidrop.app.notifications

import org.jetbrains.compose.resources.getString
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.approval_connection_request
import vnidrop.shared.generated.resources.approval_nearby_device
import vnidrop.shared.generated.resources.approval_request_body
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

internal data class NotificationText(
	val title: String,
	val body: String,
)

internal interface NotificationTextFormatter {
	suspend fun approvalRequest(receiver: String?, transferName: String): NotificationText

	suspend fun transfer(plan: PlannedTransferNotification): NotificationText
}

internal object LocalizedNotificationTextFormatter : NotificationTextFormatter {
	override suspend fun approvalRequest(receiver: String?, transferName: String): NotificationText {
		val displayReceiver = receiver ?: getString(Res.string.approval_nearby_device)
		return NotificationText(
			title = getString(Res.string.approval_connection_request),
			body = getString(Res.string.approval_request_body, displayReceiver, transferName),
		)
	}

	override suspend fun transfer(plan: PlannedTransferNotification): NotificationText {
		val transferName = plan.transferName ?: getString(Res.string.receive_unknown_transfer)
		return when (plan.kind) {
			TransferNotificationKind.SendFailed -> NotificationText(
				getString(Res.string.notifications_send_failed_title),
				getString(Res.string.notifications_send_failed_body, transferName),
			)
			TransferNotificationKind.ReceiveCompleted -> NotificationText(
				getString(Res.string.notifications_receive_completed_title),
				getString(Res.string.notifications_receive_completed_body, transferName),
			)
			TransferNotificationKind.ReceiveFailed -> NotificationText(
				getString(Res.string.notifications_receive_failed_title),
				getString(Res.string.notifications_receive_failed_body, transferName),
			)
			TransferNotificationKind.ReceiverCompleted -> {
				val receiver = plan.receiver ?: getString(Res.string.approval_nearby_device)
				NotificationText(
					getString(Res.string.notifications_receiver_completed_title),
					getString(Res.string.notifications_receiver_completed_body, receiver, transferName),
				)
			}
			TransferNotificationKind.ReceiverFailed -> {
				val receiver = plan.receiver ?: getString(Res.string.approval_nearby_device)
				NotificationText(
					getString(Res.string.notifications_receiver_failed_title),
					getString(Res.string.notifications_receiver_failed_body, receiver, transferName),
				)
			}
			TransferNotificationKind.TargetedOffer -> {
				val sender = plan.receiver ?: getString(Res.string.approval_nearby_device)
				NotificationText(
					getString(Res.string.targeted_offer_title),
					getString(Res.string.targeted_offer_body, sender, transferName),
				)
			}
		}
	}
}
