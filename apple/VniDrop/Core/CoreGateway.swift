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
	func resetUnrecoverableIdentity(appDataDir: String, networkConfiguration: RelayConfiguration) async -> Result<Void, Error>
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

	// MARK: - Saved devices

	func listPairingEligibilities() async -> Result<[PairingEligibilityModel], Error>
	func declinePairingEligibility(peerEndpointId: String) async -> Result<Void, Error>
	/// Asks a peer to pair. Returns false when no valid eligibility exists, which
	/// the core rejects silently so a stranger cannot provoke a prompt.
	func requestSavedDevicePairing(peerEndpointId: String) async -> Result<Bool, Error>
	func respondToDevicePairing(peerEndpointId: String, accepted: Bool) async -> Result<Bool, Error>
	func listDeviceRelationships() async -> Result<[DeviceRelationshipModel], Error>
	func listSavedDevices() async -> Result<[SavedDeviceModel], Error>
	/// Sets or clears (`nil`) the user-owned local label.
	func setSavedDeviceLabel(peerEndpointId: String, label: String?) async -> Result<Void, Error>
	func forgetSavedDevice(peerEndpointId: String) async -> Result<Void, Error>
	func blockDevice(peerEndpointId: String) async -> Result<Void, Error>
	/// Removes only the deny rule. Grants, relationships, and cancelled transfers
	/// are not restored — re-saving needs another qualifying transfer and consent.
	func unblockDevice(peerEndpointId: String) async -> Result<Void, Error>
	func listBlockedDevices() async -> Result<[String], Error>

	// MARK: - Targeted transfers

	func listPendingTargetedOffers() async -> Result<[PendingTargetedOfferModel], Error>
	func respondToTargetedOffer(
		transferId: String,
		accepted: Bool
	) async -> Result<TargetedOfferResponseModel, Error>
	func createTargetedTransfer(
		receiverEndpointId: String,
		sources: [ShareSource],
		transferName: String?
	) async -> Result<TargetedTransferModel, Error>
	func listTargetedTransfers() async -> Result<[TargetedTransferModel], Error>
	/// Pulls an approved transfer into a security-scoped destination, holding
	/// access for the duration of the stream (mirrors the invitation receive path).
	func receiveTargetedTransfer(
		transferId: String,
		outputDirectoryUrl: String
	) async -> Result<Void, Error>
	/// Resumes an interrupted transfer. The same immutable transfer continues from
	/// its verified progress and is not re-approved.
	func resumeTargetedTransfer(
		id: String,
		outputDirectoryUrl: String
	) async -> Result<Void, Error>
	func cancelTargetedTransfer(id: String) async -> Result<Void, Error>
	func deleteTargetedTransfer(id: String) async -> Result<Void, Error>
}
