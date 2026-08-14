import Combine
import Foundation

/// A transfer-lifecycle moment worth a local notification.
enum TransferNotificationKind: Equatable {
	case sendFailed          // A share you own failed.
	case receiveCompleted    // An incoming transfer finished downloading.
	case receiveFailed       // An incoming transfer failed.
	case receiverCompleted   // A receiver finished downloading your shared transfer.
	case receiverFailed      // A receiver's download of your shared transfer failed.
}

/// A notification resolved from core state but not yet published. `transferName`
/// is the raw name (may be nil); the coordinator localizes and applies fallbacks.
struct PlannedNotification: Equatable {
	let id: String
	let kind: TransferNotificationKind
	let transferName: String?
	let receiver: String?
}

/// Pure: transfer-status notifications for this snapshot, excluding already-published
/// ids. A terminal transfer yields at most one notification, keyed by (kind, id).
func plannedTransferNotifications(_ transfers: [Transfer], published: Set<String>) -> [PlannedNotification] {
	transfers.compactMap { transfer in
		let kind: TransferNotificationKind
		switch (transfer.direction, transfer.status) {
		case (.send, .failed): kind = .sendFailed
		case (.receive, .done): kind = .receiveCompleted
		case (.receive, .failed): kind = .receiveFailed
		default: return nil
		}
		let id = transferNotificationId(kind, transferId: transfer.transferId)
		guard !published.contains(id) else { return nil }
		return PlannedNotification(id: id, kind: kind, transferName: transfer.transferName, receiver: nil)
	}
}

/// Pure: one notification per receiver that has finished downloading a shared
/// transfer, excluding already-published ids.
func plannedReceiverNotifications(_ requests: [ReceiverRequestModel], published: Set<String>) -> [PlannedNotification] {
	requests.compactMap { request in
		let kind: TransferNotificationKind
		let idPrefix: String
		switch request.status {
		case .completed: kind = .receiverCompleted; idPrefix = "receiver-completed"
		case .failed: kind = .receiverFailed; idPrefix = "receiver-failed"
		default: return nil
		}
		let id = "\(idPrefix)-\(request.id)"
		guard !published.contains(id) else { return nil }
		return PlannedNotification(
			id: id, kind: kind,
			transferName: request.transferName,
			receiver: request.receiverName ?? request.receiverDeviceName
		)
	}
}

private func transferNotificationId(_ kind: TransferNotificationKind, transferId: UInt64) -> String {
	switch kind {
	case .sendFailed: return "send-failed-\(transferId)"
	case .receiveCompleted: return "receive-completed-\(transferId)"
	case .receiveFailed: return "receive-failed-\(transferId)"
	case .receiverCompleted: return "receiver-completed-\(transferId)"
	case .receiverFailed: return "receiver-failed-\(transferId)"
	}
}

/// Fires local notifications for transfer-lifecycle moments (a receive finishing
/// or failing, a share failing, a receiver completing), so a user who left the
/// app can see the outcome. Approval prompts are handled by `ApprovalCoordinator`.
///
/// Gated on the OS notification permission (and, on iOS, on being backgrounded).
/// Each moment is terminal, so it is marked seen the first time it is observed and
/// never re-published. The first state snapshot — which includes existing history
/// such as past receives — only primes those ids as seen, so only new transitions
/// notify.
@MainActor
final class TransferNotificationCoordinator: ObservableObject {
	private let repository: CoreGateway
	private let notifications: LocalNotificationService
	private let visibility: AppVisibility
	private let messages: UiMessageController

	private var published = Set<String>()
	private var primedTransfers = false
	private var cancellables = Set<AnyCancellable>()

	init(
		repository: CoreGateway,
		notifications: LocalNotificationService,
		visibility: AppVisibility,
		messages: UiMessageController
	) {
		self.repository = repository
		self.notifications = notifications
		self.visibility = visibility
		self.messages = messages

		repository.statePublisher
			.sink { [weak self] core in
				guard let self, core.isInitialized else { return }
				Task { await self.syncTransfers(core.transfers) }
			}
			.store(in: &cancellables)

		repository.signals
			.sink { [weak self] signal in
				guard let self else { return }
				switch signal {
				case .receiverHistoryChanged(let transferId), .transfersChanged(let transferId):
					Task { await self.syncReceivers(transferId: transferId) }
				case .approvalChanged:
					break
				case .pairingChanged, .targetedTransferChanged:
					// Saved-device notifications are owned by the saved-device
					// coordinator, which tracks its own notification identifiers.
					break
				}
			}
			.store(in: &cancellables)
	}

	/// iOS suppresses notifications while the user is in the app (the convention);
	/// macOS presents them even when active (also the convention — the app window
	/// is usually open), relying on the presenter delegate to show the banner.
	private var canPublish: Bool {
		guard notifications.permission == .granted else { return false }
		#if os(iOS)
		return !visibility.isForeground
		#else
		return true
		#endif
	}

	private func syncTransfers(_ transfers: [Transfer]) async {
		let planned = plannedTransferNotifications(transfers, published: published)
		guard primedTransfers else {
			// The first snapshot includes existing history (e.g. past receives).
			// Mark those terminal transfers seen without notifying, so only new
			// transitions notify.
			primedTransfers = true
			for plan in planned { published.insert(plan.id) }
			return
		}
		for plan in planned { await deliver(plan) }
	}

	private func syncReceivers(transferId: UInt64) async {
		let result = await repository.receiverRequests(transferId: transferId)
		switch result {
		case .success(let requests):
			for plan in plannedReceiverNotifications(requests, published: published) {
				await deliver(plan)
			}
		case .failure(let error):
			messages.error(error)
		}
	}

	/// Mark seen unconditionally (a terminal moment notifies at most once), then
	/// publish only when the gate allows.
	private func deliver(_ plan: PlannedNotification) async {
		published.insert(plan.id)
		guard canPublish else { return }
		let name = plan.transferName ?? String(localized: L10n.Receive.unknownTransfer)
		let notification: LocalNotification
		switch plan.kind {
		case .sendFailed:
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Notifications.sendFailedTitle),
				body: L10n.Notifications.sendFailedBody(transferName: name))
		case .receiveCompleted:
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Notifications.receiveCompletedTitle),
				body: L10n.Notifications.receiveCompletedBody(transferName: name))
		case .receiveFailed:
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Notifications.receiveFailedTitle),
				body: L10n.Notifications.receiveFailedBody(transferName: name))
		case .receiverCompleted:
			let receiver = plan.receiver ?? String(localized: L10n.Approval.nearbyDevice)
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Notifications.receiverCompletedTitle),
				body: L10n.Notifications.receiverCompletedBody(receiver: receiver, transferName: name))
		case .receiverFailed:
			let receiver = plan.receiver ?? String(localized: L10n.Approval.nearbyDevice)
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Notifications.receiverFailedTitle),
				body: L10n.Notifications.receiverFailedBody(receiver: receiver, transferName: name))
		}
		if case .failure(let error) = await notifications.publish(notification) {
			messages.error(error)
		}
	}
}
