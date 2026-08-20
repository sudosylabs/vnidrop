import XCTest
@testable import VniDrop

final class SavedDevicesReadModelTests: XCTestCase {
	private let readModel = SavedDevicesReadModel()

	func testTransferActionMatrixIsDerivedBeforeRendering() {
		let activeStates: [TargetedTransferStateModel] = [
			.preparing, .offering, .awaitingApproval, .approved,
			.connecting, .transferring, .interrupted,
		]
		let terminalStates: [TargetedTransferStateModel] = [
			.completed, .declined, .cancelled, .failed,
		]

		for state in activeStates {
			XCTAssertEqual(item(state, .sender).availableActions, [.cancel], "outgoing \(state)")
			let expected: [SavedDeviceTransferAction]
			switch state {
			case .approved: expected = [.receive, .cancel]
			case .interrupted: expected = [.resume, .cancel]
			default: expected = [.cancel]
			}
			XCTAssertEqual(item(state, .receiver).availableActions, expected, "incoming \(state)")
		}
		for state in terminalStates {
			XCTAssertEqual(item(state, .sender).availableActions, [.delete], "\(state)")
		}
	}

	func testProgressFactsAreReceiverOnlyAndSurviveInterruption() {
		for state: TargetedTransferStateModel in [.connecting, .transferring, .interrupted] {
			XCTAssertEqual(item(state, .receiver).progressFraction ?? -1, 0.4, accuracy: 0.001)
			XCTAssertNil(item(state, .sender).progressFraction)
		}
		XCTAssertNil(item(.approved, .receiver).progressFraction)
		XCTAssertNil(item(.completed, .receiver).progressFraction)
	}

	func testDurableReadsProduceOneSortedJoinedPlatformSnapshot() {
		let snapshot = readModel.derive(
			SavedDevicesReadInputs(
				eligibilities: [eligibility("dismissed", 4), eligibility("eligible", 2)],
				relationships: [
					relationship("old-outgoing", .pendingOutgoing, 1),
					relationship("incoming", .pendingIncoming, 3),
					relationship("saved", .saved, 5),
				],
				savedDevices: [
					device("peer", "Desk", "Remote", 2),
					device("older", nil, "Laptop", 1),
				],
				pendingOffers: [offer("later", 9), offer("earlier", 7)],
				targetedTransfers: [
					transfer("old", .sender, .completed, 1),
					transfer("deleted", .sender, .deleted, 3),
					transfer("new", .receiver, .interrupted, 2),
				]
			),
			dismissedEligibilityIds: ["dismissed"]
		)

		XCTAssertEqual(snapshot.eligibilities.map(\.peerEndpointId), ["dismissed", "eligible"])
		XCTAssertEqual(snapshot.pendingRelationships.map(\.remoteEndpointId), ["incoming", "old-outgoing"])
		XCTAssertEqual(snapshot.savedDevices.map(\.endpointId), ["peer", "older"])
		XCTAssertEqual(snapshot.pendingOffers.map(\.transferId), ["earlier", "later"])
		XCTAssertEqual(snapshot.targetedTransfers.map(\.id), ["new", "old"])
		XCTAssertEqual(snapshot.targetedTransfers.first { $0.id == "old" }?.peerDisplayName, "Desk")
		XCTAssertEqual(snapshot.nextPairingPrompt, .incomingRequest(peerEndpointId: "incoming", remoteDisplayName: nil))
	}

	func testRefreshAndRestartNeedOnlyDurableReadsNotAnEventPayload() {
		let inputs = SavedDevicesReadInputs(
			savedDevices: [device("peer", nil, "Phone", 1)],
			targetedTransfers: [transfer("transfer", .sender, .failed, 1)]
		)
		XCTAssertEqual(readModel.derive(inputs), SavedDevicesReadModel().derive(inputs))
	}

	private func item(
		_ state: TargetedTransferStateModel,
		_ role: TargetedTransferRoleModel
	) -> SavedDeviceTransferItem {
		readModel.derive(
			SavedDevicesReadInputs(targetedTransfers: [transfer("id", role, state, 1)])
		).targetedTransfers[0]
	}

	private func eligibility(_ peer: String, _ createdAt: Int64) -> PairingEligibilityModel {
		PairingEligibilityModel(
			peerEndpointId: peer, remoteDisplayName: nil, sessionId: "session-\(peer)",
			protocolVersion: 1, createdAt: createdAt, expiresAt: createdAt + 10
		)
	}

	private func relationship(
		_ peer: String,
		_ state: DeviceRelationshipStateModel,
		_ updatedAt: Int64
	) -> DeviceRelationshipModel {
		DeviceRelationshipModel(
			remoteEndpointId: peer, state: state, generation: 1,
			minimumProtocolVersion: 1, createdAt: 0, updatedAt: updatedAt
		)
	}

	private func device(
		_ id: String,
		_ label: String?,
		_ remoteName: String?,
		_ createdAt: Int64
	) -> SavedDeviceModel {
		SavedDeviceModel(
			endpointId: id, localLabel: label, remoteDisplayName: remoteName,
			createdAt: createdAt, lastAuthenticatedAt: createdAt
		)
	}

	private func offer(_ id: String, _ receivedAt: Int64) -> PendingTargetedOfferModel {
		PendingTargetedOfferModel(
			transferId: id, senderEndpointId: "peer", receiverEndpointId: "local",
			manifestId: "manifest-\(id)", contentHash: "hash-\(id)", transferName: id,
			fileCount: 1, totalSize: 100, protocolVersion: 1, receivedAt: receivedAt
		)
	}

	private func transfer(
		_ id: String,
		_ role: TargetedTransferRoleModel,
		_ state: TargetedTransferStateModel,
		_ updatedAt: Int64
	) -> TargetedTransferModel {
		TargetedTransferModel(
			id: id, role: role,
			senderEndpointId: role == .sender ? "local" : "peer",
			receiverEndpointId: role == .sender ? "peer" : "local",
			manifestId: "manifest-\(id)", contentHash: "hash-\(id)",
			transferName: id, fileCount: 1,
			totalSize: 100, verifiedBytes: 40, state: state,
			createdAt: 0, updatedAt: updatedAt
		)
	}
}
