import XCTest
import Combine
@testable import VniDrop

/// Ports `feature/approvals/ApprovalCoordinatorTest.kt` (the gateway-observable
/// parts; notification assertions require a notification-service seam we don't
/// have on Apple yet).
@MainActor
final class ApprovalCoordinatorTests: XCTestCase {

	private func makeCoordinator(_ core: FakeCoreGateway) -> ApprovalCoordinator {
		ApprovalCoordinator(
			repository: core,
			notifications: LocalNotificationService(),
			visibility: AppVisibility(),
			messages: UiMessageController()
		)
	}

	func testOrdersPendingRequestsByRequestedAt() async {
		let core = FakeCoreGateway()
		core.requests[1] = [Fixtures.request(id: "new", requestedAt: 20),
							 Fixtures.request(id: "old", requestedAt: 10)]
		let coordinator = makeCoordinator(core)

		core.setState(CoreState(isInitialized: true, transfers: [Fixtures.transfer(id: 1, direction: .send, status: .sharing)]))
		core.emit(.approvalChanged(transferId: 1))

		await waitUntil { !coordinator.state.pending.isEmpty }
		XCTAssertEqual(coordinator.state.pending.map(\.id), ["old", "new"])
		XCTAssertEqual(coordinator.state.current?.id, "old")
	}

	func testFailedResponseKeepsRequestVisibleAndClearsResponding() async {
		let core = FakeCoreGateway()
		core.requests[1] = [Fixtures.request(id: "request", requestedAt: 10)]
		core.responseResult = .failure(TestError.unimplemented)
		let coordinator = makeCoordinator(core)

		core.setState(CoreState(isInitialized: true, transfers: [Fixtures.transfer(id: 1, direction: .send, status: .sharing)]))
		core.emit(.approvalChanged(transferId: 1))
		await waitUntil { coordinator.state.pending.contains { $0.id == "request" } }

		coordinator.accept("request")
		await waitUntil { coordinator.state.respondingIds.isEmpty && core.responses.count == 1 }

		XCTAssertTrue(coordinator.state.pending.contains { $0.id == "request" })
		XCTAssertTrue(coordinator.state.respondingIds.isEmpty)
		XCTAssertEqual(core.responses.first?.accepted, true)
	}

	func testAcceptRespondsPositivelyAndSingleFlights() async {
		let core = FakeCoreGateway()
		core.requests[1] = [Fixtures.request(id: "request", requestedAt: 10)]
		let coordinator = makeCoordinator(core)
		core.setState(CoreState(isInitialized: true, transfers: [Fixtures.transfer(id: 1, direction: .send, status: .sharing)]))
		core.emit(.approvalChanged(transferId: 1))
		await waitUntil { coordinator.state.current != nil }

		coordinator.accept("request")
		coordinator.accept("request") // second call must be ignored (single-flight)
		await waitUntil { core.responses.count >= 1 }
		try? await Task.sleep(nanoseconds: 50_000_000)

		XCTAssertEqual(core.responses.count, 1)
		XCTAssertEqual(core.responses.first?.id, "request")
	}
}
