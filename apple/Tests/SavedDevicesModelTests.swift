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
		role: TargetedTransferRoleModel? = nil,
		state: TargetedTransferStateModel = .completed,
		verifiedBytes: UInt64 = 0,
		totalSize: UInt64 = 100,
		updatedAt: Int64 = 1
	) -> TargetedTransferModel {
		TargetedTransferModel(
			// Matches what the core records for a row created under the current
			// identity; pass `role` explicitly to model one that predates a reset.
			id: id, role: role ?? (sender == Self.localEndpoint ? .sender : .receiver),
			senderEndpointId: sender, receiverEndpointId: receiver,
			manifestId: "m", contentHash: "hash",
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

	func testDirectionSurvivesAnIdentityReset() async {
		// A row created before the reset names an endpoint this device no longer
		// has. Inferring direction from the sender made these read as incoming
		// from the device's own retired identity — a phantom peer in the list.
		let retiredLocal = "retired-local-endpoint"
		gateway.savedDevices = [savedDevice()]
		gateway.targetedTransfers = [
			transfer(id: "stale-out", sender: retiredLocal, receiver: Self.peer, role: .sender),
			transfer(id: "stale-in", sender: Self.peer, receiver: retiredLocal, role: .receiver),
		]
		let model = await makeModel()

		let byId = Dictionary(uniqueKeysWithValues: model.state.targetedTransfers.map { ($0.id, $0) })
		XCTAssertEqual(byId["stale-out"]?.direction, .outgoing)
		XCTAssertEqual(byId["stale-in"]?.direction, .incoming)
		// The peer must stay the other device, never this device's old identity.
		XCTAssertEqual(byId["stale-out"]?.peerEndpointId, Self.peer)
		XCTAssertEqual(byId["stale-in"]?.peerEndpointId, Self.peer)
	}

	func testResolvesDirectionFromTheRecordedRole() async {
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

	// MARK: - Transfer actions

	func testOnlyTheReceivingSideIsOfferedReceiveAndResume() async {
		gateway.savedDevices = [savedDevice()]
		gateway.targetedTransfers = [
			transfer(id: "in-approved", sender: Self.peer, receiver: Self.localEndpoint, state: .approved),
			transfer(id: "out-approved", sender: Self.localEndpoint, receiver: Self.peer, state: .approved),
			transfer(id: "in-interrupted", sender: Self.peer, receiver: Self.localEndpoint, state: .interrupted),
			transfer(id: "out-interrupted", sender: Self.localEndpoint, receiver: Self.peer, state: .interrupted),
		]
		let model = await makeModel()
		let byId = Dictionary(uniqueKeysWithValues: model.state.targetedTransfers.map { ($0.id, $0) })

		XCTAssertEqual(byId["in-approved"]?.canReceive, true)
		XCTAssertEqual(byId["in-interrupted"]?.canResume, true)
		// The sender has nothing to pull: it is the one holding the files. Offering
		// "Receive" there asked it to download its own outgoing transfer.
		XCTAssertEqual(byId["out-approved"]?.canReceive, false)
		XCTAssertEqual(byId["out-interrupted"]?.canResume, false)
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

	/// Sets up a create that never returns until released — an unavailable peer,
	/// where the core waits out its connection and offer timeouts. `announcesId`
	/// mirrors the core emitting `created` once the row exists, which it does
	/// before it ever contacts the peer.
	private func stalledSend(
		_ model: SavedDevicesModel,
		id: String = "t-inflight",
		announcesId: Bool = true
	) async {
		gateway.holdsTargetedCreate = true
		model.beginSend(to: Self.peer)
		model.onSendFilesPicked([picked("a.txt")])
		model.createTargetedTransfer()
		await waitUntil { self.gateway.isHoldingTargetedCreate }
		if announcesId {
			gateway.emitTargetedTransferCreated(id: id)
			await waitUntil { !model.state.isCreatingSend || model.knowsInFlightSendTransfer }
		}
		XCTAssertTrue(model.state.isCreatingSend)
	}

	func testCancellingAnUnansweredSendReachesTheCoreWhileItIsStillRunning() async {
		gateway.savedDevices = [savedDevice()]
		gateway.createTargetedTransferResult = .success(transfer(id: "t-inflight"))
		let model = await makeModel()
		await stalledSend(model)

		model.abandonSend()

		// The point of cancelling: it must not wait for the create to finish. The
		// cancel goes out while the core is still parked inside that very call.
		await waitUntil { !self.gateway.cancelledTargetedTransfers.isEmpty }
		XCTAssertEqual(gateway.cancelledTargetedTransfers, ["t-inflight"])
		XCTAssertFalse(model.state.isCreatingSend)
		XCTAssertNil(model.state.sendTargetPeerId)

		gateway.releaseTargetedCreate()
	}

	func testCancelledSendIsDeletedRatherThanLeftInHistory() async {
		gateway.savedDevices = [savedDevice()]
		gateway.createTargetedTransferResult = .success(transfer(id: "t-inflight"))
		let model = await makeModel()
		await stalledSend(model)

		model.abandonSend()
		await waitUntil { !self.gateway.deletedTargetedTransfers.isEmpty }

		// Cancelling is not "closing the sheet": the core would otherwise record a
		// failed transfer, and the user would be told a send they called off failed.
		XCTAssertEqual(gateway.deletedTargetedTransfers, ["t-inflight"])
		gateway.releaseTargetedCreate()
	}

	func testCancelledSendNeverReachesHistoryEvenIfARefreshRacesTheDelete() async {
		gateway.savedDevices = [savedDevice()]
		gateway.createTargetedTransferResult = .success(transfer(id: "t-inflight"))
		// The core reports it as failed — the state an unanswered offer lands in.
		gateway.targetedTransfers = [transfer(id: "t-inflight", state: .failed)]
		let model = await makeModel()
		await stalledSend(model)

		model.abandonSend()
		gateway.releaseTargetedCreate()
		await waitUntil { !self.gateway.deletedTargetedTransfers.isEmpty }

		// Nothing about the cancelled send may surface, or the notification
		// coordinator announces a failure for work the user called off.
		XCTAssertTrue(model.state.targetedTransfers.isEmpty)
	}

	func testCancellingReleasesTheComposerBeforeTheCreateReturns() async {
		gateway.savedDevices = [savedDevice()]
		gateway.createTargetedTransferResult = .success(transfer(id: "t-inflight"))
		let model = await makeModel()
		await stalledSend(model)

		model.abandonSend()

		XCTAssertFalse(model.state.isCreatingSend)
		XCTAssertTrue(model.state.sendFiles.isEmpty)
		// The import still owns the sources until the call lands.
		XCTAssertTrue(fileSystem.discardedFiles.isEmpty)

		gateway.releaseTargetedCreate()
		await waitUntil { !self.fileSystem.discardedFiles.isEmpty }
		XCTAssertEqual(fileSystem.discardedFiles, ["/tmp/a.txt"])
	}

	func testCancelledSendWithNoIdYetIsStillCleanedUpWhenTheCreateReturns() async {
		gateway.savedDevices = [savedDevice()]
		gateway.createTargetedTransferResult = .success(transfer(id: "t-late"))
		let model = await makeModel()
		// The create beat its own `created` event, so cancelling has no id to use.
		await stalledSend(model, announcesId: false)
		model.abandonSend()
		XCTAssertTrue(gateway.cancelledTargetedTransfers.isEmpty)

		gateway.releaseTargetedCreate()

		// The result carries the id, so the cleanup still happens — just later.
		await waitUntil { !self.gateway.cancelledTargetedTransfers.isEmpty }
		XCTAssertEqual(gateway.cancelledTargetedTransfers, ["t-late"])
		XCTAssertEqual(gateway.deletedTargetedTransfers, ["t-late"])
	}

	func testCancelledSendThatNeverRegisteredHasNothingToCancel() async {
		gateway.savedDevices = [savedDevice()]
		gateway.createTargetedTransferResult = .failure(TestError.unimplemented)
		let model = await makeModel()
		await stalledSend(model, announcesId: false)
		model.abandonSend()

		gateway.releaseTargetedCreate()

		await waitUntil { !self.fileSystem.discardedFiles.isEmpty }
		XCTAssertTrue(gateway.cancelledTargetedTransfers.isEmpty)
		// The composition is gone, so a failure the user walked away from is not
		// resurrected as an error they have to dismiss.
		XCTAssertTrue(model.state.sendFiles.isEmpty)
	}

	func testCancelledResultDoesNotDisturbANewerSend() async {
		gateway.savedDevices = [savedDevice()]
		gateway.createTargetedTransferResult = .success(transfer(id: "t-inflight"))
		let model = await makeModel()
		await stalledSend(model)
		model.abandonSend()

		// The user starts composing again while the first call is still pending.
		model.beginSend(to: Self.peer)
		model.onSendFilesPicked([picked("b.txt")])
		gateway.releaseTargetedCreate()
		await waitUntil { !self.fileSystem.discardedFiles.isEmpty }

		XCTAssertEqual(model.state.sendTargetPeerId, Self.peer)
		XCTAssertEqual(model.state.sendFiles.map(\.value), ["/tmp/b.txt"])
		XCTAssertFalse(model.state.isCreatingSend)
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
