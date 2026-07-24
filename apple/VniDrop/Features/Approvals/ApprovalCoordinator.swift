import Foundation
import Combine

/// A pending receiver approval, ported from `feature/approvals/ApprovalCoordinator.kt`.
struct PendingApproval: Equatable, Identifiable {
	let id: String
	let transferId: UInt64
	let transferName: String
	let receiverName: String?
	let receiverDeviceName: String?
	/// Cryptographic peer identity — not display-name spoofable.
	let remoteEndpointId: String
	let requestedAt: Int64
}

struct ApprovalState: Equatable {
	var pending: [PendingApproval] = []
	var respondingIds: Set<String> = []

	var current: PendingApproval? { pending.first }
}

/// Drives receiver-approval prompts and their notifications, ported from
/// `ApprovalCoordinator.kt`.
@MainActor
final class ApprovalCoordinator: ObservableObject {
	@Published private(set) var state = ApprovalState()

	private let repository: CoreGateway
	private let notifications: LocalNotificationService
	private let visibility: AppVisibility
	private let messages: UiMessageController

	private var publishedNotificationIds = Set<String>()
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

		repository.signals
			.sink { [weak self] signal in
				guard let self else { return }
				if case .approvalChanged(let transferId) = signal {
					Task { await self.refresh(transferId: transferId) }
				}
			}
			.store(in: &cancellables)

		repository.statePublisher
			.sink { [weak self] core in
				guard let self, core.isInitialized else { return }
				let sharing = core.transfers.filter { $0.direction == .send && $0.status == .sharing }
				for transfer in sharing {
					Task { await self.refresh(transferId: transfer.transferId) }
				}
			}
			.store(in: &cancellables)

		// Recompute notifications when any input changes.
		Publishers.CombineLatest3(
			visibility.$isForeground,
			$state,
			notifications.$permission
		)
		.sink { [weak self] foreground, approvalState, permission in
			guard let self else { return }
			Task {
				await self.synchronizeNotifications(
					foreground: foreground,
					pending: approvalState.pending,
					permission: permission
				)
			}
		}
		.store(in: &cancellables)
	}

	func accept(_ requestId: String) { respond(requestId, accepted: true) }
	func refuse(_ requestId: String) { respond(requestId, accepted: false) }

	private func respond(_ requestId: String, accepted: Bool) {
		guard !state.respondingIds.contains(requestId) else { return }
		state.respondingIds.insert(requestId)
		Task {
			let request = state.pending.first { $0.id == requestId }
			let result = await repository.respondReceiverRequest(
				requestId: requestId,
				accepted: accepted,
				reason: accepted ? nil : "sender-refused"
			)
			state.respondingIds.remove(requestId)
			switch result {
			case .success:
				if let request { await refresh(transferId: request.transferId) }
			case .failure(let error):
				messages.error(error)
			}
		}
	}

	private func refresh(transferId: UInt64) async {
		let result = await repository.receiverRequests(transferId: transferId)
		switch result {
		case .success(let requests):
			let refreshed = requests
				.filter { $0.status == .requested }
				.map { $0.toPending() }
			let refreshedIds = Set(refreshed.map { $0.id })
			let removed = Set(state.pending.filter { $0.transferId == transferId }.map { $0.id })
				.subtracting(refreshedIds)
			for id in removed {
				notifications.cancel(id: Self.notificationId(id))
				publishedNotificationIds.remove(id)
			}
			var pending = state.pending.filter { $0.transferId != transferId } + refreshed
			// distinctBy id, sorted by requestedAt
			var seen = Set<String>()
			pending = pending.filter { seen.insert($0.id).inserted }
				.sorted { $0.requestedAt < $1.requestedAt }
			state.pending = pending
		case .failure(let error):
			messages.error(error)
		}
	}

	private func synchronizeNotifications(
		foreground: Bool,
		pending: [PendingApproval],
		permission: NotificationPermission
	) async {
		// iOS suppresses notifications while the user is in the app (the modal shows
		// instead); macOS presents them even when active (the app window is usually
		// open), relying on the presenter delegate.
		#if os(iOS)
		let suppressed = foreground || permission != .granted
		#else
		let suppressed = permission != .granted
		#endif
		if suppressed {
			// Cancel only our own approval notifications — other coordinators
			// (e.g. transfer-lifecycle) manage their own and must not be wiped.
			for id in publishedNotificationIds { notifications.cancel(id: Self.notificationId(id)) }
			return
		}
		for request in pending where !publishedNotificationIds.contains(request.id) {
			// Reserve the id *before* awaiting: the CombineLatest can fire several
			// times near-simultaneously, and without this each pass re-adds the same
			// notification identifier. macOS coalesces a repeated add of an in-flight
			// id into a silent update and shows no banner.
			publishedNotificationIds.insert(request.id)
			let receiver = request.receiverName
				?? request.receiverDeviceName
				?? String(localized: L10n.Approval.nearbyDevice)
			let title = String(localized: L10n.Approval.connectionRequest)
			let body = L10n.Approval.requestBody(receiver: receiver, transferName: request.transferName)
			let result = await notifications.publish(
				LocalNotification(id: Self.notificationId(request.id), title: title, body: body)
			)
			if case .failure(let error) = result {
				publishedNotificationIds.remove(request.id)
				messages.error(error)
			}
		}
	}

	private static func notificationId(_ requestId: String) -> String { "approval-\(requestId)" }
}

private extension ReceiverRequestModel {
	func toPending() -> PendingApproval {
		PendingApproval(
			id: id, transferId: transferId, transferName: transferName,
			receiverName: receiverName, receiverDeviceName: receiverDeviceName,
			remoteEndpointId: remoteEndpointId, requestedAt: requestedAt
		)
	}
}
