import XCTest
@testable import VniDrop

@MainActor
final class TransferNotificationTests: XCTestCase {

	func testTransferNotificationsFireForTerminalStatesOnly() {
		let transfers = [
			Fixtures.transfer(id: 1, direction: .send, status: .failed),
			Fixtures.transfer(id: 2, direction: .receive, status: .done),
			Fixtures.transfer(id: 3, direction: .receive, status: .failed),
			Fixtures.transfer(id: 4, direction: .receive, status: .receiving),  // in-flight, ignored
			Fixtures.transfer(id: 5, direction: .send, status: .sharing),       // active share, ignored
			Fixtures.transfer(id: 6, direction: .send, status: .done),          // send-done isn't notified
		]
		let planned = plannedTransferNotifications(transfers, published: [])
		XCTAssertEqual(planned.map(\.kind), [.sendFailed, .receiveCompleted, .receiveFailed])
		XCTAssertEqual(planned.map(\.id), ["send-failed-1", "receive-completed-2", "receive-failed-3"])
		XCTAssertEqual(planned.first?.transferName, "Photos")
	}

	func testTransferNotificationsSkipAlreadyPublished() {
		let transfers = [Fixtures.transfer(id: 2, direction: .receive, status: .done)]
		XCTAssertTrue(plannedTransferNotifications(transfers, published: ["receive-completed-2"]).isEmpty)
	}

	func testReceiverNotificationsFireOnlyForCompletedReceivers() {
		let requests = [
			Fixtures.request(id: "a", requestedAt: 1, status: .completed),
			Fixtures.request(id: "b", requestedAt: 2, status: .accepted),
			Fixtures.request(id: "c", requestedAt: 3, status: .requested),
		]
		let planned = plannedReceiverNotifications(requests, published: [])
		XCTAssertEqual(planned.map(\.id), ["receiver-completed-a"])
		XCTAssertEqual(planned.first?.kind, .receiverCompleted)
		XCTAssertEqual(planned.first?.receiver, "Peer")
		XCTAssertEqual(planned.first?.transferName, "Photos")
	}

	func testReceiverNotificationsSkipAlreadyPublished() {
		let requests = [Fixtures.request(id: "a", requestedAt: 1, status: .completed)]
		XCTAssertTrue(plannedReceiverNotifications(requests, published: ["receiver-completed-a"]).isEmpty)
	}

	func testReceiverNotificationsFireForFailedReceivers() {
		let requests = [Fixtures.request(id: "x", requestedAt: 1, status: .failed)]
		let planned = plannedReceiverNotifications(requests, published: [])
		XCTAssertEqual(planned.map(\.id), ["receiver-failed-x"])
		XCTAssertEqual(planned.first?.kind, .receiverFailed)
	}
}
