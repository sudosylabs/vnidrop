import Foundation
import Combine
import VnidropCore
@testable import VniDrop

enum TestError: Error { case unimplemented }

/// In-memory `CoreGateway`, mirroring `support/Fakes.kt`'s `FakeCoreGateway`.
/// Lets model tests drive core state/signals and stub results without the FFI.
@MainActor
final class FakeCoreGateway: CoreGateway {
	private let stateSubject = CurrentValueSubject<CoreState, Never>(CoreState())
	private let signalsSubject = PassthroughSubject<CoreSignal, Never>()

	var state: CoreState { stateSubject.value }
	var statePublisher: AnyPublisher<CoreState, Never> { stateSubject.eraseToAnyPublisher() }
	var signals: AnyPublisher<CoreSignal, Never> { signalsSubject.eraseToAnyPublisher() }

	// Stubbed results
	var requests: [UInt64: [ReceiverRequestModel]] = [:]
	var responseResult: Result<Void, Error> = .success(())
	var shareResult: Result<Share, Error> = .failure(TestError.unimplemented)
	var inspectionResult: Result<TicketInspectionModel, Error> = .failure(TestError.unimplemented)
	var receiveResult: Result<Void, Error> = .success(())
	var cancelResult: Result<Void, Error> = .success(())
	var deleteResult: Result<Void, Error> = .success(())
	var clearReceiveHistoryResult: Result<UInt64, Error> = .success(0)
	var initializeResult: Result<Void, Error> = .success(())
	var initializeResults: [Result<Void, Error>] = []
	var resetUnrecoverableIdentityResult: Result<Void, Error> = .success(())

	// Recorded calls
	private(set) var responses: [(id: String, accepted: Bool, reason: String?)] = []
	private(set) var deletedTransfers: [UInt64] = []
	private(set) var cancelledTransfers: [UInt64] = []
	private(set) var clearReceiveHistoryCount = 0
	private(set) var receiveCount = 0
	private(set) var lastReceiveTicket: String?
	private(set) var lastReceiveReceiverName: String?
	private(set) var lastShareAccessPolicy: ShareAccessPolicy?
	private(set) var initializedNetworkConfigurations: [RelayConfiguration] = []
	private(set) var resetUnrecoverableIdentityCount = 0

	func setState(_ state: CoreState) { stateSubject.send(state) }

	/// Publishes the `created` lifecycle event the core emits once a targeted
	/// transfer row exists, then the signal that follows it — the sequence that
	/// lets a caller cancel a create still holding the serial lane.
	func emitTargetedTransferCreated(id: String) {
		var state = stateSubject.value
		state.events.insert(
			CoreEventModel(
				id: "event-\(id)", revision: 1, timestamp: 1, scope: "endpoint", transferId: nil,
				direction: nil, phase: EventPhase.targetedTransfer.rawValue,
				kind: EventKind.created.rawValue,
				dataJson: #"{"targeted_transfer_id":"\#(id)"}"#
			),
			at: 0
		)
		stateSubject.send(state)
		emit(.targetedTransferChanged)
	}
	func emit(_ signal: CoreSignal) { signalsSubject.send(signal) }

	func initialize(
		appDataDir: String,
		networkConfiguration: RelayConfiguration
	) async -> Result<Void, Error> {
		initializedNetworkConfigurations.append(networkConfiguration)
		let result = initializeResults.isEmpty ? initializeResult : initializeResults.removeFirst()
		guard case .success = result else { return result }
		var s = stateSubject.value
		s.isInitialized = true
		stateSubject.send(s)
		return .success(())
	}
	func resetUnrecoverableIdentity(
		appDataDir: String,
		networkConfiguration: RelayConfiguration
	) async -> Result<Void, Error> {
		resetUnrecoverableIdentityCount += 1
		guard case .success = resetUnrecoverableIdentityResult else {
			return resetUnrecoverableIdentityResult
		}
		var s = stateSubject.value
		s.isInitialized = true
		stateSubject.send(s)
		return .success(())
	}
	func shutdown() {}
	func shareSources(_ sources: [ShareSource], transferName: String, senderName: String, accessPolicy: ShareAccessPolicy) async -> Result<Share, Error> {
		lastShareAccessPolicy = accessPolicy
		return shareResult
	}
	func inspectTicket(_ ticket: String) async -> Result<TicketInspectionModel, Error> { inspectionResult }
	func receive(ticket: String, outputDir: String, receiverName: String) async -> Result<Void, Error> {
		receiveCount += 1; lastReceiveTicket = ticket; lastReceiveReceiverName = receiverName
		return receiveResult
	}
	func receiveIntoSecurityScopedDirectory(ticket: String, outputDirectoryUrl: String, receiverName: String) async -> Result<Void, Error> {
		receiveCount += 1; lastReceiveTicket = ticket; lastReceiveReceiverName = receiverName
		return receiveResult
	}
	func cancel(transferId: UInt64) async -> Result<Void, Error> { cancelledTransfers.append(transferId); return cancelResult }
	func delete(transferId: UInt64) async -> Result<Void, Error> { deletedTransfers.append(transferId); return deleteResult }
	func clearReceiveHistory() async -> Result<UInt64, Error> { clearReceiveHistoryCount += 1; return clearReceiveHistoryResult }
	func storageUsage() async -> Result<CoreStorageUsageModel, Error> {
		.success(CoreStorageUsageModel(blobStoreBytes: 0, appDataBytes: 0))
	}
	func receivedArtifacts() async -> Result<[ReceivedArtifactModel], Error> { .success([]) }
	func receiverRequests(transferId: UInt64) async -> Result<[ReceiverRequestModel], Error> { .success(requests[transferId] ?? []) }
	func respondReceiverRequest(requestId: String, accepted: Bool, reason: String?) async -> Result<Void, Error> {
		responses.append((requestId, accepted, reason))
		return responseResult
	}
	func refresh() async -> Result<Void, Error> { .success(()) }

	// MARK: - Saved devices

	// Stubbed results
	var savedDevices: [SavedDeviceModel] = []
	/// Overrides `savedDevices` when set, so a test can fail one leg of the
	/// five-read snapshot without stubbing the rest.
	var savedDevicesResult: Result<[SavedDeviceModel], Error>?
	var deviceRelationships: [DeviceRelationshipModel] = []
	var pairingEligibilities: [PairingEligibilityModel] = []
	var blockedDevices: [String] = []
	var pendingTargetedOffers: [PendingTargetedOfferModel] = []
	var targetedTransfers: [TargetedTransferModel] = []
	var setLabelResult: Result<Void, Error> = .success(())
	var forgetResult: Result<Void, Error> = .success(())
	var blockResult: Result<Void, Error> = .success(())
	var requestPairingResult: Result<Bool, Error> = .success(true)
	var respondToPairingResult: Result<Bool, Error> = .success(true)
	var offerResponseResult: Result<TargetedOfferResponseModel, Error> = .success(.declined)
	var createTargetedTransferResult: Result<TargetedTransferModel, Error> = .failure(TestError.unimplemented)
	var targetedReceiveResult: Result<Void, Error> = .success(())
	var targetedCancelResult: Result<Void, Error> = .success(())
	var targetedDeleteResult: Result<Void, Error> = .success(())

	// Recorded calls
	private(set) var setLabels: [(peerEndpointId: String, label: String?)] = []
	private(set) var forgottenDevices: [String] = []
	private(set) var blockedCalls: [String] = []
	private(set) var unblockedCalls: [String] = []
	private(set) var declinedEligibilities: [String] = []
	private(set) var requestedPairings: [String] = []
	private(set) var pairingResponses: [(peerEndpointId: String, accepted: Bool)] = []
	private(set) var offerResponses: [(transferId: String, accepted: Bool)] = []
	private(set) var createdTargetedTransfers: [(receiverEndpointId: String, sources: [ShareSource], transferName: String?)] = []
	private(set) var targetedReceives: [(transferId: String, outputDirectoryUrl: String)] = []
	private(set) var targetedResumes: [(id: String, outputDirectoryUrl: String)] = []
	private(set) var cancelledTargetedTransfers: [String] = []
	private(set) var deletedTargetedTransfers: [String] = []

	func listPairingEligibilities() async -> Result<[PairingEligibilityModel], Error> {
		.success(pairingEligibilities)
	}
	func declinePairingEligibility(peerEndpointId: String) async -> Result<Void, Error> {
		declinedEligibilities.append(peerEndpointId)
		return .success(())
	}
	func requestSavedDevicePairing(peerEndpointId: String) async -> Result<Bool, Error> {
		requestedPairings.append(peerEndpointId)
		return requestPairingResult
	}
	func respondToDevicePairing(peerEndpointId: String, accepted: Bool) async -> Result<Bool, Error> {
		pairingResponses.append((peerEndpointId, accepted))
		return respondToPairingResult
	}
	func listDeviceRelationships() async -> Result<[DeviceRelationshipModel], Error> {
		.success(deviceRelationships)
	}
	func listSavedDevices() async -> Result<[SavedDeviceModel], Error> {
		savedDevicesResult ?? .success(savedDevices)
	}
	func setSavedDeviceLabel(peerEndpointId: String, label: String?) async -> Result<Void, Error> {
		setLabels.append((peerEndpointId, label))
		return setLabelResult
	}
	func forgetSavedDevice(peerEndpointId: String) async -> Result<Void, Error> {
		forgottenDevices.append(peerEndpointId)
		return forgetResult
	}
	func blockDevice(peerEndpointId: String) async -> Result<Void, Error> {
		blockedCalls.append(peerEndpointId)
		return blockResult
	}
	func unblockDevice(peerEndpointId: String) async -> Result<Void, Error> {
		unblockedCalls.append(peerEndpointId)
		return .success(())
	}
	func listBlockedDevices() async -> Result<[String], Error> { .success(blockedDevices) }

	// MARK: - Targeted transfers

	func listPendingTargetedOffers() async -> Result<[PendingTargetedOfferModel], Error> {
		.success(pendingTargetedOffers)
	}
	func respondToTargetedOffer(
		transferId: String, accepted: Bool
	) async -> Result<TargetedOfferResponseModel, Error> {
		offerResponses.append((transferId, accepted))
		return offerResponseResult
	}
	/// Holds `createTargetedTransfer` open until `releaseTargetedCreate()`, so a
	/// test can observe the model while a create is genuinely in flight — the
	/// state the user is stuck in when the receiving device never answers.
	var holdsTargetedCreate = false
	private var targetedCreateGate: CheckedContinuation<Void, Never>?

	/// True once the call is parked on the gate. `isCreatingSend` flips before the
	/// task body runs, so releasing on that alone can resume nothing and hang.
	var isHoldingTargetedCreate: Bool { targetedCreateGate != nil }

	func releaseTargetedCreate() {
		let gate = targetedCreateGate
		targetedCreateGate = nil
		gate?.resume()
	}

	func createTargetedTransfer(
		receiverEndpointId: String, sources: [ShareSource], transferName: String?
	) async -> Result<TargetedTransferModel, Error> {
		createdTargetedTransfers.append((receiverEndpointId, sources, transferName))
		if holdsTargetedCreate {
			await withCheckedContinuation { targetedCreateGate = $0 }
		}
		return createTargetedTransferResult
	}
	func listTargetedTransfers() async -> Result<[TargetedTransferModel], Error> {
		.success(targetedTransfers)
	}
	func receiveTargetedTransfer(
		transferId: String, outputDirectoryUrl: String
	) async -> Result<Void, Error> {
		targetedReceives.append((transferId, outputDirectoryUrl))
		return targetedReceiveResult
	}
	func resumeTargetedTransfer(id: String, outputDirectoryUrl: String) async -> Result<Void, Error> {
		targetedResumes.append((id, outputDirectoryUrl))
		return targetedReceiveResult
	}
	func cancelTargetedTransfer(id: String) async -> Result<Void, Error> {
		cancelledTargetedTransfers.append(id)
		return targetedCancelResult
	}
	func deleteTargetedTransfer(id: String) async -> Result<Void, Error> {
		deletedTargetedTransfers.append(id)
		return targetedDeleteResult
	}
}

/// Minimal `FileSystemService` fake — a writable path receive folder, no reveal.
@MainActor
final class FakeFileSystemService: FileSystemService {
	var supportsCustomReceiveFolders = false
	var folder = ReceiveFolder(kind: .fileSystemPath, value: "/tmp/vnidrop-tests", displayName: "Documents")

	func defaultReceiveFolder() -> ReceiveFolder { folder }
	func validateReceiveFolder(_ folder: ReceiveFolder) async -> FolderAccessStatus { .writable }
	func canRevealReceiveFolder(_ folder: ReceiveFolder) -> Bool { false }
	private(set) var shareDestinations: [ShareDestination] = []

	func sharePickedFiles(repository: CoreGateway, files: [PickedShareFile], transferName: String, senderName: String, destination: ShareDestination) async -> Result<Share, Error> {
		shareDestinations.append(destination)
		guard case .invitation(let accessPolicy) = destination else {
			return .failure(TestError.unimplemented)
		}
		return await repository.shareSources(
			[], transferName: transferName, senderName: senderName, accessPolicy: accessPolicy
		)
	}

	/// Records targeted sends and forwards to the gateway so its recorders and
	/// stubbed result drive the assertions.
	private(set) var targetedSends: [(files: [PickedShareFile], transferName: String, receiver: String)] = []

	func sendPickedFilesToSavedDevice(
		repository: CoreGateway,
		files: [PickedShareFile],
		transferName: String,
		receiverEndpointId: String
	) async -> Result<TargetedTransferModel, Error> {
		targetedSends.append((files, transferName, receiverEndpointId))
		return await repository.createTargetedTransfer(
			receiverEndpointId: receiverEndpointId,
			sources: [],
			transferName: transferName.isEmpty ? nil : transferName
		)
	}

	/// Picker copies released via `discardPickedFiles`.
	private(set) var discardedFiles: [String] = []

	func discardPickedFiles(_ files: [PickedShareFile]) async {
		discardedFiles.append(contentsOf: files.map(\.value))
	}
}

@MainActor
final class FakeDeviceInfoProvider: DeviceInfoProvider {
	func load() async -> DeviceInfo {
		DeviceInfo(deviceName: "Test Device", deviceModel: "TestModel",
				   operatingSystem: "TestOS 1.0", network: nil, batteryLevel: nil)
	}
}

// MARK: - Factories

@MainActor
enum Fixtures {
	static func preferences(username: String = "Tester") -> AppPreferencesRepository {
		let defaults = UserDefaults(suiteName: "vnidrop.tests.\(UUID().uuidString)")!
		return AppPreferencesRepository(
			defaults: defaults,
			fallback: AppPreferencesDefaults(
				username: username,
				receiveFolder: ReceiveFolder(kind: .fileSystemPath, value: "/tmp/vnidrop-tests", displayName: "Documents"),
				themeMode: .system
			)
		)
	}

	static func request(id: String, requestedAt: Int64, transferId: UInt64 = 1, status: ReceiverDeliveryStatus = .requested) -> ReceiverRequestModel {
		ReceiverRequestModel(
			id: id, transferId: transferId, remoteEndpointId: "endpoint-\(id)",
			transferName: "Photos", receiverName: "Peer", receiverDeviceName: "Phone",
			appVersion: "1.0", status: status, reason: nil,
			requestedAt: requestedAt, respondedAt: nil, completedAt: nil
		)
	}

	static func transfer(id: UInt64, direction: TransferDirection, status: TransferStatus) -> Transfer {
		Transfer(
			localId: "local-\(id)", transferId: id, direction: direction, status: status,
			peerId: nil, transferName: "Photos", contentHash: nil, fileCount: 1, totalSize: 1024,
			ticket: "ticket", accessPolicy: .requireApproval, createdAt: 0, updatedAt: 0
		)
	}
}

/// Polls `condition` on the main actor until true or `timeout` elapses. Used to
/// await the models' internal `Task`s, which XCTest can't join directly.
@MainActor
func waitUntil(timeout: TimeInterval = 2, _ condition: @escaping () -> Bool) async {
	let deadline = Date().addingTimeInterval(timeout)
	while !condition() && Date() < deadline {
		try? await Task.sleep(nanoseconds: 5_000_000)
	}
}
