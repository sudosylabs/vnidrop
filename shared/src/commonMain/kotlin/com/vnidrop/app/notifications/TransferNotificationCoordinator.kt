package com.vnidrop.app.notifications

import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.ReceiverDeliveryStatus
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.TargetedTransferStateModel
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.feature.saveddevices.SavedDeviceTransferDirection
import com.vnidrop.app.feature.saveddevices.SavedDeviceTransferItem
import com.vnidrop.app.feature.saveddevices.SavedDevicesReadInputs
import com.vnidrop.app.feature.saveddevices.SavedDevicesReadModel
import com.vnidrop.app.feature.saveddevices.SavedDevicesReadSnapshot
import com.vnidrop.app.platform.AppVisibility
import com.vnidrop.app.runtime.SavedDeviceListenIntent
import com.vnidrop.app.ui.feedback.UiMessageController
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal enum class TransferNotificationKind {
	SendFailed,
	ReceiveCompleted,
	ReceiveFailed,
	ReceiverCompleted,
	ReceiverFailed,
}

internal data class PlannedTransferNotification(
	val id: String,
	val kind: TransferNotificationKind,
	val transferName: String?,
	val receiver: String? = null,
)

internal enum class SavedDeviceNotificationKind {
	PairingRequest,
	TargetedOffer,
	TargetedReceiveCompleted,
	TargetedReceiveFailed,
	TargetedSendCompleted,
	TargetedSendFailed,
}

internal data class PlannedSavedDeviceNotification(
	val id: String,
	val kind: SavedDeviceNotificationKind,
	val deviceName: String?,
	val transferName: String?,
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

internal fun plannedSavedDevicePrompts(
	snapshot: SavedDevicesReadSnapshot,
): List<PlannedSavedDeviceNotification> = buildList {
	snapshot.pendingRelationships
		.filter { it.state == DeviceRelationshipStateModel.PendingIncoming }
		.forEach { relationship ->
			val name = snapshot.eligibilities
				.firstOrNull { it.peerEndpointId == relationship.remoteEndpointId }
				?.remoteDisplayName
				?: snapshot.senderDisplayNames[relationship.remoteEndpointId]
			add(
				PlannedSavedDeviceNotification(
					id = "pairing-request-${relationship.remoteEndpointId}",
					kind = SavedDeviceNotificationKind.PairingRequest,
					deviceName = name,
					transferName = null,
				),
			)
		}
	snapshot.pendingOffers.forEach { offer ->
		add(
			PlannedSavedDeviceNotification(
				id = "targeted-offer-${offer.transferId}",
				kind = SavedDeviceNotificationKind.TargetedOffer,
				deviceName = snapshot.senderDisplayNames[offer.senderEndpointId],
				transferName = offer.transferName,
			),
		)
	}
}

internal fun plannedTargetedOutcomes(
	transfers: List<SavedDeviceTransferItem>,
	published: Set<String>,
): List<PlannedSavedDeviceNotification> = transfers.mapNotNull { transfer ->
	val kind = when {
		transfer.direction == SavedDeviceTransferDirection.Incoming &&
			transfer.state == TargetedTransferStateModel.Completed ->
			SavedDeviceNotificationKind.TargetedReceiveCompleted
		transfer.direction == SavedDeviceTransferDirection.Incoming &&
			transfer.state == TargetedTransferStateModel.Failed ->
			SavedDeviceNotificationKind.TargetedReceiveFailed
		transfer.direction == SavedDeviceTransferDirection.Outgoing &&
			transfer.state == TargetedTransferStateModel.Completed ->
			SavedDeviceNotificationKind.TargetedSendCompleted
		transfer.direction == SavedDeviceTransferDirection.Outgoing &&
			transfer.state == TargetedTransferStateModel.Failed ->
			SavedDeviceNotificationKind.TargetedSendFailed
		else -> return@mapNotNull null
	}
	val id = "targeted-${kind.idPrefix}-${transfer.id}"
	PlannedSavedDeviceNotification(
		id = id,
		kind = kind,
		deviceName = transfer.peerDisplayName,
		transferName = transfer.transferName,
	).takeUnless { id in published }
}

class TransferNotificationCoordinator internal constructor(
	private val repository: CoreGateway,
	listenIntent: Flow<SavedDeviceListenIntent>,
	private val notifications: LocalNotificationService,
	private val visibility: AppVisibility,
	private val messages: UiMessageController,
	private val scope: CoroutineScope,
	private val notificationText: NotificationTextFormatter,
) {
	constructor(
		repository: CoreGateway,
		listenIntent: Flow<SavedDeviceListenIntent>,
		notifications: LocalNotificationService,
		visibility: AppVisibility,
		messages: UiMessageController,
		scope: CoroutineScope,
	) : this(
		repository,
		listenIntent,
		notifications,
		visibility,
		messages,
		scope,
		LocalizedNotificationTextFormatter,
	)

	private val publishedInvitationIds = mutableSetOf<String>()
	private var transfersPrimed = false
	private var listenEnabled = false
	private val publishedPrompts = mutableSetOf<String>()
	private val publishedOutcomes = mutableSetOf<String>()
	private var primedOutcomes = false
	private val snapshot = MutableStateFlow<SavedDevicesReadSnapshot?>(null)
	private val readModel = SavedDevicesReadModel()
	private val refreshMutex = Mutex()

	init {
		scope.launch {
			combine(snapshot, listenIntent, visibility.isForeground) { current, intent, foreground ->
				Triple(current, intent, foreground)
			}.collect { (current, intent, foreground) ->
				listenEnabled = intent.optedIn && intent.permissionGranted
				synchronizeSavedDevices(current, canPublish = listenEnabled && !foreground)
			}
		}
		scope.launch {
			repository.state.collect { core ->
				if (core.isInitialized) syncTransfers(core.transfers)
			}
		}
		scope.launch {
			repository.state.map { it.isInitialized }.distinctUntilChanged().collect { initialized ->
				if (initialized) refreshSavedDevices() else snapshot.value = null
			}
		}
		scope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					is CoreSignal.ReceiverHistoryChanged -> syncReceivers(signal.transferId)
					is CoreSignal.TransfersChanged -> syncReceivers(signal.transferId)
					CoreSignal.PairingChanged,
					CoreSignal.TargetedTransferChanged -> refreshSavedDevices()
					is CoreSignal.ApprovalChanged,
					CoreSignal.RuntimeObligationChanged -> Unit
				}
			}
		}
	}

	private suspend fun syncTransfers(transfers: List<Transfer>) {
		val planned = plannedTransferNotifications(transfers, publishedInvitationIds)
		if (!transfersPrimed) {
			transfersPrimed = true
			publishedInvitationIds += planned.map(PlannedTransferNotification::id)
			return
		}
		planned.forEach { deliverInvitation(it) }
	}

	private suspend fun syncReceivers(transferId: ULong) {
		val isOutgoing = repository.state.value.transfers.any {
			it.transferId == transferId && it.direction == TransferDirection.Send
		}
		if (!isOutgoing) return
		repository.receiverRequests(transferId).fold(
			onSuccess = { requests ->
				plannedReceiverNotifications(requests, publishedInvitationIds).forEach { deliverInvitation(it) }
			},
			onFailure = messages::error,
		)
	}

	private suspend fun refreshSavedDevices() {
		refreshMutex.withLock {
			if (!repository.state.value.isInitialized) return@withLock
			val eligibilities = repository.listPairingEligibilities().getOrElse {
				messages.error(it)
				return@withLock
			}
			val relationships = repository.listDeviceRelationships().getOrElse {
				messages.error(it)
				return@withLock
			}
			val savedDevices = repository.listSavedDevices().getOrElse {
				messages.error(it)
				return@withLock
			}
			val pendingOffers = repository.listPendingTargetedOffers().getOrElse {
				messages.error(it)
				return@withLock
			}
			val targetedTransfers = repository.listTargetedTransfers().getOrElse {
				messages.error(it)
				return@withLock
			}
			snapshot.value = readModel.derive(
				SavedDevicesReadInputs(
					eligibilities = eligibilities,
					relationships = relationships,
					savedDevices = savedDevices,
					pendingOffers = pendingOffers,
					targetedTransfers = targetedTransfers,
				),
			)
		}
	}

	private suspend fun synchronizeSavedDevices(
		snapshot: SavedDevicesReadSnapshot?,
		canPublish: Boolean,
	) {
		if (snapshot == null) {
			withdrawPrompts(keep = emptySet())
			return
		}
		synchronizePrompts(snapshot, canPublish)
		synchronizeOutcomes(snapshot, canPublish)
	}

	private suspend fun synchronizePrompts(
		snapshot: SavedDevicesReadSnapshot,
		canPublish: Boolean,
	) {
		val planned = plannedSavedDevicePrompts(snapshot)
		withdrawPrompts(keep = planned.map { it.id }.toSet())
		if (!canPublish) {
			withdrawPrompts(keep = emptySet())
			return
		}
		for (plan in planned) {
			if (plan.id in publishedPrompts) continue
			publishSavedDevice(plan).onSuccess { publishedPrompts += plan.id }
		}
	}

	private suspend fun withdrawPrompts(keep: Set<String>) {
		val stale = publishedPrompts.filterNot { it in keep }
		stale.forEach { id ->
			notifications.cancel(id)
			publishedPrompts.remove(id)
		}
	}

	private suspend fun synchronizeOutcomes(
		snapshot: SavedDevicesReadSnapshot,
		canPublish: Boolean,
	) {
		val planned = plannedTargetedOutcomes(snapshot.targetedTransfers, publishedOutcomes)
		if (!primedOutcomes) {
			primedOutcomes = true
			publishedOutcomes += planned.map { it.id }
			return
		}
		for (plan in planned) {
			publishedOutcomes += plan.id
			if (!canPublish) continue
			publishSavedDevice(plan)
		}
	}

	private suspend fun deliverInvitation(plan: PlannedTransferNotification) {
		publishedInvitationIds += plan.id
		if (!listenEnabled || visibility.isForeground.value) return
		val text = notificationText.transfer(plan)
		notifications.publish(LocalNotification(plan.id, text.title, text.body)).onFailure(messages::error)
	}

	private suspend fun publishSavedDevice(plan: PlannedSavedDeviceNotification): Result<Unit> {
		val text = notificationText.savedDevice(plan)
		return notifications.publish(LocalNotification(plan.id, text.title, text.body))
			.onFailure(messages::error)
	}
}

private val TransferNotificationKind.idPrefix: String
	get() = when (this) {
		TransferNotificationKind.SendFailed -> "send-failed"
		TransferNotificationKind.ReceiveCompleted -> "receive-completed"
		TransferNotificationKind.ReceiveFailed -> "receive-failed"
		TransferNotificationKind.ReceiverCompleted -> "receiver-completed"
		TransferNotificationKind.ReceiverFailed -> "receiver-failed"
	}

private val SavedDeviceNotificationKind.idPrefix: String
	get() = when (this) {
		SavedDeviceNotificationKind.PairingRequest -> "pairing-request"
		SavedDeviceNotificationKind.TargetedOffer -> "targeted-offer"
		SavedDeviceNotificationKind.TargetedReceiveCompleted -> "receive-completed"
		SavedDeviceNotificationKind.TargetedReceiveFailed -> "receive-failed"
		SavedDeviceNotificationKind.TargetedSendCompleted -> "send-completed"
		SavedDeviceNotificationKind.TargetedSendFailed -> "send-failed"
	}
