import Foundation
import Combine
import VnidropCore

struct CoreStorageUsageModel: Sendable {
	let blobStoreBytes: UInt64
	let appDataBytes: UInt64
}

struct ReceivedArtifactModel: Sendable {
	let locator: String
	let logicalSize: UInt64
}

/// Seam between the feature models and the Rust core, mirroring `CoreGateway`
/// in the KMP `shared` module. `CoreRepository` is the production implementation;
/// tests substitute a fake so the models can be exercised without the FFI.
@MainActor
protocol CoreGateway: AnyObject {
	/// Latest published core state.
	var state: CoreState { get }
	/// Publisher of core-state changes (the models subscribe to this).
	var statePublisher: AnyPublisher<CoreState, Never> { get }
	/// Coalesced change hints emitted by the event sink.
	var signals: AnyPublisher<CoreSignal, Never> { get }

	func initialize(appDataDir: String, networkConfiguration: RelayConfiguration) async -> Result<Void, Error>
	func shutdown()
	func shareSources(
		_ sources: [ShareSource],
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy
	) async -> Result<Share, Error>
	func inspectTicket(_ ticket: String) async -> Result<TicketInspectionModel, Error>
	func receive(ticket: String, outputDir: String, receiverName: String) async -> Result<Void, Error>
	func receiveIntoSecurityScopedDirectory(
		ticket: String,
		outputDirectoryUrl: String,
		receiverName: String
	) async -> Result<Void, Error>
	func cancel(transferId: UInt64) async -> Result<Void, Error>
	func delete(transferId: UInt64) async -> Result<Void, Error>
	func clearReceiveHistory() async -> Result<UInt64, Error>
	func storageUsage() async -> Result<CoreStorageUsageModel, Error>
	func receivedArtifacts() async -> Result<[ReceivedArtifactModel], Error>
	func receiverRequests(transferId: UInt64) async -> Result<[ReceiverRequestModel], Error>
	func respondReceiverRequest(requestId: String, accepted: Bool, reason: String?) async -> Result<Void, Error>
	func refresh() async -> Result<Void, Error>

	// MARK: Device history

	func contacts() async -> Result<[DeviceContact], Error>
	func pendingPairings() async -> [PendingPairingModel]
	func pendingOffers() async -> [IncomingOfferModel]
	/// Hand a device a revocable capability to reach this one.
	func allowDeviceToReachMe(endpointId: String, displayName: String?) async -> Result<Void, Error>
	/// Accept or decline a device's offer to be remembered.
	func respondToPairing(endpointId: String, accepted: Bool) async -> Result<Bool, Error>
	/// Answer an incoming offer. Returns the ticket on acceptance, which the
	/// caller passes to `receive` with a platform-appropriate destination.
	func respondToOffer(offerId: String, accepted: Bool) async -> String?
	func sendToContact(
		endpointId: String,
		sources: [ShareSource],
		transferName: String,
		senderName: String
	) async -> Result<ContactSendOutcome, Error>
	/// Transfers this device is holding for contacts that were not running.
	func heldOffers() async -> Result<[HeldOfferModel], Error>
	/// Ask remembered devices whether they hold anything for this one.
	///
	/// Only ever called from a foreground transition or an explicit user action:
	/// it reveals to every contact that this device is awake.
	func pollContactsForOffers() async -> Result<UInt64, Error>
	func forgetContact(endpointId: String) async -> Result<Void, Error>
	func forgetAllContacts() async -> Result<UInt64, Error>
	func blockContact(endpointId: String) async -> Result<Void, Error>
	func unblockContact(endpointId: String) async -> Result<Void, Error>
	func blockedContacts() async -> Result<[String], Error>
	func setContactLabel(endpointId: String, label: String?) async -> Result<Void, Error>
	func setGrantLifetime(_ lifetime: GrantLifetimeOption) async
}
