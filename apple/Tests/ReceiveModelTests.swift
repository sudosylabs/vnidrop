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
