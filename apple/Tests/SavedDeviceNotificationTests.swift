import XCTest
@testable import VniDrop

@MainActor
final class SavedDeviceNotificationTests: XCTestCase {
	private static let peer = "peer-endpoint"

	private func state(
		pendingRelationships: [DeviceRelationshipModel] = [],
		eligibilities: [PairingEligibilityModel] = [],
		savedDevices: [SavedDeviceModel] = [],
		offers: [PendingTargetedOfferModel] = [],
		senderDisplayNames: [String: String] = [:],
		transfers: [SavedDeviceTransferItem] = []
	) -> SavedDevicesState {
		var state = SavedDevicesState()
		state.isLoading = false
		state.pendingRelationships = pendingRelationships
		state.eligibilities = eligibilities
		state.savedDevices = savedDevices
		state.targetedOffers.pending = offers
		state.targetedOffers.senderDisplayNames = senderDisplayNames
		state.targetedTransfers = transfers
		return state
	}

	private func relationship(_ state: DeviceRelationshipStateModel) -> DeviceRelationshipModel {
		DeviceRelationshipModel(
			remoteEndpointId: Self.peer, state: state, generation: 1,
			minimumProtocolVersion: 1, createdAt: 1, updatedAt: 1
		)
	}

	private func offer(_ transferId: String = "t1") -> PendingTargetedOfferModel {
		PendingTargetedOfferModel(
			transferId: transferId, senderEndpointId: Self.peer, receiverEndpointId: "me",
			manifestId: "m", contentHash: "h", transferName: "Photos", fileCount: 1,
			totalSize: 10, protocolVersion: 1, receivedAt: 1
		)
	}

	private func transferItem(
		id: String = "t1",
		state: TargetedTransferStateModel,
		direction: SavedDeviceTransferDirection = .incoming
	) -> SavedDeviceTransferItem {
		SavedDeviceTransferItem(
			id: id, peerEndpointId: Self.peer, peerDisplayName: "Studio Mac",
			direction: direction, transferName: "Photos", fileCount: 1, totalSize: 10,
			verifiedBytes: 10, state: state, createdAt: 1, updatedAt: 1
		)
	}

	// MARK: - Prompts

	func testIncomingPairingRequestIsPlanned() {
		let planned = plannedSavedDevicePrompts(state(pendingRelationships: [relationship(.pendingIncoming)]))

		XCTAssertEqual(planned.map(\.kind), [.pairingRequest])
		XCTAssertEqual(planned.first?.id, "pairing-request-\(Self.peer)")
	}

	func testOutgoingPairingRequestIsNotPlanned() {
		// We are the ones waiting; there is nothing for the user to answer.
		let planned = plannedSavedDevicePrompts(state(pendingRelationships: [relationship(.pendingOutgoing)]))

		XCTAssertTrue(planned.isEmpty)
	}

	func testPairingRequestUsesEligibilityNameWhenAvailable() {
		let eligibility = PairingEligibilityModel(
			peerEndpointId: Self.peer, remoteDisplayName: "Alice's Mac", sessionId: "s",
			protocolVersion: 1, createdAt: 1, expiresAt: 2
		)
		let planned = plannedSavedDevicePrompts(state(
			pendingRelationships: [relationship(.pendingIncoming)],
			eligibilities: [eligibility]
		))

		XCTAssertEqual(planned.first?.deviceName, "Alice's Mac")
	}

	func testPendingOfferIsPlannedWithSenderNameOnlyWhenSaved() {
		let unnamed = plannedSavedDevicePrompts(state(offers: [offer()]))
		XCTAssertEqual(unnamed.map(\.kind), [.targetedOffer])
		// An unsaved sender has no name we can vouch for.
		XCTAssertNil(unnamed.first?.deviceName)

		let named = plannedSavedDevicePrompts(state(
			offers: [offer()],
			senderDisplayNames: [Self.peer: "Studio Mac"]
		))
		XCTAssertEqual(named.first?.deviceName, "Studio Mac")
	}

	func testPromptIdsAreStablePerSubject() {
		// Stable ids are what let the coordinator withdraw a prompt once answered.
		let first = plannedSavedDevicePrompts(state(offers: [offer()]))
		let second = plannedSavedDevicePrompts(state(offers: [offer()]))

		XCTAssertEqual(first.map(\.id), second.map(\.id))
	}

	// MARK: - Terminal outcomes

	func testCompletedAndFailedTransfersArePlanned() {
		let planned = plannedTargetedOutcomes(
			[transferItem(id: "a", state: .completed), transferItem(id: "b", state: .failed)],
			published: []
		)

		XCTAssertEqual(planned.map(\.kind), [.targetedReceiveCompleted, .targetedReceiveFailed])
	}

	func testOutcomeWordingFollowsDirection() {
		// On the sending device it is the *peer* that finished downloading. Using
		// the receive wording here told the sender it had downloaded its own files.
		let outgoing = plannedTargetedOutcomes(
			[
				transferItem(id: "a", state: .completed, direction: .outgoing),
				transferItem(id: "b", state: .failed, direction: .outgoing),
			],
			published: []
		)
		XCTAssertEqual(outgoing.map(\.kind), [.targetedSendCompleted, .targetedSendFailed])

		let incoming = plannedTargetedOutcomes(
			[transferItem(id: "c", state: .completed, direction: .incoming)],
			published: []
		)
		XCTAssertEqual(incoming.map(\.kind), [.targetedReceiveCompleted])
	}

	func testIdsDoNotCollideAcrossDirections() {
		let incoming = plannedTargetedOutcomes([transferItem(state: .completed)], published: [])
		let outgoing = plannedTargetedOutcomes(
			[transferItem(state: .completed, direction: .outgoing)], published: []
		)

		XCTAssertNotEqual(incoming.first?.id, outgoing.first?.id)
	}

	func testUserDrivenTerminalStatesAreNotPlanned() {
		// The user already knows: they cancelled or declined it themselves.
		let planned = plannedTargetedOutcomes(
			[
				transferItem(id: "a", state: .cancelled),
				transferItem(id: "b", state: .declined),
				transferItem(id: "c", state: .transferring),
			],
			published: []
		)

		XCTAssertTrue(planned.isEmpty)
	}

	func testAlreadyPublishedOutcomesAreNotReplanned() {
		let first = plannedTargetedOutcomes([transferItem(state: .completed)], published: [])
		XCTAssertEqual(first.count, 1)

		let second = plannedTargetedOutcomes(
			[transferItem(state: .completed)],
			published: Set(first.map(\.id))
		)
		XCTAssertTrue(second.isEmpty)
	}
}
