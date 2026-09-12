package com.vnidrop.app.notifications

internal object FakeNotificationTextFormatter : NotificationTextFormatter {
	override suspend fun approvalRequest(receiver: String?, transferName: String): NotificationText =
		NotificationText(
			title = "Connection request",
			body = "${receiver ?: "Nearby device"} wants to receive “$transferName”.",
		)

	override suspend fun transfer(plan: PlannedTransferNotification): NotificationText {
		val transferName = plan.transferName ?: "Unknown transfer"
		return when (plan.kind) {
			TransferNotificationKind.SendFailed -> NotificationText("Send failed", "Couldn’t send “$transferName”.")
			TransferNotificationKind.ReceiveCompleted -> NotificationText("Transfer received", "Received “$transferName”.")
			TransferNotificationKind.ReceiveFailed -> NotificationText("Receive failed", "Couldn’t receive “$transferName”.")
			TransferNotificationKind.ReceiverCompleted -> NotificationText("Transfer delivered", "${plan.receiver ?: "Nearby device"} received “$transferName”.")
			TransferNotificationKind.ReceiverFailed -> NotificationText("Delivery failed", "${plan.receiver ?: "Nearby device"} couldn’t receive “$transferName”.")
		}
	}

	override suspend fun savedDevice(plan: PlannedSavedDeviceNotification): NotificationText {
		val device = plan.deviceName ?: "Nearby device"
		val transferName = plan.transferName ?: "Unknown transfer"
		return when (plan.kind) {
			SavedDeviceNotificationKind.PairingRequest ->
				NotificationText("Remember this device?", "$device offered to let you send it files without a new invitation.")
			SavedDeviceNotificationKind.TargetedOffer ->
				NotificationText("Incoming transfer", "$device wants to send you “$transferName”.")
			SavedDeviceNotificationKind.TargetedReceiveCompleted ->
				NotificationText("Transfer received", "Received “$transferName”.")
			SavedDeviceNotificationKind.TargetedReceiveFailed ->
				NotificationText("Receive failed", "Couldn’t receive “$transferName”.")
			SavedDeviceNotificationKind.TargetedSendCompleted ->
				NotificationText("Transfer delivered", "$device received “$transferName”.")
			SavedDeviceNotificationKind.TargetedSendFailed ->
				NotificationText("Delivery failed", "$device couldn’t receive “$transferName”.")
		}
	}
}
