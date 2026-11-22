import XCTest
@testable import VniDrop

/// Ports the receive-side state-machine assertions from `feature/ViewModelsTest.kt`.
@MainActor
final class ReceiveModelTests: XCTestCase {

	private func makeModel(_ core: FakeCoreGateway) -> ReceiveModel {
		ReceiveModel(
			repository: core,
			fileSystemService: FakeFileSystemService(),
			preferences: Fixtures.preferences(),
			messages: UiMessageController()
		)
	}

	func testDeleteHistoryItemDeletesTerminalReceiveTransfer() async {
		let core = FakeCoreGateway()
		let model = makeModel(core)
		core.setState(CoreState(isInitialized: true, transfers: [Fixtures.transfer(id: 5, direction: .receive, status: .done)]))
		await waitUntil { model.coreState.transfers.contains { $0.transferId == 5 } }

		model.requestDeleteHistoryItem(5)
		XCTAssertEqual(model.state.historyDeleteTarget, .transfer(transferId: 5))

		model.confirmHistoryDelete()
		// Must close immediately (not after the async delete) so the alert can't
		// re-present on macOS.
		XCTAssertNil(model.state.historyDeleteTarget)
		await waitUntil { core.deletedTransfers.contains(5) }
		XCTAssertEqual(core.deletedTransfers, [5])
		XCTAssertNil(model.state.historyDeleteTarget)
	}

	func testClearHistoryCallsClearReceiveHistory() async {
		let core = FakeCoreGateway()
		let model = makeModel(core)
		core.setState(CoreState(isInitialized: true, transfers: [Fixtures.transfer(id: 5, direction: .receive, status: .done)]))
		await waitUntil { !model.coreState.transfers.isEmpty }

		model.requestClearHistory()
		XCTAssertEqual(model.state.historyDeleteTarget, .all)

		model.confirmHistoryDelete()
		await waitUntil { core.clearReceiveHistoryCount == 1 }
		XCTAssertEqual(core.clearReceiveHistoryCount, 1)
		XCTAssertNil(model.state.historyDeleteTarget)
	}

	func testDeleteHistoryItemIgnoresNonTerminalTransfer() async {
		let core = FakeCoreGateway()
		let model = makeModel(core)
		core.setState(CoreState(isInitialized: true, transfers: [Fixtures.transfer(id: 9, direction: .receive, status: .receiving)]))
		await waitUntil { !model.coreState.transfers.isEmpty }

		model.requestDeleteHistoryItem(9)
		XCTAssertNil(model.state.historyDeleteTarget) // in-flight receive can't be deleted from history
	}

	func testSupersededInvitationInspectionCannotReplaceTheCurrentTicketMetadata() async {
		let core = FakeCoreGateway()
		let model = makeModel(core)
		func inspection(_ id: UInt64) -> TicketInspectionModel {
			TicketInspectionModel(kind: "vnidrop", metadata: TransferMetadataModel(
				transferId: id, transferName: "Test", senderName: nil,
				contentHash: "hash", fileCount: 1, totalSize: 1
			))
		}
		var first: CheckedContinuation<Result<TicketInspectionModel, Error>, Never>?
		core.inspectHook = { ticket in
			if ticket == "first" { return await withCheckedContinuation { first = $0 } }
			return .success(inspection(2))
		}

		model.onInvitationResult(.invitationFile, .success("first"))
		await waitUntil { first != nil }
		model.onInvitationResult(.invitationFile, .success("second"))
		await waitUntil { model.state.inspection?.metadata.transferId == 2 }
		first?.resume(returning: .success(inspection(1)))
		await Task.yield()

		XCTAssertEqual(model.state.ticket, "second")
		XCTAssertEqual(model.state.inspection?.metadata.transferId, 2)
	}

	func testExternalInvitationWaitsForCoreStartup() async {
		let core = FakeCoreGateway()
		let model = makeModel(core)
		var ready = false
		let pending = Task { ready = await model.waitUntilReadyForExternalInvitation() }
		await Task.yield()
		XCTAssertFalse(ready)
		core.setState(CoreState(isInitialized: true))
		await pending.value
		XCTAssertTrue(ready)
	}

	func testCancelWaitsForTheInspectedReceiveToRegister() async {
		let core = FakeCoreGateway()
		let model = makeModel(core)
		core.setState(CoreState(isInitialized: true))
		core.inspectionResult = .success(TicketInspectionModel(
			kind: "vnidrop",
			metadata: TransferMetadataModel(
				transferId: 7, transferName: "Test", senderName: nil,
				contentHash: "hash", fileCount: 1, totalSize: 1
			)
		))
		var receiveContinuation: CheckedContinuation<Void, Never>?
		core.receiveGate = { await withCheckedContinuation { receiveContinuation = $0 } }
		model.onInvitationResult(.invitationFile, .success("ticket"))
		await waitUntil { model.state.inspection != nil && model.state.receiveFolder != nil }
		model.receive()
		await waitUntil { receiveContinuation != nil }
		defer { receiveContinuation?.resume() }

		core.emit(.transfersChanged(transferId: 88))
		model.cancelActiveReceive()
		XCTAssertTrue(core.cancelledTransfers.isEmpty)
		core.emit(.transfersChanged(transferId: 7, receiveStarted: true))
		await waitUntil { core.cancelledTransfers == [7] }
		XCTAssertEqual(core.cancelledTransfers, [7])
	}

	func testCancelActiveReceiveCancelsTheReceivingTransfer() async {
		let core = FakeCoreGateway()
		let model = makeModel(core)
		core.setState(CoreState(isInitialized: true, transfers: [Fixtures.transfer(id: 7, direction: .receive, status: .receiving)]))
		await waitUntil { !model.coreState.transfers.isEmpty }

		model.cancelActiveReceive()
		await waitUntil { core.cancelledTransfers.contains(7) }
		XCTAssertEqual(core.cancelledTransfers, [7])
	}
}
