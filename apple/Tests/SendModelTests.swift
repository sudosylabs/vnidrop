import XCTest
@testable import VniDrop

/// Ports the send-side state-machine assertions from `feature/ViewModelsTest.kt`.
@MainActor
final class SendModelTests: XCTestCase {

	private func makeModel(_ core: FakeCoreGateway) -> SendModel {
		SendModel(
			repository: core,
			fileSystemService: FakeFileSystemService(),
			preferences: Fixtures.preferences(),
			filePreviewRepository: FilePreviewRepository(appDataDir: NSTemporaryDirectory() + UUID().uuidString),
			messages: UiMessageController()
		)
	}

	func testOpenAndCloseTransferDetails() {
		let model = makeModel(FakeCoreGateway())
		model.openTransfer(3)
		XCTAssertEqual(model.state.selectedTransferId, 3)
		model.closeTransferDetails()
		XCTAssertNil(model.state.selectedTransferId)
	}

	func testDeleteTransferConfirmationFlow() async {
		let core = FakeCoreGateway()
		let model = makeModel(core)
		model.openTransfer(3)

		model.requestDeleteTransfer()
		XCTAssertTrue(model.state.isDeleteConfirmationOpen)

		model.confirmDeleteTransfer()
		// Must close immediately (not after the async delete) so the alert can't
		// re-present on macOS.
		XCTAssertFalse(model.state.isDeleteConfirmationOpen)
		await waitUntil { core.deletedTransfers.contains(3) }
		XCTAssertEqual(core.deletedTransfers, [3])
		XCTAssertNil(model.state.selectedTransferId)
		XCTAssertFalse(model.state.isDeleteConfirmationOpen)
	}

	func testStopSharingCancelsTheTransfer() async {
		let core = FakeCoreGateway()
		let model = makeModel(core)
		model.stopSharing(transferId: 4)
		await waitUntil { core.cancelledTransfers.contains(4) }
		XCTAssertEqual(core.cancelledTransfers, [4])
	}

	func testCancelReceiverRefusesTheRequest() async {
		let core = FakeCoreGateway()
		let model = makeModel(core)
		model.openTransfer(1)
		model.cancelReceiver(requestId: "req-1")
		await waitUntil { core.responses.contains { $0.id == "req-1" } }
		let response = core.responses.first { $0.id == "req-1" }
		XCTAssertEqual(response?.accepted, false)
	}

	func testOnlyActiveShareExposesStoredInvitationTicket() {
		XCTAssertEqual(
			Fixtures.transfer(id: 1, direction: .send, status: .sharing).invitationPresentation,
			.ready("ticket")
		)
		XCTAssertEqual(
			Fixtures.transfer(id: 2, direction: .send, status: .importing).invitationPresentation,
			.preparing
		)
		for status in [TransferStatus.stopped, .failed, .cancelled, .done] {
			XCTAssertEqual(
				Fixtures.transfer(id: 3, direction: .send, status: status).invitationPresentation,
				.unavailable
			)
		}
	}

	func testOversizedInvitationReportsQRCodeUnavailable() {
		XCTAssertNil(QRCode.generate(from: String(repeating: "x", count: 10_000)))
	}
}
