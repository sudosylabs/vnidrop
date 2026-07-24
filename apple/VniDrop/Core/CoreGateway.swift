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
}
