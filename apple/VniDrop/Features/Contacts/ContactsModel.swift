import Combine
import Foundation

/// A device worth remembering after a completed transfer.
///
/// Only a suggestion: nothing is issued until the user agrees, because being
/// reachable is a standing permission and a transfer is a one-off.
struct PairingSuggestion: Equatable, Identifiable {
	let endpointId: String
	let displayName: String?
	let transferName: String?

	var id: String { endpointId }

	var resolvedName: String {
		guard let displayName, !displayName.isEmpty else {
			return String(localized: L10n.Approval.nearbyDevice)
		}
		return displayName
	}
}

struct ContactsState: Equatable {
	var contacts: [DeviceContact] = []
	var blocked: [String] = []
	var pendingPairings: [PendingPairingModel] = []
	var pendingOffers: [IncomingOfferModel] = []
	var grantLifetime: GrantLifetimeOption = .days90
	var isLoading = false
	/// Endpoints with an in-flight decision, so a row can disable itself without
	/// blocking the rest of the list.
	var busyEndpoints: Set<String> = []
	var busyOfferIds: Set<String> = []
	var suggestions: [PairingSuggestion] = []
	var selectedEndpointId: String?

	var selected: DeviceContact? {
		guard let selectedEndpointId else { return nil }
		return contacts.first { $0.endpointId == selectedEndpointId }
	}

	/// One prompt at a time: pairing consent is a modal decision and stacking
	/// sheets on top of each other reads as a loop of dialogs.
	var currentPairing: PendingPairingModel? { pendingPairings.first }
	var currentOffer: IncomingOfferModel? { pendingOffers.first }
	var currentSuggestion: PairingSuggestion? { suggestions.first }
}

/// Drives the device-history surfaces: the list, its detail, and the two
/// consent prompts. Ported in the MVVM shape used by the other feature models.
@MainActor
final class ContactsModel: ObservableObject {
	@Published private(set) var state = ContactsState()

	private let repository: CoreGateway
	private let messages: UiMessageController
	private let preferences: AppPreferencesRepository
	private var cancellables = Set<AnyCancellable>()

	init(
		repository: CoreGateway,
		messages: UiMessageController,
		preferences: AppPreferencesRepository
	) {
		self.repository = repository
		self.messages = messages
		self.preferences = preferences
		state.grantLifetime = preferences.preferences.grantLifetime

		repository.signals
			.sink { [weak self] signal in
				guard let self else { return }
				switch signal {
				case .contactsChanged:
					Task { await self.refresh() }
				case .offersChanged:
					Task { await self.refreshOffers() }
				case .receiverHistoryChanged(let transferId), .transfersChanged(let transferId):
					// A completed delivery names the device that received from us.
					Task { await self.considerSendPeers(transferId: transferId) }
				case .approvalChanged:
					break
				}
			}
			.store(in: &cancellables)

		repository.statePublisher
			.sink { [weak self] core in
				guard let self, core.isInitialized else { return }
				self.considerReceivePeers(core.transfers)
			}
			.store(in: &cancellables)

		repository.statePublisher
			.map(\.isInitialized)
			.removeDuplicates()
			.sink { [weak self] isInitialized in
				guard let self, isInitialized else { return }
				// The core owns the lifetime; push the stored preference on start
				// so a restart does not silently fall back to the default.
				Task {
					await self.repository.setGrantLifetime(self.state.grantLifetime)
					await self.refresh()
				}
			}
			.store(in: &cancellables)
	}

	// MARK: - Loading

	func refresh() async {
		state.isLoading = true
		defer { state.isLoading = false }

		switch await repository.contacts() {
		case .success(let contacts):
			state.contacts = contacts
		case .failure(let error):
			messages.error(error)
		}
		if case .success(let blocked) = await repository.blockedContacts() {
			state.blocked = blocked
		}
		state.pendingPairings = await repository.pendingPairings()
		await refreshOffers()
	}

	func refreshOffers() async {
		state.pendingOffers = await repository.pendingOffers()
	}

	// MARK: - Post-transfer suggestions

	/// A completed receive names its sender, so that device becomes a candidate.
	private func considerReceivePeers(_ transfers: [Transfer]) {
		let candidates = transfers
			.filter { $0.direction == .receive && $0.status == .done }
			.compactMap { transfer -> PairingSuggestion? in
				guard let peerId = transfer.peerId else { return nil }
				return PairingSuggestion(
					endpointId: peerId,
					displayName: nil,
					transferName: transfer.transferName
				)
			}
		add(suggestions: candidates)
	}

	/// A completed delivery names the device we sent to.
	private func considerSendPeers(transferId: UInt64) async {
		guard case .success(let requests) = await repository.receiverRequests(transferId: transferId) else {
			return
		}
		let candidates = requests
			.filter { $0.status == .completed }
			.map { request in
				PairingSuggestion(
					endpointId: request.remoteEndpointId,
					displayName: request.receiverName ?? request.receiverDeviceName,
					transferName: request.transferName
				)
			}
		add(suggestions: candidates)
	}

	/// Filters candidates down to devices actually worth asking about.
	private func add(suggestions candidates: [PairingSuggestion]) {
		let known = Set(state.contacts.map(\.endpointId))
		let blocked = Set(state.blocked)
		let declined = preferences.preferences.declinedPairingSuggestions
		let pending = Set(state.suggestions.map(\.endpointId))

		let fresh = candidates.filter { candidate in
			!known.contains(candidate.endpointId)
				&& !blocked.contains(candidate.endpointId)
				&& !declined.contains(candidate.endpointId)
				&& !pending.contains(candidate.endpointId)
		}
		guard !fresh.isEmpty else { return }
		state.suggestions.append(contentsOf: fresh)
	}

	/// Agree to be reachable by a suggested device.
	func acceptSuggestion(_ suggestion: PairingSuggestion) async {
		state.suggestions.removeAll { $0.endpointId == suggestion.endpointId }
		preferences.clearDeclinedPairingSuggestion(suggestion.endpointId)
		await allowDeviceToReachMe(
			endpointId: suggestion.endpointId,
			displayName: preferences.preferences.username
		)
	}

	/// Decline, and remember the decline so the next transfer does not re-ask.
	func declineSuggestion(_ suggestion: PairingSuggestion) {
		state.suggestions.removeAll { $0.endpointId == suggestion.endpointId }
		preferences.declinePairingSuggestion(suggestion.endpointId)
	}

	// MARK: - Selection

	func select(_ endpointId: String?) { state.selectedEndpointId = endpointId }

	// MARK: - Pairing consent

	/// Agree to be reachable by a device, typically right after a transfer.
	func allowDeviceToReachMe(endpointId: String, displayName: String?) async {
		state.busyEndpoints.insert(endpointId)
		defer { state.busyEndpoints.remove(endpointId) }

		if case .failure(let error) = await repository.allowDeviceToReachMe(
			endpointId: endpointId,
			displayName: displayName
		) {
			messages.error(error)
			return
		}
		await refresh()
	}

	/// Answer a device's offer to be remembered.
	func respondToPairing(endpointId: String, accepted: Bool) async {
		state.busyEndpoints.insert(endpointId)
		defer { state.busyEndpoints.remove(endpointId) }

		switch await repository.respondToPairing(endpointId: endpointId, accepted: accepted) {
		case .success:
			// Drop the prompt immediately: the core has already consumed it, and
			// leaving it on screen invites a second answer that does nothing.
			state.pendingPairings.removeAll { $0.endpointId == endpointId }
			if accepted { await refresh() }
		case .failure(let error):
			messages.error(error)
		}
	}

	// MARK: - Incoming offers

	/// Answer an incoming offer. Returns the ticket when accepted so the caller
	/// can run the receive with a platform-appropriate destination; the core
	/// releases it only on acceptance.
	func respondToOffer(offerId: String, accepted: Bool) async -> String? {
		state.busyOfferIds.insert(offerId)
		defer { state.busyOfferIds.remove(offerId) }

		let ticket = await repository.respondToOffer(offerId: offerId, accepted: accepted)
		state.pendingOffers.removeAll { $0.offerId == offerId }
		return ticket
	}

	// MARK: - Management

	func setLabel(endpointId: String, label: String) async {
		let trimmed = label.trimmingCharacters(in: .whitespacesAndNewlines)
		if case .failure(let error) = await repository.setContactLabel(
			endpointId: endpointId,
			label: trimmed.isEmpty ? nil : trimmed
		) {
			messages.error(error)
			return
		}
		await refresh()
	}

	func forget(endpointId: String) async {
		state.busyEndpoints.insert(endpointId)
		defer { state.busyEndpoints.remove(endpointId) }

		if case .failure(let error) = await repository.forgetContact(endpointId: endpointId) {
			messages.error(error)
			return
		}
		if state.selectedEndpointId == endpointId { state.selectedEndpointId = nil }
		await refresh()
	}

	func forgetAll() async {
		if case .failure(let error) = await repository.forgetAllContacts() {
			messages.error(error)
			return
		}
		state.selectedEndpointId = nil
		await refresh()
	}

	func block(endpointId: String) async {
		state.busyEndpoints.insert(endpointId)
		defer { state.busyEndpoints.remove(endpointId) }

		if case .failure(let error) = await repository.blockContact(endpointId: endpointId) {
			messages.error(error)
			return
		}
		if state.selectedEndpointId == endpointId { state.selectedEndpointId = nil }
		await refresh()
	}

	func unblock(endpointId: String) async {
		if case .failure(let error) = await repository.unblockContact(endpointId: endpointId) {
			messages.error(error)
			return
		}
		await refresh()
	}

	func setGrantLifetime(_ lifetime: GrantLifetimeOption) {
		state.grantLifetime = lifetime
		preferences.setGrantLifetime(lifetime)
		Task { await repository.setGrantLifetime(lifetime) }
	}
}
