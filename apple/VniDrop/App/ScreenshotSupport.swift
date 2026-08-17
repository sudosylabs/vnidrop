#if DEBUG
import Foundation
import Combine
import VnidropCore
#if os(macOS)
import AppKit
#endif

/// macOS makes a sheet's first text field the initial first responder and selects its
/// contents, which reads as a live edit (blue selection) in a marketing capture.
/// Nothing in the product wants that changed — only the screenshot does.
@MainActor
func clearScreenshotFocus() {
	#if os(macOS)
	Task { @MainActor in
		// The sheet is presented asynchronously and takes first responder once it
		// becomes key, so a single early clear is lost. Sweep every window a few
		// times across the presentation window instead of guessing one delay.
		for _ in 0..<12 {
			try? await Task.sleep(for: .milliseconds(250))
			for window in NSApp.windows { window.makeFirstResponder(nil) }
		}
	}
	#endif
}

/// Which marketing screen to stage. Selected via the `-VniScreenshot <name>` launch
/// argument (read from `NSArgumentDomain`), set by the App Store screenshot UI test.
enum ScreenshotScenario: String {
	case transferDetails = "transfer-details" // Send Anywhere: the detail view
	case share                                 // Share Securely: the QR / share panel
	case approval                              // Choose Receivers: the receive-request modal
	case compose                               // Web hero (Mac): the transfer composer sheet
	case receiveConnect = "receive-connect"    // Web hero (iPhone): Receive + connect sheet

	/// The active scenario for this launch, or `nil` in a normal run.
	static var current: ScreenshotScenario? {
		guard let raw = UserDefaults.standard.string(forKey: "VniScreenshot") else { return nil }
		return ScreenshotScenario(rawValue: raw)
	}
}

/// A fixture `CoreGateway` that publishes deterministic content instead of driving
/// the Rust core, so App Store screenshots are stable and localized. Only the reads
/// the screenshot screens need are meaningful; mutations are inert.
@MainActor
final class ScreenshotCoreGateway: CoreGateway {
	private let scenario: ScreenshotScenario
	private let subject: CurrentValueSubject<CoreState, Never>
	private let signalSubject = PassthroughSubject<CoreSignal, Never>()

	/// Deterministic fixture transfer shown across every scenario.
	static let transferId: UInt64 = 1

	/// Staged file for the `compose` scenario. Never touches the filesystem — the
	/// composer only reads `displayName`/`sizeBytes` to render its summary row.
	static let composerFile = PickedShareFile(
		value: "/screenshot/Purple Rain.mp3",
		displayName: "Purple Rain.mp3",
		sizeBytes: 8_912_896 // 8.5 MiB — the size formatter renders binary units
	)
	private let fixtureTransfer = Transfer(
		localId: "screenshot-1",
		transferId: ScreenshotCoreGateway.transferId,
		direction: .send,
		status: .sharing,
		peerId: nil,
		transferName: "Transfer.MOV",
		contentHash: "b1946ac92492d2347c6235b4d2611184",
		fileCount: 1,
		totalSize: 9_100_000,
		ticket: "vnd://screenshot-demo-ticket-abcdefghijklmnopqrstuvwxyz0123456789",
		accessPolicy: .requireApproval,
		createdAt: 1_722_000_000,
		updatedAt: 1_722_000_000
	)

	init(scenario: ScreenshotScenario) {
		self.scenario = scenario
		self.subject = CurrentValueSubject(CoreState())
	}

	var state: CoreState { subject.value }
	var statePublisher: AnyPublisher<CoreState, Never> { subject.eraseToAnyPublisher() }
	var signals: AnyPublisher<CoreSignal, Never> { signalSubject.eraseToAnyPublisher() }

	func initialize(appDataDir: String, networkConfiguration: RelayConfiguration) async -> Result<Void, Error> {
		subject.value = CoreState(
			isInitialized: true,
			status: CoreStatus(endpointId: "screenshot-endpoint", activeTransfers: 1, activeShares: 1),
			events: [],
			transfers: [fixtureTransfer],
			lastShare: nil,
			lastInspection: nil
		)
		// Nudge the approval coordinator to (re)read requests for the sharing transfer.
		signalSubject.send(.approvalChanged(transferId: Self.transferId))
		return .success(())
	}

	func receiverRequests(transferId: UInt64) async -> Result<[ReceiverRequestModel], Error> {
		guard scenario == .approval else { return .success([]) }
		return .success([
			ReceiverRequestModel(
				id: "screenshot-request-1",
				transferId: Self.transferId,
				remoteEndpointId: "k51qzi5uqu5d-screenshot-peer-endpoint-identity",
				transferName: "Transfer.MOV",
				receiverName: nil,
				receiverDeviceName: "Mac mini",
				appVersion: "1.0",
				status: .requested,
				reason: nil,
				requestedAt: 1_722_000_000,
				respondedAt: nil,
				completedAt: nil
			)
		])
	}

	// MARK: - Inert mutations (screenshots never exercise these)

	func shutdown() {}
	func shareSources(_ sources: [ShareSource], transferName: String, senderName: String, accessPolicy: ShareAccessPolicy) async -> Result<Share, Error> {
		.failure(ScreenshotGatewayError.unsupported)
	}
	func inspectTicket(_ ticket: String) async -> Result<TicketInspectionModel, Error> { .failure(ScreenshotGatewayError.unsupported) }
	func receive(ticket: String, outputDir: String, receiverName: String) async -> Result<Void, Error> { .success(()) }
	func receiveIntoSecurityScopedDirectory(ticket: String, outputDirectoryUrl: String, receiverName: String) async -> Result<Void, Error> { .success(()) }
	func cancel(transferId: UInt64) async -> Result<Void, Error> { .success(()) }
	func delete(transferId: UInt64) async -> Result<Void, Error> { .success(()) }
	func clearReceiveHistory() async -> Result<UInt64, Error> { .success(0) }
	func storageUsage() async -> Result<CoreStorageUsageModel, Error> { .success(CoreStorageUsageModel(blobStoreBytes: 0, appDataBytes: 0)) }
	func receivedArtifacts() async -> Result<[ReceivedArtifactModel], Error> { .success([]) }
	func respondReceiverRequest(requestId: String, accepted: Bool, reason: String?) async -> Result<Void, Error> { .success(()) }
	func refresh() async -> Result<Void, Error> { .success(()) }
	func resetUnrecoverableIdentity(appDataDir: String, networkConfiguration: RelayConfiguration) async -> Result<Void, Error> { .success(()) }

	// MARK: - Saved Devices (empty; no marketing screen stages them yet)

	func listPairingEligibilities() async -> Result<[PairingEligibilityModel], Error> { .success([]) }
	func declinePairingEligibility(peerEndpointId: String) async -> Result<Void, Error> { .success(()) }
	func requestSavedDevicePairing(peerEndpointId: String) async -> Result<Bool, Error> { .success(false) }
	func respondToDevicePairing(peerEndpointId: String, accepted: Bool) async -> Result<Bool, Error> { .success(false) }
	func listDeviceRelationships() async -> Result<[DeviceRelationshipModel], Error> { .success([]) }
	func listSavedDevices() async -> Result<[SavedDeviceModel], Error> { .success([]) }
	func setSavedDeviceLabel(peerEndpointId: String, label: String?) async -> Result<Void, Error> { .success(()) }
	func forgetSavedDevice(peerEndpointId: String) async -> Result<Void, Error> { .success(()) }
	func blockDevice(peerEndpointId: String) async -> Result<Void, Error> { .success(()) }
	func unblockDevice(peerEndpointId: String) async -> Result<Void, Error> { .success(()) }
	func listBlockedDevices() async -> Result<[String], Error> { .success([]) }
	func listPendingTargetedOffers() async -> Result<[PendingTargetedOfferModel], Error> { .success([]) }
	func respondToTargetedOffer(transferId: String, accepted: Bool) async -> Result<TargetedOfferResponseModel, Error> {
		.failure(ScreenshotGatewayError.unsupported)
	}
	func createTargetedTransfer(receiverEndpointId: String, sources: [ShareSource], transferName: String?) async -> Result<TargetedTransferModel, Error> {
		.failure(ScreenshotGatewayError.unsupported)
	}
	func listTargetedTransfers() async -> Result<[TargetedTransferModel], Error> { .success([]) }
	func receiveTargetedTransfer(transferId: String, outputDirectoryUrl: String) async -> Result<Void, Error> { .success(()) }
	func resumeTargetedTransfer(id: String, outputDirectoryUrl: String) async -> Result<Void, Error> { .success(()) }
	func cancelTargetedTransfer(id: String) async -> Result<Void, Error> { .success(()) }
	func deleteTargetedTransfer(id: String) async -> Result<Void, Error> { .success(()) }
}

private enum ScreenshotGatewayError: Error { case unsupported }
#endif
