import XCTest
import Combine
@testable import VniDrop

@MainActor
final class SavedDevicesModelTests: XCTestCase {
	private var gateway: FakeCoreGateway!
	private var fileSystem: FakeFileSystemService!
	private var messages: UiMessageController!

	override func setUp() async throws {
		gateway = FakeCoreGateway()
		fileSystem = FakeFileSystemService()
		messages = UiMessageController()
	}

	/// Builds the model and lets the initial refresh settle. The model loads on the
	/// combined (preferences, isInitialized) signal, so the core must look ready.
	private func makeModel() async -> SavedDevicesModel {
		var state = CoreState()
		state.isInitialized = true
		state.status = CoreStatus(endpointId: Self.localEndpoint, activeTransfers: 0, activeShares: 0)
		gateway.setState(state)
		let model = SavedDevicesModel(
			repository: gateway,
			fileSystemService: fileSystem,
			preferences: Fixtures.preferences(),
			messages: messages
		)
		await waitUntil { !model.state.isLoading }
		return model
	}

	private static let localEndpoint = "local-endpoint"
	private static let peer = "peer-endpoint"

	private func savedDevice(
		_ endpointId: String = peer,
		localLabel: String? = nil,
		remoteDisplayName: String? = "Remote Name",
		createdAt: Int64 = 1
	) -> SavedDeviceModel {
		SavedDeviceModel(
			endpointId: endpointId, localLabel: localLabel, remoteDisplayName: remoteDisplayName,
			createdAt: createdAt, lastAuthenticatedAt: nil
		)
	}

	private func transfer(
		id: String = "t1",
		sender: String = localEndpoint,
		receiver: String = peer,
		state: TargetedTransferStateModel = .completed,
		verifiedBytes: UInt64 = 0,
		totalSize: UInt64 = 100,
		updatedAt: Int64 = 1
	) -> TargetedTransferModel {
		TargetedTransferModel(
			id: id, senderEndpointId: sender, receiverEndpointId: receiver, manifestId: "m",
			transferName: "Photos", fileCount: 2, totalSize: totalSize, verifiedBytes: verifiedBytes,
			state: state, createdAt: 1, updatedAt: updatedAt
		)
	}

	private func relationship(
		_ endpointId: String = peer,
		state: DeviceRelationshipStateModel,
		updatedAt: Int64 = 1
	) -> DeviceRelationshipModel {
		DeviceRelationshipModel(
			remoteEndpointId: endpointId, state: state, generation: 1,
			minimumProtocolVersion: 1, createdAt: 1, updatedAt: updatedAt
		)
	}

	private func eligibility(
		_ endpointId: String = peer,
		remoteDisplayName: String? = "Eligible Device",
		createdAt: Int64 = 1
	) -> PairingEligibilityModel {
		PairingEligibilityModel(
			peerEndpointId: endpointId, remoteDisplayName: remoteDisplayName, sessionId: "s",
			protocolVersion: 1, createdAt: createdAt, expiresAt: createdAt + 86_400
		)
	}

	// MARK: - Snapshot composition

	func testLoadsSnapshotAndKeepsTransfersOutOfTheDeviceList() async {
		gateway.savedDevices = [savedDevice()]
		gateway.deviceRelationships = [relationship(state: .saved)]
		gateway.targetedTransfers = [transfer()]
		let model = await makeModel()

		XCTAssertEqual(model.state.savedDevices.map(\.endpointId), [Self.peer])
		// A saved relationship is not "pending" and must not appear as one.
		XCTAssertTrue(model.state.pendingRelationships.isEmpty)
		// Transfers are reachable only per-device, never as a global list on screen.
		XCTAssertEqual(model.state.transfers(for: Self.peer).map(\.id), ["t1"])
	}

	func testResolvesDirectionAgainstLocalEndpoint() async {
		gateway.savedDevices = [savedDevice()]
		gateway.targetedTransfers = [
			transfer(id: "out", sender: Self.localEndpoint, receiver: Self.peer),
			transfer(id: "in", sender: Self.peer, receiver: Self.localEndpoint),
		]
		let model = await makeModel()

		let byId = Dictionary(uniqueKeysWithValues: model.state.targetedTransfers.map { ($0.id, $0) })
		XCTAssertEqual(byId["out"]?.direction, .outgoing)
		XCTAssertEqual(byId["in"]?.direction, .incoming)
		// Either way the peer is the *other* device.
		XCTAssertEqual(byId["out"]?.peerEndpointId, Self.peer)
		XCTAssertEqual(byId["in"]?.peerEndpointId, Self.peer)
	}

	func testHidesDeletedTransfers() async {
		gateway.targetedTransfers = [transfer(id: "kept"), transfer(id: "gone", state: .deleted)]
		let model = await makeModel()

		XCTAssertEqual(model.state.targetedTransfers.map(\.id), ["kept"])
	}

	func testPrefersLocalLabelOverRemoteDisplayName() async {
		gateway.savedDevices = [savedDevice(localLabel: "My Laptop", remoteDisplayName: "hostname")]
		gateway.targetedTransfers = [transfer()]
		let model = await makeModel()

		XCTAssertEqual(model.state.targetedTransfers.first?.peerDisplayName, "My Laptop")
	}

	func testBlankLocalLabelFallsBackToRemoteDisplayName() async {
		gateway.savedDevices = [savedDevice(localLabel: "   ", remoteDisplayName: "hostname")]
		gateway.targetedTransfers = [transfer()]
		let model = await makeModel()

		XCTAssertEqual(model.state.targetedTransfers.first?.peerDisplayName, "hostname")
	}

	func testFailedLoadIsReportedRatherThanRenderedPartially() async {
		gateway.savedDevicesResult = .failure(TestError.unimplemented)
		let model = await makeModel()

		XCTAssertTrue(model.state.loadFailed)
		XCTAssertTrue(model.state.savedDevices.isEmpty)
	}

	// MARK: - Pairing prompt

	func testIncomingRequestOutranksEligibility() async {
		gateway.deviceRelationships = [relationship(state: .pendingIncoming)]
		gateway.pairingEligibilities = [eligibility("other-peer")]
		let model = await makeModel()

		guard case .incomingRequest(let peerId, _) = model.state.pairingPrompt.prompt else {
			return XCTFail("expected an incoming request to take priority")
		}
		XCTAssertEqual(peerId, Self.peer)
	}

	func testDismissingEligibilityDoesNotConsumeItInTheCore() async {
		gateway.pairingEligibilities = [eligibility()]
		let model = await makeModel()
		XCTAssertNotNil(model.state.pairingPrompt.prompt)

		model.dismissPairingPrompt()

		XCTAssertNil(model.state.pairingPrompt.prompt)
		// Dismissal is local suppression, not a decline: the single-use capability
		// must survive so the device stays actionable from the list.
		XCTAssertTrue(gateway.declinedEligibilities.isEmpty)
		XCTAssertEqual(model.state.eligibilities.count, 1)
	}

	func testDismissedEligibilityDoesNotReappearOnRefresh() async {
		gateway.pairingEligibilities = [eligibility()]
		let model = await makeModel()
		model.dismissPairingPrompt()

		gateway.emit(.pairingChanged)
		await waitUntil { !model.state.isLoading }

		XCTAssertNil(model.state.pairingPrompt.prompt)
	}

	func testDecliningEligibilityConsumesItInTheCore() async {
		gateway.pairingEligibilities = [eligibility()]
		let model = await makeModel()

		model.declinePairingPrompt()
		await waitUntil { !model.state.pairingPrompt.busy }

		XCTAssertEqual(gateway.declinedEligibilities, [Self.peer])
	}

	// MARK: - Label editing

	func testLabelSaveTrimsAndClosesEditorOnSuccess() async {
		gateway.savedDevices = [savedDevice()]
		let model = await makeModel()
		model.openLabelEditor(Self.peer)
		model.setLabelDraft("  Studio Mac  ")

		model.saveLabel()
		await waitUntil { !model.state.isSavingLabel }

		XCTAssertEqual(gateway.setLabels.map(\.label), ["Studio Mac"])
		XCTAssertNil(model.state.labelingPeerId)
		XCTAssertEqual(model.state.labelDraft, "")
	}

	func testLabelFailurePreservesDraftAndEditor() async {
		gateway.savedDevices = [savedDevice()]
		gateway.setLabelResult = .failure(TestError.unimplemented)
		let model = await makeModel()
		model.openLabelEditor(Self.peer)
		model.setLabelDraft("Studio Mac")

		model.saveLabel()
		await waitUntil { !model.state.isSavingLabel }

		// The retry path depends on both surviving.
		XCTAssertEqual(model.state.labelingPeerId, Self.peer)
		XCTAssertEqual(model.state.labelDraft, "Studio Mac")
	}

	func testEditorCannotBeDismissedOrEditedWhileSaving() async {
		gateway.savedDevices = [savedDevice()]
		let model = await makeModel()
		model.openLabelEditor(Self.peer)
		model.setLabelDraft("Studio Mac")

		model.saveLabel()
		// Still in flight: conflicting actions must be refused, not queued.
		model.setLabelDraft("Something Else")
		model.dismissLabelEditor()
		XCTAssertEqual(model.state.labelDraft, "Studio Mac")
		XCTAssertEqual(model.state.labelingPeerId, Self.peer)

		await waitUntil { !model.state.isSavingLabel }
		XCTAssertEqual(gateway.setLabels.map(\.label), ["Studio Mac"])
	}

	func testClearingLabelSendsNil() async {
		gateway.savedDevices = [savedDevice(localLabel: "Old")]
		let model = await makeModel()
		model.openLabelEditor(Self.peer)

		model.clearLabel()
		await waitUntil { !model.state.isSavingLabel }

		XCTAssertEqual(gateway.setLabels.count, 1)
		XCTAssertNil(gateway.setLabels[0].label)
	}

	func testBlankDraftClearsTheLabel() async {
		gateway.savedDevices = [savedDevice(localLabel: "Old")]
		let model = await makeModel()
		model.openLabelEditor(Self.peer)
		model.setLabelDraft("   ")

		model.saveLabel()
		await waitUntil { !model.state.isSavingLabel }

		XCTAssertNil(gateway.setLabels[0].label)
	}

	// MARK: - Targeted offers

	func testApprovingOfferStartsTheReceivePull() async {
		gateway.offerResponseResult = .success(.approved(transferId: "t1"))
		let model = await makeModel()

		model.acceptTargetedOffer("t1")
		await waitUntil { self.gateway.offerResponses.count == 1 && !model.state.isLoading }

		XCTAssertEqual(gateway.offerResponses.map(\.accepted), [true])
		XCTAssertEqual(gateway.targetedReceives.map(\.transferId), ["t1"])
	}

	func testDecliningOfferDoesNotPull() async {
		gateway.offerResponseResult = .success(.declined)
		let model = await makeModel()

		model.declineTargetedOffer("t1")
		await waitUntil { self.gateway.offerResponses.count == 1 }

		XCTAssertEqual(gateway.offerResponses.map(\.accepted), [false])
		XCTAssertTrue(gateway.targetedReceives.isEmpty)
	}

	func testAlreadySettledOfferDoesNotStartASecondPull() async {
		// The idempotent replay path returns the existing result; re-pulling would
		// duplicate work against a transfer that is already resolved.
		gateway.offerResponseResult = .success(.alreadySettled(transferId: "t1"))
		let model = await makeModel()

		model.acceptTargetedOffer("t1")
		await waitUntil { self.gateway.offerResponses.count == 1 }

		XCTAssertTrue(gateway.targetedReceives.isEmpty)
	}

	func testConcurrentResponsesToTheSameOfferAreIgnored() async {
		gateway.offerResponseResult = .success(.declined)
		let model = await makeModel()

		model.declineTargetedOffer("t1")
		model.declineTargetedOffer("t1")
		await waitUntil { !model.state.targetedOffers.respondingIds.contains("t1") }

		XCTAssertEqual(gateway.offerResponses.count, 1)
	}

	// MARK: - Transfer lifecycle

	func testResumeUsesTheResumePath() async {
		let model = await makeModel()

		model.resumeTargetedTransfer("t1")
		await waitUntil { !model.state.busyTransferIds.contains("t1") }

		XCTAssertEqual(gateway.targetedResumes.map(\.id), ["t1"])
		XCTAssertTrue(gateway.targetedReceives.isEmpty)
	}

	func testCancelAndDeleteReachTheCore() async {
		let model = await makeModel()

		model.cancelTargetedTransfer("t1")
		await waitUntil { !model.state.busyTransferIds.contains("t1") }
		model.deleteTargetedTransfer("t2")
		await waitUntil { !model.state.busyTransferIds.contains("t2") }

		XCTAssertEqual(gateway.cancelledTargetedTransfers, ["t1"])
		XCTAssertEqual(gateway.deletedTargetedTransfers, ["t2"])
	}

	func testBusyTransferIgnoresRepeatedCommands() async {
		let model = await makeModel()

		model.cancelTargetedTransfer("t1")
		model.cancelTargetedTransfer("t1")
		await waitUntil { !model.state.busyTransferIds.contains("t1") }

		XCTAssertEqual(gateway.cancelledTargetedTransfers, ["t1"])
	}

	// MARK: - Destructive actions

	func testForgetAndBlockReachTheCoreAndRefresh() async {
		gateway.savedDevices = [savedDevice()]
		let model = await makeModel()

		model.forget(Self.peer)
		await waitUntil { !model.state.busyPeerIds.contains(Self.peer) }
		model.block(Self.peer)
		await waitUntil { !model.state.busyPeerIds.contains(Self.peer) }

		XCTAssertEqual(gateway.forgottenDevices, [Self.peer])
		XCTAssertEqual(gateway.blockedCalls, [Self.peer])
	}

	func testBusyPeerIgnoresRepeatedCommands() async {
		gateway.savedDevices = [savedDevice()]
		let model = await makeModel()

		model.forget(Self.peer)
		model.forget(Self.peer)
		await waitUntil { !model.state.busyPeerIds.contains(Self.peer) }

		XCTAssertEqual(gateway.forgottenDevices, [Self.peer])
	}

	// MARK: - Targeted send

	private func picked(_ name: String, isDirectory: Bool = false) -> PickedShareFile {
		PickedShareFile(
			value: "/tmp/\(name)", displayName: name, sizeBytes: 10,
			isTemporaryCopy: true, isDirectory: isDirectory
		)
	}

	func testSendRequiresDestinationSourcesAndName() async {
		gateway.savedDevices = [savedDevice()]
		let model = await makeModel()
		XCTAssertFalse(model.state.canCreateTargetedTransfer)

		model.beginSend(to: Self.peer)
		XCTAssertFalse(model.state.canCreateTargetedTransfer, "no sources yet")

		model.onSendFilesPicked([picked("a.txt")])
		XCTAssertTrue(model.state.canCreateTargetedTransfer)

		model.setSendTransferName("   ")
		XCTAssertFalse(model.state.canCreateTargetedTransfer, "a blank name is not a name")
	}

	func testSendCreatesTargetedTransferAndClosesComposition() async {
		gateway.savedDevices = [savedDevice()]
		gateway.createTargetedTransferResult = .success(transfer())
		let model = await makeModel()
		model.beginSend(to: Self.peer)
		model.onSendFilesPicked([picked("a.txt")])

		model.createTargetedTransfer()
		await waitUntil { !model.state.isCreatingSend }

		XCTAssertEqual(gateway.createdTargetedTransfers.map(\.receiverEndpointId), [Self.peer])
		XCTAssertEqual(gateway.createdTargetedTransfers.first?.transferName, "a.txt")
		XCTAssertNil(model.state.sendTargetPeerId)
		XCTAssertTrue(model.state.sendFiles.isEmpty)
		// The core owns the bytes once the transfer exists; the picker copy goes.
		XCTAssertEqual(fileSystem.discardedFiles, ["/tmp/a.txt"])
	}

	func testSendFailureKeepsCompositionForRetry() async {
		gateway.savedDevices = [savedDevice()]
		gateway.createTargetedTransferResult = .failure(TestError.unimplemented)
		let model = await makeModel()
		model.beginSend(to: Self.peer)
		model.onSendFilesPicked([picked("a.txt")])
		model.setSendTransferName("Report")

		model.createTargetedTransfer()
		await waitUntil { !model.state.isCreatingSend }

		// Retrying must not require re-picking the sources.
		XCTAssertEqual(model.state.sendTargetPeerId, Self.peer)
		XCTAssertEqual(model.state.sendFiles.map(\.value), ["/tmp/a.txt"])
		XCTAssertEqual(model.state.sendTransferName, "Report")
		XCTAssertTrue(fileSystem.discardedFiles.isEmpty, "sources must survive a failed create")
	}

	func testRemovingASourceRederivesOnlyAGeneratedName() async {
		let model = await makeModel()
		model.beginSend(to: Self.peer)
		model.onSendFilesPicked([picked("a.txt"), picked("b.txt")])
		model.setSendTransferName("My Name")

		model.removeSendFile("/tmp/b.txt")

		XCTAssertEqual(model.state.sendTransferName, "My Name")
		XCTAssertEqual(model.state.sendFiles.map(\.value), ["/tmp/a.txt"])
	}

	func testReplacingSelectionDiscardsThePreviousPickerCopies() async {
		let model = await makeModel()
		model.beginSend(to: Self.peer)
		model.onSendFilesPicked([picked("a.txt")])

		model.onSendFilesPicked([picked("b.txt")])
		await waitUntil { !self.fileSystem.discardedFiles.isEmpty }

		XCTAssertEqual(fileSystem.discardedFiles, ["/tmp/a.txt"])
		XCTAssertEqual(model.state.sendFiles.map(\.value), ["/tmp/b.txt"])
	}

	func testCancellingSendDiscardsSources() async {
		let model = await makeModel()
		model.beginSend(to: Self.peer)
		model.onSendFilesPicked([picked("a.txt")])

		model.cancelSend()
		await waitUntil { !self.fileSystem.discardedFiles.isEmpty }

		XCTAssertNil(model.state.sendTargetPeerId)
		XCTAssertEqual(fileSystem.discardedFiles, ["/tmp/a.txt"])
	}

	func testFolderSourcesAreSupported() async {
		gateway.savedDevices = [savedDevice()]
		gateway.createTargetedTransferResult = .success(transfer())
		let model = await makeModel()
		model.beginSend(to: Self.peer)
		model.onSendFilesPicked([picked("Photos", isDirectory: true)])

		model.createTargetedTransfer()
		await waitUntil { !model.state.isCreatingSend }

		XCTAssertEqual(fileSystem.targetedSends.first?.files.first?.isDirectory, true)
	}

	// MARK: - Signals

	func testPairingSignalTriggersRefresh() async {
		let model = await makeModel()
		gateway.savedDevices = [savedDevice()]

		gateway.emit(.pairingChanged)
		await waitUntil { model.state.savedDevices.count == 1 }

		XCTAssertEqual(model.state.savedDevices.map(\.endpointId), [Self.peer])
	}

	func testInvitationSignalsDoNotTriggerRefresh() async {
		let model = await makeModel()
		gateway.savedDevices = [savedDevice()]

		gateway.emit(.transfersChanged(transferId: 1))
		gateway.emit(.approvalChanged(transferId: 1))
		try? await Task.sleep(nanoseconds: 100_000_000)

		// The saved-device domain is unaffected by invitation-share activity.
		XCTAssertTrue(model.state.savedDevices.isEmpty)
	}
}
