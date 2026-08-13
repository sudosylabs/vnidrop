import Foundation
import Combine
@preconcurrency import VnidropCore

enum CoreNetworkLifecycleError: Error, Equatable, LocalizedError, Sendable {
	case transitionInProgress
	case activeNetworkWork

	var errorDescription: String? {
		switch self {
		case .transitionInProgress: return "A network restart is already in progress."
		case .activeNetworkWork: return "Stop active transfers and shares before restarting the network."
		}
	}
}

enum CoreNetworkLifecycle {
	nonisolated static func requireIdle(activeTransfers: UInt64, activeShares: UInt64) throws {
		guard activeTransfers == 0, activeShares == 0 else {
			throw CoreNetworkLifecycleError.activeNetworkWork
		}
	}
}

protocol CoreBindingFactory: Sendable {
	func initialize(
		appDataDir: String,
		eventSink: CoreEventSink,
		networkConfiguration: RelayConfiguration
	) throws -> VnidropCore
	func resetUnrecoverableIdentity(
		appDataDir: String,
		eventSink: CoreEventSink,
		networkConfiguration: RelayConfiguration
	) throws -> VnidropCore
}

struct NativeCoreBindingFactory: CoreBindingFactory {
	private func nativeConfiguration(_ networkConfiguration: RelayConfiguration) -> CoreNetworkConfig {
		switch networkConfiguration.mode {
		case .automatic:
			return defaultCoreNetworkConfig()
		case .strictCustom:
			return CoreNetworkConfig(mode: .strictCustom, relayUrls: networkConfiguration.relayURLs)
		case .customWithDirectFallback:
			return CoreNetworkConfig(mode: .customWithDirectFallback, relayUrls: networkConfiguration.relayURLs)
		case .localOnly:
			return CoreNetworkConfig(mode: .localOnly, relayUrls: [])
		}
	}

	func initialize(
		appDataDir: String,
		eventSink: CoreEventSink,
		networkConfiguration: RelayConfiguration
	) throws -> VnidropCore {
		return try VnidropCore.initializeWithLimitsAndNetworkConfig(
			appDataDir: appDataDir,
			eventSink: eventSink,
			limits: defaultCoreLimits(),
			networkConfig: nativeConfiguration(networkConfiguration)
		)
	}

	func resetUnrecoverableIdentity(
		appDataDir: String,
		eventSink: CoreEventSink,
		networkConfiguration: RelayConfiguration
	) throws -> VnidropCore {
		try VnidropCore.resetUnrecoverableIdentityWithLimitsAndNetworkConfig(
			appDataDir: appDataDir,
			eventSink: eventSink,
			limits: defaultCoreLimits(),
			networkConfig: nativeConfiguration(networkConfiguration)
		)
	}
}

/// Swift port of `core/CoreRepository.kt`. Owns the `VnidropCore` handle, maps the
/// generated UniFFI records into app domain models, publishes an observable
/// `CoreState`, and emits coalesced `CoreSignal`s from the event sink.
///
/// UniFFI calls block (the core drives its own runtime via `block_on`), so they
/// run on a background queue and results are hopped back to the main actor.
@MainActor
final class CoreRepository: ObservableObject, CoreGateway {
	@Published private(set) var state = CoreState()
	var statePublisher: AnyPublisher<CoreState, Never> { $state.eraseToAnyPublisher() }

	private let signalsSubject = PassthroughSubject<CoreSignal, Never>()
	/// Coalesced change hints; subscribe to react to approval/history/transfer changes.
	var signals: AnyPublisher<CoreSignal, Never> { signalsSubject.eraseToAnyPublisher() }

	// Initialization swaps happen on `queue`; shutdown and snapshot reads may also
	// access the handle from the main actor. The underlying core is internally
	// synchronized, and `nonisolated(unsafe)` documents that crossing for Swift 6.
	private nonisolated(unsafe) var core: VnidropCore?
	// Core calls run through `dispatcher` (see runCore/runInterrupt); the factory
	// and transition flag drive relay-aware (re)initialization.
	private let dispatcher = CoreDispatcher()
	private let coreFactory: any CoreBindingFactory
	private var isNetworkTransitionInProgress = false
	private lazy var sink = RepositoryEventSink { [weak self] event in
		Task { @MainActor in self?.handle(event: event) }
	}

	private nonisolated static let maxEvents = 200

	init(coreFactory: any CoreBindingFactory = NativeCoreBindingFactory()) {
		self.coreFactory = coreFactory
	}

	// MARK: - Lifecycle

	func initialize(
		appDataDir: String,
		networkConfiguration: RelayConfiguration
	) async -> Result<Void, Error> {
		guard !isNetworkTransitionInProgress else {
			return .failure(CoreNetworkLifecycleError.transitionInProgress)
		}
		isNetworkTransitionInProgress = true
		defer { isNetworkTransitionInProgress = false }

		let result = await runCore { [sink] in
			if let existing = self.core {
				let status = existing.status()
				try CoreNetworkLifecycle.requireIdle(
					activeTransfers: status.activeTransfers,
					activeShares: status.activeShares
				)
				existing.shutdown()
				self.core = nil
			}
			let created = try self.coreFactory.initialize(
				appDataDir: appDataDir,
				eventSink: sink,
				networkConfiguration: networkConfiguration
			)
			self.core = created
			return created
		}
		switch result {
		case .success:
			self.refreshSnapshot()
			self.state.isInitialized = true
			return .success(())
		case .failure(let error):
			if error as? CoreNetworkLifecycleError != .activeNetworkWork {
				self.state = CoreState()
			}
			return .failure(error)
		}
	}

	func resetUnrecoverableIdentity(
		appDataDir: String,
		networkConfiguration: RelayConfiguration
	) async -> Result<Void, Error> {
		guard !isNetworkTransitionInProgress else {
			return .failure(CoreNetworkLifecycleError.transitionInProgress)
		}
		isNetworkTransitionInProgress = true
		defer { isNetworkTransitionInProgress = false }
		let result = await runCore { [sink] in
			let created = try self.coreFactory.resetUnrecoverableIdentity(
				appDataDir: appDataDir,
				eventSink: sink,
				networkConfiguration: networkConfiguration
			)
			self.core = created
			return created
		}
		switch result {
		case .success:
			refreshSnapshot()
			state.isInitialized = true
			return .success(())
		case .failure(let error):
			state = CoreState()
			return .failure(error)
		}
	}

	func shutdown() {
		core?.shutdown()
		core = nil
		state = CoreState()
	}

	// MARK: - Share

	func shareSources(
		_ sources: [ShareSource],
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy
	) async -> Result<Share, Error> {
		guard !isNetworkTransitionInProgress else {
			return .failure(CoreNetworkLifecycleError.transitionInProgress)
		}
		guard !sources.isEmpty else {
			return .failure(InvitationError.shareEmpty)
		}
		return await runCore {
			let result = try self.requireCore().shareFiles(
				sources: sources,
				metadata: ShareMetadataInput(
					transferId: Self.nextTransferId(),
					transferName: transferName.isEmpty ? nil : transferName,
					senderName: senderName.isEmpty ? nil : senderName,
					accessMode: accessPolicy.toNative()
				)
			)
			return result.toModel()
		}.map { share in
			self.refreshSnapshot()
			self.state.lastShare = share
			return share
		}
	}

	// MARK: - Inspect / Receive

	func inspectTicket(_ ticket: String) async -> Result<TicketInspectionModel, Error> {
		await runCore {
			try self.requireCore().inspectTicket(ticket: ticket).toModel()
		}.map { inspection in
			self.state.lastInspection = inspection
			return inspection
		}
	}

	func receive(ticket: String, outputDir: String, receiverName: String) async -> Result<Void, Error> {
		guard !isNetworkTransitionInProgress else {
			return .failure(CoreNetworkLifecycleError.transitionInProgress)
		}
		return await runCore {
			try self.requireCore().receive(
				ticket: ticket,
				outputDir: outputDir,
				receiverName: receiverName.isEmpty ? nil : receiverName
			)
		}.map { self.refreshSnapshot() }
	}

	/// Receive into a security-scoped directory URL, holding access while the core
	/// streams (mirrors `receiveIntoSecurityScopedDirectory`).
	func receiveIntoSecurityScopedDirectory(
		ticket: String,
		outputDirectoryUrl: String,
		receiverName: String
	) async -> Result<Void, Error> {
		guard !isNetworkTransitionInProgress else {
			return .failure(CoreNetworkLifecycleError.transitionInProgress)
		}
		return await runCore {
			try withSecurityScopedAccess(pathOrUrl: outputDirectoryUrl) {
				try self.requireCore().receive(
					ticket: ticket,
					outputDir: outputDirectoryUrl,
					receiverName: receiverName.isEmpty ? nil : receiverName
				)
			}
		}.map { self.refreshSnapshot() }
	}

	// MARK: - Lifecycle actions

	func cancel(transferId: UInt64) async -> Result<Void, Error> {
		// Off the serial `queue`: a receive in flight is blocking it, and the
		// cancel signal must reach the core to unblock that receive.
		await runInterrupt {
			try self.requireCore().cancelTransfer(transferId: transferId)
		}.map { self.refreshSnapshot() }
	}

	func delete(transferId: UInt64) async -> Result<Void, Error> {
		await runCore {
			try self.requireCore().deleteTransfer(transferId: transferId)
		}.map {
			self.refreshSnapshot()
			self.signalsSubject.send(.approvalChanged(transferId: transferId))
			self.signalsSubject.send(.receiverHistoryChanged(transferId: transferId))
		}
	}

	func clearReceiveHistory() async -> Result<UInt64, Error> {
		await runCore {
			try self.requireCore().deleteReceiveHistory()
		}.map { deleted in
			self.refreshSnapshot()
			return deleted
		}
	}

	func storageUsage() async -> Result<CoreStorageUsageModel, Error> {
		await runCore {
			let usage = try self.requireCore().storageUsage()
			return CoreStorageUsageModel(
				blobStoreBytes: usage.blobStoreBytes,
				appDataBytes: usage.databaseBytes + usage.logsBytes + usage.previewsBytes + usage.otherCoreBytes
			)
		}
	}

	func receivedArtifacts() async -> Result<[ReceivedArtifactModel], Error> {
		await runCore {
			try self.requireCore().listReceivedArtifacts().compactMap { artifact in
				guard artifact.locatorKind == .filesystemPath else { return nil }
				return ReceivedArtifactModel(locator: artifact.locator, logicalSize: artifact.logicalSize)
			}
		}
	}

	func receiverRequests(transferId: UInt64) async -> Result<[ReceiverRequestModel], Error> {
		await runCore {
			try self.requireCore().listReceiverRequests(transferId: transferId).map { $0.toModel() }
		}
	}

	func respondReceiverRequest(
		requestId: String,
		accepted: Bool,
		reason: String? = nil
	) async -> Result<Void, Error> {
		await runCore {
			try self.requireCore().respondReceiverRequest(requestId: requestId, accepted: accepted, reason: reason)
		}
	}

	func refresh() async -> Result<Void, Error> {
		// Read from the core off the main actor, then apply the snapshot on the
		// main actor so `@Published state` is never mutated from `queue`.
		await runCore { self.readSnapshot() }.map { snapshot in
			if let snapshot { self.applySnapshot(snapshot) }
		}
	}
	// MARK: - Event sink handling (ported from CoreRepository.sink)

	private func handle(event: CoreEvent) {
		let model = event.toModel()
		var events = state.events
		events.insert(model, at: 0)
		if events.count > Self.maxEvents { events = Array(events.prefix(Self.maxEvents)) }
		state.events = events

		guard let transferId = model.transferId else { return }
		switch model.phase {
		case "approval", "access": signalsSubject.send(.approvalChanged(transferId: transferId))
		case "delivery": signalsSubject.send(.receiverHistoryChanged(transferId: transferId))
		default: break
		}
		if model.shouldRefreshTransfers {
			signalsSubject.send(.transfersChanged(transferId: transferId))
		}
	}

	// MARK: - Internals

	/// Snapshot of the values read from the core in one pass.
	private struct CoreSnapshot: Sendable {
		let status: CoreStatus
		let transfers: [Transfer]
		let events: [CoreEventModel]
	}

	/// Reads the current core state. Safe to call off the main actor (pure core
	/// FFI reads); does not touch `@Published` state.
	private nonisolated func readSnapshot() -> CoreSnapshot? {
		guard let core = self.core else { return nil }
		let status = core.status()
		let transfers = (try? core.listTransfers())?.map { $0.toModel() } ?? []
		let events = (try? core.listEvents(transferId: nil))?.prefix(Self.maxEvents).map { $0.toModel() } ?? []
		return CoreSnapshot(
			status: CoreStatus(
				endpointId: status.endpointId,
				activeTransfers: status.activeTransfers,
				activeShares: status.activeShares
			),
			transfers: transfers,
			events: Array(events)
		)
	}

	/// Applies a snapshot to `@Published state`. Must run on the main actor.
	private func applySnapshot(_ snapshot: CoreSnapshot) {
		state.status = snapshot.status
		state.transfers = snapshot.transfers
		state.events = snapshot.events
	}

	private func refreshSnapshot() {
		if let snapshot = readSnapshot() { applySnapshot(snapshot) }
	}

	private nonisolated func requireCore() throws -> VnidropCore {
		guard let core = self.core else {
			throw InvitationError.coreNotInitialized
		}
		return core
	}

	/// Runs a blocking core call off the main actor and hops the result back.
	private nonisolated func runCore<T: Sendable>(_ block: @escaping @Sendable () throws -> T) async -> Result<T, Error> {
		await dispatcher.run(block)
	}

	/// Like `runCore`, but off the serial lane so it can interrupt a blocking
	/// call in flight there (e.g. cancel a `receive`). Only use for core calls
	/// that are safe to run concurrently with another core call.
	private nonisolated func runInterrupt<T: Sendable>(_ block: @escaping @Sendable () throws -> T) async -> Result<T, Error> {
		await dispatcher.runInterrupt(block)
	}

	private nonisolated static func nextTransferId() -> UInt64 {
		UInt64.random(in: 1...UInt64(Int64.max))
	}
}

/// Event sink bridged to the repository. `onEvent` is invoked on core-owned
/// threads; the handler hops to the main actor.
private final class RepositoryEventSink: CoreEventSink, @unchecked Sendable {
	private let handler: @Sendable (CoreEvent) -> Void
	init(handler: @escaping @Sendable (CoreEvent) -> Void) { self.handler = handler }
	func onEvent(event: CoreEvent) { handler(event) }
}

/// Runs `body` while holding security-scoped access to a bookmarked URL/path.
private func withSecurityScopedAccess<T>(pathOrUrl: String, _ body: () throws -> T) throws -> T {
	let url = URL(string: pathOrUrl) ?? URL(fileURLWithPath: pathOrUrl)
	let started = url.startAccessingSecurityScopedResource()
	defer { if started { url.stopAccessingSecurityScopedResource() } }
	return try body()
}

// MARK: - Mapping (ported from CoreRepository.kt)

private extension CoreEvent {
	func toModel() -> CoreEventModel {
		CoreEventModel(
			id: id, revision: revision, timestamp: timestamp, scope: scope, transferId: transferId,
			direction: direction, phase: phase, kind: kind, dataJson: dataJson
		)
	}
}

private let refreshPhases: Set<EventPhase> = [.lifecycle, .error, .ticket, .importing, .download, .export, .handshake]
private let refreshKinds: Set<EventKind> = [
	.started, .done, .created, .failed, .cancelled, .shareStopped, .foundCollection, .connected,
]

private extension CoreEventModel {
	var shouldRefreshTransfers: Bool {
		guard let eventPhase, let eventKind else { return false }
		return refreshPhases.contains(eventPhase) && refreshKinds.contains(eventKind)
	}
}

extension ShareAccessPolicy {
	func toNative() -> TransferAccessMode {
		switch self {
		case .requireApproval: return .approvalRequired
		case .anyoneWithTransfer: return .public
		}
	}
}

private extension TransferAccessMode {
	func toModel() -> ShareAccessPolicy {
		switch self {
		case .approvalRequired: return .requireApproval
		case .public: return .anyoneWithTransfer
		}
	}
}

private extension StoredTransfer {
	func toModel() -> Transfer {
		Transfer(
			localId: localId,
			transferId: transferId,
			direction: Self.direction(direction),
			status: Self.status(status),
			peerId: peerId,
			transferName: transferName,
			contentHash: contentHash,
			fileCount: fileCount,
			totalSize: totalSize,
			ticket: ticket,
			accessPolicy: accessMode.toModel(),
			createdAt: createdAt,
			updatedAt: updatedAt
		)
	}

	static func direction(_ raw: String) -> TransferDirection {
		switch raw {
		case "send": return .send
		case "receive": return .receive
		default: return .send
		}
	}

	static func status(_ raw: String) -> TransferStatus {
		switch raw {
		case "importing": return .importing
		case "sharing": return .sharing
		case "receiving": return .receiving
		case "done": return .done
		case "failed": return .failed
		case "cancelled": return .cancelled
		case "stopped": return .stopped
		default: return .failed
		}
	}
}

private extension ShareResult {
	func toModel() -> Share {
		Share(
			transferId: transferId, ticket: ticket, transferName: transferName,
			contentHash: hash, fileCount: fileCount, totalSize: totalSize
		)
	}
}

private extension TicketInspection {
	func toModel() -> TicketInspectionModel {
		TicketInspectionModel(kind: kind, metadata: metadata.toModel())
	}
}

private extension TransferMetadata {
	func toModel() -> TransferMetadataModel {
		TransferMetadataModel(
			transferId: transferId, transferName: transferName, senderName: senderName,
			contentHash: contentHash, fileCount: fileCount, totalSize: totalSize
		)
	}
}

private extension ReceiverRequest {
	func toModel() -> ReceiverRequestModel {
		ReceiverRequestModel(
			id: id, transferId: transferId, remoteEndpointId: remoteEndpointId,
			transferName: transferName, receiverName: receiverName, receiverDeviceName: receiverDeviceName,
			appVersion: appVersion, status: Self.status(status), reason: reason,
			requestedAt: requestedAt, respondedAt: respondedAt, completedAt: completedAt
		)
	}

	static func status(_ raw: String) -> ReceiverDeliveryStatus {
		switch raw {
		case "requested": return .requested
		case "accepted": return .accepted
		case "refused": return .refused
		case "expired": return .expired
		case "completed": return .completed
		case "failed": return .failed
		default: return .unknown
		}
	}
}



