import Combine
import Foundation

/// A saved-device moment worth a local notification.
enum SavedDeviceNotificationKind: Equatable {
	/// A peer asked to pair and is waiting on an answer.
	case pairingRequest
	/// A targeted offer is waiting on approve/decline.
	case targetedOffer
	/// An incoming targeted transfer finished downloading here.
	case targetedReceiveCompleted
	/// An incoming targeted transfer failed here.
	case targetedReceiveFailed
	/// A transfer we sent finished downloading on the peer.
	case targetedSendCompleted
	/// A transfer we sent failed on the peer.
	case targetedSendFailed
}

extension SavedDeviceNotificationKind {
	/// Stable id component. Distinct per kind so a transfer cannot collide with
	/// itself across directions.
	var idPrefix: String {
		switch self {
		case .pairingRequest: return "pairing-request"
		case .targetedOffer: return "offer"
		case .targetedReceiveCompleted: return "receive-completed"
		case .targetedReceiveFailed: return "receive-failed"
		case .targetedSendCompleted: return "send-completed"
		case .targetedSendFailed: return "send-failed"
		}
	}
}

/// A saved-device notification resolved from model state but not yet published.
struct PlannedSavedDeviceNotification: Equatable {
	let id: String
	let kind: SavedDeviceNotificationKind
	/// Peer display name, already resolved; nil when we have no trustworthy one.
	let deviceName: String?
	let transferName: String?
}

/// Pure: notifications for consent decisions currently waiting on the user.
/// These are *pending* moments — they are withdrawn when the underlying request
/// disappears, so the caller cancels ids that stop appearing here.
func plannedSavedDevicePrompts(_ state: SavedDevicesState) -> [PlannedSavedDeviceNotification] {
	var planned: [PlannedSavedDeviceNotification] = []
	for relationship in state.pendingRelationships where relationship.state == .pendingIncoming {
		planned.append(PlannedSavedDeviceNotification(
			id: "pairing-request-\(relationship.remoteEndpointId)",
			kind: .pairingRequest,
			deviceName: state.eligibilities
				.first { $0.peerEndpointId == relationship.remoteEndpointId }?
				.remoteDisplayName
				?? state.device(relationship.remoteEndpointId)?.displayNameOrNil,
			transferName: nil
		))
	}
	for offer in state.targetedOffers.pending {
		planned.append(PlannedSavedDeviceNotification(
			id: "targeted-offer-\(offer.transferId)",
			kind: .targetedOffer,
			// Only a saved sender has a name we can vouch for.
			deviceName: state.targetedOffers.senderDisplayNames[offer.senderEndpointId],
			transferName: offer.transferName
		))
	}
	return planned
}

/// Pure: notifications for targeted transfers that reached a terminal outcome,
/// excluding already-published ids. Terminal moments notify at most once.
func plannedTargetedOutcomes(
	_ transfers: [SavedDeviceTransferItem],
	published: Set<String>
) -> [PlannedSavedDeviceNotification] {
	transfers.compactMap { transfer in
		// Direction decides the wording: on the receiving device the transfer
		// "finished downloading", while on the sending device it is the *peer*
		// that finished. Ignoring direction told the sender it had downloaded
		// its own outgoing files.
		let kind: SavedDeviceNotificationKind
		switch (transfer.direction, transfer.state) {
		case (.incoming, .completed): kind = .targetedReceiveCompleted
		case (.incoming, .failed): kind = .targetedReceiveFailed
		case (.outgoing, .completed): kind = .targetedSendCompleted
		case (.outgoing, .failed): kind = .targetedSendFailed
		// Cancelled and declined are the user's own doing on one side or the
		// other; notifying about them would be noise.
		default: return nil
		}
		let id = "targeted-\(kind.idPrefix)-\(transfer.id)"
		guard !published.contains(id) else { return nil }
		return PlannedSavedDeviceNotification(
			id: id, kind: kind,
			deviceName: transfer.peerDisplayName,
			transferName: transfer.transferName
		)
	}
}

/// Fires local notifications for the saved-device domain: pairing requests and
/// targeted offers waiting on a decision, and targeted transfers reaching a
/// terminal outcome. Invitation-share moments belong to
/// `TransferNotificationCoordinator`; approval prompts to `ApprovalCoordinator`.
///
/// Reads `SavedDevicesModel`'s published snapshot rather than querying the core
/// again — the model already refreshes on every saved-device signal, and a second
/// read loop would race it.
@MainActor
final class SavedDeviceNotificationCoordinator: ObservableObject {
	private let notifications: LocalNotificationService
	private let visibility: AppVisibility
	private let messages: UiMessageController

	/// Prompt ids currently published, so they can be withdrawn when answered.
	private var publishedPrompts = Set<String>()
	/// Terminal ids already seen; never re-published.
	private var publishedOutcomes = Set<String>()
	private var primedOutcomes = false
	private var cancellables = Set<AnyCancellable>()

	init(
		model: SavedDevicesModel,
		notifications: LocalNotificationService,
		visibility: AppVisibility,
		messages: UiMessageController
	) {
		self.notifications = notifications
		self.visibility = visibility
		self.messages = messages

		// Recompute when any input changes: the snapshot itself, foregrounding, or
		// the permission being granted later.
		Publishers.CombineLatest3(
			model.$state,
			visibility.$isForeground,
			notifications.$permission
		)
		.sink { [weak self] state, _, _ in
			guard let self else { return }
			Task { await self.synchronize(state) }
		}
		.store(in: &cancellables)
	}

	/// iOS suppresses notifications while the user is in the app (the modal or the
	/// screen shows instead); macOS presents them even when active, since the app
	/// window is usually open. Mirrors the other two coordinators.
	private var canPublish: Bool {
		guard notifications.permission == .granted else { return false }
		#if os(iOS)
		return !visibility.isForeground
		#else
		return true
		#endif
	}

	private func synchronize(_ state: SavedDevicesState) async {
		await synchronizePrompts(state)
		await synchronizeOutcomes(state)
	}

	private func synchronizePrompts(_ state: SavedDevicesState) async {
		let planned = plannedSavedDevicePrompts(state)
		let plannedIds = Set(planned.map(\.id))

		// Withdraw notifications whose request is gone — answered here, answered on
		// the peer, expired, or dropped when the core restarted. Leaving them would
		// invite the user to act on a decision that no longer exists.
		for id in publishedPrompts.subtracting(plannedIds) {
			notifications.cancel(id: id)
			publishedPrompts.remove(id)
		}

		guard canPublish else {
			// Suppressed: withdraw anything already showing, but keep the request
			// itself pending so it can notify again later.
			for id in publishedPrompts { notifications.cancel(id: id) }
			publishedPrompts.removeAll()
			return
		}

		for plan in planned where !publishedPrompts.contains(plan.id) {
			// Reserve the id *before* awaiting: CombineLatest can fire repeatedly in
			// quick succession, and a repeated add of an in-flight identifier is
			// coalesced into a silent update with no banner.
			publishedPrompts.insert(plan.id)
			if case .failure(let error) = await publish(plan) {
				publishedPrompts.remove(plan.id)
				messages.error(error)
			}
		}
	}

	private func synchronizeOutcomes(_ state: SavedDevicesState) async {
		let planned = plannedTargetedOutcomes(state.targetedTransfers, published: publishedOutcomes)
		guard primedOutcomes else {
			// The first snapshot carries existing history; mark those terminal
			// transfers seen without notifying so only new transitions notify.
			primedOutcomes = true
			for plan in planned { publishedOutcomes.insert(plan.id) }
			return
		}
		for plan in planned {
			// Mark seen unconditionally — a terminal moment notifies at most once,
			// whether or not the gate let it through.
			publishedOutcomes.insert(plan.id)
			guard canPublish else { continue }
			if case .failure(let error) = await publish(plan) {
				messages.error(error)
			}
		}
	}

	private func publish(_ plan: PlannedSavedDeviceNotification) async -> Result<Void, Error> {
		let device = plan.deviceName ?? String(localized: L10n.Approval.nearbyDevice)
		let transferName = plan.transferName ?? String(localized: L10n.Receive.unknownTransfer)
		let notification: LocalNotification
		switch plan.kind {
		case .pairingRequest:
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Pairing.requestTitle),
				body: L10n.Pairing.requestBody(device: device)
			)
		case .targetedOffer:
			// The sender is offering to send to us — not the invitation-approval
			// case, where the remote device asks to receive from us.
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Targeted.offerTitle),
				body: L10n.Targeted.offerBody(device: device, transferName: transferName)
			)
		case .targetedReceiveCompleted:
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Notifications.receiveCompletedTitle),
				body: L10n.Notifications.receiveCompletedBody(transferName: transferName)
			)
		case .targetedReceiveFailed:
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Notifications.receiveFailedTitle),
				body: L10n.Notifications.receiveFailedBody(transferName: transferName)
			)
		case .targetedSendCompleted:
			// We are the sender: the *peer* finished downloading, not us.
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Notifications.receiverCompletedTitle),
				body: L10n.Notifications.receiverCompletedBody(receiver: device, transferName: transferName)
			)
		case .targetedSendFailed:
			notification = LocalNotification(
				id: plan.id,
				title: String(localized: L10n.Notifications.receiverFailedTitle),
				body: L10n.Notifications.receiverFailedBody(receiver: device, transferName: transferName)
			)
		}
		return await notifications.publish(notification)
	}
}
