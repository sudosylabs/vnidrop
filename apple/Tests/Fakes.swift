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

	func setState(_ state: CoreState) { stateSubject.send(state) }
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
}

/// Minimal `FileSystemService` fake — a writable path receive folder, no reveal.
@MainActor
final class FakeFileSystemService: FileSystemService {
	var supportsCustomReceiveFolders = false
	var folder = ReceiveFolder(kind: .fileSystemPath, value: "/tmp/vnidrop-tests", displayName: "Documents")

	func defaultReceiveFolder() -> ReceiveFolder { folder }
	func validateReceiveFolder(_ folder: ReceiveFolder) async -> FolderAccessStatus { .writable }
	func canRevealReceiveFolder(_ folder: ReceiveFolder) -> Bool { false }
	func sharePickedFiles(repository: CoreGateway, files: [PickedShareFile], transferName: String, senderName: String, accessPolicy: ShareAccessPolicy) async -> Result<Share, Error> {
		await repository.shareSources([], transferName: transferName, senderName: senderName, accessPolicy: accessPolicy)
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
