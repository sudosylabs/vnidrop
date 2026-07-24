import XCTest
@testable import VniDrop

/// Ports selected `shared/src/commonTest/.../ui/state` assertions to verify the
/// progress-derivation logic matches the Kotlin implementation.
final class ProgressDerivationTests: XCTestCase {

	func testFormatBytes() {
		XCTAssertEqual(formatBytes(0), "0 B")
		XCTAssertEqual(formatBytes(1023), "1023 B")
		XCTAssertEqual(formatBytes(1024), "1.0 KB")
		XCTAssertEqual(formatBytes(1536), "1.5 KB")
		XCTAssertEqual(formatBytes(1024 * 1024), "1.0 MB")
	}

	func testWindowClassThresholds() {
		XCTAssertEqual(windowClassFor(width: 320), .phone)
		XCTAssertEqual(windowClassFor(width: 599), .phone)
		XCTAssertEqual(windowClassFor(width: 600), .tablet)
		XCTAssertEqual(windowClassFor(width: 919), .tablet)
		XCTAssertEqual(windowClassFor(width: 920), .desktop)
	}

	func testParseProgressPrefersExported() {
		XCTAssertEqual(parseProgress("{\"exported\":50,\"file_size\":100}"), 0.5)
		XCTAssertEqual(parseProgress("{\"downloaded\":25,\"total_size\":100}"), 0.25)
		XCTAssertNil(parseProgress("{\"foo\":1}"))
		XCTAssertEqual(parseProgress("{\"offset\":200,\"size\":100}"), 1.0) // clamped
	}

	func testFindStringSkipsNull() {
		XCTAssertEqual(findString("{\"endpoint_id\":\"abc\"}", key: "endpoint_id"), "abc")
		XCTAssertNil(findString("{\"endpoint_id\":null}", key: "endpoint_id"))
		XCTAssertNil(findString("{\"endpoint_id\":123}", key: "endpoint_id"))
	}

	func testProgressForTransferUsesLatestNewestFirst() {
		let events = [
			event(phase: "import", kind: "copy-progress", json: "{\"exported\":30,\"file_size\":100}"),
			event(phase: "import", kind: "started", json: "{}"),
		]
		let progress = progressForTransfer(events: events, transferId: 1)
		XCTAssertEqual(progress?.labelKey, L10n.Progress.preparing)
		XCTAssertEqual(progress?.progress, 0.3)
	}

	func testReceiverCompletionAfterProgressIsTerminal() {
		let events = [
			receiverEvent(kind: "completed", json: "{\"connection_id\":1,\"request_id\":1,\"endpoint_id\":\"peer-a\"}"),
			receiverEvent(kind: "progress", json: "{\"connection_id\":1,\"request_id\":1,\"endpoint_id\":\"peer-a\",\"end_offset\":100}"),
			receiverEvent(kind: "started", json: "{\"connection_id\":1,\"request_id\":1,\"endpoint_id\":\"peer-a\",\"size\":100}"),
		]

		let progress = progressForReceiver(
			events: events,
			transferId: 1,
			remoteEndpointId: "peer-a",
			totalSizeHint: 100
		)
		XCTAssertEqual(progress?.kind, .completed)
		XCTAssertEqual(progress?.labelKey, L10n.Progress.completed)
		XCTAssertEqual(progress?.progress, 1)
	}

	func testStatusLabelKeys() {
		XCTAssertEqual(statusLabelKey(.sharing), L10n.Status.available)
		XCTAssertEqual(statusLabelKey(.receiving), L10n.Status.receiving)
		XCTAssertEqual(statusLabelKey(.done), L10n.Status.completed)
	}

	private func event(phase: String, kind: String, json: String) -> CoreEventModel {
		CoreEventModel(
			id: UUID().uuidString, timestamp: 0, scope: "transfer", transferId: 1,
			direction: "send", phase: phase, kind: kind, dataJson: json
		)
	}

	private func receiverEvent(kind: String, json: String) -> CoreEventModel {
		event(phase: "transfer", kind: kind, json: json)
	}
}
