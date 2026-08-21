package com.vnidrop.app.feature.approvals

import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.ReceiverDeliveryStatus
import com.vnidrop.app.notifications.LocalNotification
import com.vnidrop.app.notifications.LocalNotificationService
import com.vnidrop.app.notifications.LocalizedNotificationTextFormatter
import com.vnidrop.app.notifications.NotificationPermission
import com.vnidrop.app.notifications.NotificationTextFormatter
import com.vnidrop.app.platform.AppVisibility
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class PendingApproval(
	val id: String,
	val transferId: ULong,
	val transferName: String,
	val receiverName: String?,
	val receiverDeviceName: String?,
	/** Cryptographic peer identity from the Iroh connection — not display-name spoofable. */
	val remoteEndpointId: String,
	val requestedAt: Long,
)

data class ApprovalState(
	val pending: List<PendingApproval> = emptyList(),
	val respondingIds: Set<String> = emptySet(),
) {
	val current: PendingApproval?
		get() = pending.firstOrNull()
}

class ApprovalCoordinator internal constructor(
	private val repository: CoreGateway,
	private val preferencesRepository: PreferencesRepository,
	private val notifications: LocalNotificationService,
	private val visibility: AppVisibility,
	private val messages: UiMessageController,
	private val scope: CoroutineScope,
	private val notificationText: NotificationTextFormatter,
) {
	constructor(
		repository: CoreGateway,
		preferencesRepository: PreferencesRepository,
		notifications: LocalNotificationService,
		visibility: AppVisibility,
		messages: UiMessageController,
		scope: CoroutineScope,
	) : this(
		repository,
		preferencesRepository,
		notifications,
		visibility,
		messages,
		scope,
		LocalizedNotificationTextFormatter,
	)

	private val _state = MutableStateFlow(ApprovalState())
	val state: StateFlow<ApprovalState> = _state.asStateFlow()

	private val publishedNotificationIds = mutableSetOf<String>()

	init {
		scope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					is CoreSignal.ApprovalChanged -> refresh(signal.transferId)
					is CoreSignal.ReceiverHistoryChanged,
					is CoreSignal.TransfersChanged,
					CoreSignal.PairingChanged,
					CoreSignal.TargetedTransferChanged,
					CoreSignal.RuntimeObligationChanged -> Unit
				}
			}
		}
		scope.launch {
			repository.state.collectLatest { core ->
				if (core.isInitialized) {
					core.transfers
						.filter { it.direction == TransferDirection.Send && it.status == TransferStatus.Sharing }
						.forEach { refresh(it.transferId) }
				}
			}
		}
		scope.launch {
			combine(
				preferencesRepository.preferences,
				visibility.isForeground,
				state,
				notifications.permission,
			) { preferences, foreground, approvalState, permission ->
				NotificationContext(preferences.notificationsEnabled, foreground, approvalState.pending, permission)
			}.collect { context ->
				synchronizeNotifications(context)
			}
		}
	}

	fun accept(requestId: String) = respond(requestId, accepted = true)

	fun refuse(requestId: String) = respond(requestId, accepted = false)

	private fun respond(requestId: String, accepted: Boolean) {
		if (!_state.value.respondingIds.addable(requestId)) return
		_state.update { it.copy(respondingIds = it.respondingIds + requestId) }
		scope.launch {
			val request = _state.value.pending.firstOrNull { it.id == requestId }
			val result = repository.respondReceiverRequest(
				requestId = requestId,
				accepted = accepted,
				reason = if (accepted) null else "sender-refused",
			)
			_state.update { it.copy(respondingIds = it.respondingIds - requestId) }
			result.fold(
				onSuccess = {
					if (request != null) refresh(request.transferId)
				},
				onFailure = messages::error,
			)
		}
	}

	private suspend fun refresh(transferId: ULong) {
		repository.receiverRequests(transferId).fold(
			onSuccess = { requests ->
				val refreshed = requests.filter { it.status == ReceiverDeliveryStatus.Requested }.map(ReceiverRequestModel::toPending)
				val removed = _state.value.pending.filter { it.transferId == transferId }.map { it.id }.toSet() - refreshed.map { it.id }.toSet()
				removed.forEach { id ->
					notifications.cancel(notificationId(id))
					publishedNotificationIds.remove(id)
				}
				_state.update { current ->
					current.copy(
						pending = (current.pending.filterNot { it.transferId == transferId } + refreshed)
							.distinctBy(PendingApproval::id)
							.sortedBy(PendingApproval::requestedAt),
					)
				}
			},
			onFailure = messages::error,
		)
	}

	private suspend fun synchronizeNotifications(context: NotificationContext) {
		if (context.foreground || !context.enabled || context.permission != NotificationPermission.Granted) {
			publishedNotificationIds.forEach { notifications.cancel(notificationId(it)) }
			return
		}
		context.pending.filterNot { it.id in publishedNotificationIds }.forEach { request ->
			val text = notificationText.approvalRequest(
				receiver = request.receiverName ?: request.receiverDeviceName,
				transferName = request.transferName,
			)
			notifications.publish(
				LocalNotification(
					id = notificationId(request.id),
					title = text.title,
					body = text.body,
				),
			).onSuccess {
				publishedNotificationIds += request.id
			}.onFailure(messages::error)
		}
	}
}

private data class NotificationContext(
	val enabled: Boolean,
	val foreground: Boolean,
	val pending: List<PendingApproval>,
	val permission: NotificationPermission,
)

private fun ReceiverRequestModel.toPending(): PendingApproval = PendingApproval(
	id = id,
	transferId = transferId,
	transferName = transferName,
	receiverName = receiverName,
	receiverDeviceName = receiverDeviceName,
	remoteEndpointId = remoteEndpointId,
	requestedAt = requestedAt,
)

private fun Set<String>.addable(value: String): Boolean = value !in this

private fun notificationId(requestId: String): String = "approval-$requestId"
