import XCTest
@testable import VniDrop

final class BugReportPayloadTests: XCTestCase {
	func testRequiredFieldsAndUTF8Limits() throws {
		let valid = BugReportDraft(whatHappened: String(repeating: "é", count: 2000), expected: "Expected", steps: "", contact: "", includeLogs: false)
		XCTAssertNoThrow(try BugReportPayload.validate(valid))
		let invalid = BugReportDraft(whatHappened: valid.whatHappened + "é", expected: "Expected", steps: "", contact: "", includeLogs: false)
		XCTAssertThrowsError(try BugReportPayload.validate(invalid)) {
			XCTAssertEqual($0 as? BugReportError, .textTooLong)
		}
		let empty = BugReportDraft(whatHappened: " \n", expected: "Expected", steps: "", contact: "", includeLogs: false)
		XCTAssertThrowsError(try BugReportPayload.validate(empty)) {
			XCTAssertEqual($0 as? BugReportError, .missingWhat)
		}
		let longContact = BugReportDraft(whatHappened: "Failed", expected: "Expected", steps: "", contact: String(repeating: "a", count: 321), includeLogs: false)
		XCTAssertThrowsError(try BugReportPayload.validate(longContact)) {
			XCTAssertEqual($0 as? BugReportError, .textTooLong)
		}
	}

	func testEncodedBodyStaysBoundedAfterJSONEscaping() throws {
		let draft = BugReportDraft(whatHappened: "Failed", expected: "Expected", steps: "", contact: "", includeLogs: true)
		let payload = BugReportPayload(
			draft: draft, installId: UUID().uuidString,
			environment: PlatformEnvironment(name: "macOS", appVersion: "0.3.4", defaultCoreDataDir: "/unused"),
			deviceInfo: nil, logs: String(repeating: "\u{0001}", count: BugReportPayload.maximumLogBytes)
		)
		let body = try payload.encoded()
		XCTAssertLessThanOrEqual(body.count, BugReportPayload.maximumBodyBytes)
		let object = try JSONSerialization.jsonObject(with: body) as! [String: Any]
		let logs = try XCTUnwrap(object["logs"] as? String)
		XCTAssertFalse(logs.isEmpty)
		XCTAssertEqual(object["whatHappened"] as? String, "Failed")
	}

	func testTruncationKeepsCompleteUTF8Scalars() {
		XCTAssertEqual(BugReportPayload.prefix("a🎉b", maximumBytes: 4), "a")
		XCTAssertEqual(BugReportPayload.tail("a🎉b", maximumBytes: 4), "b")
		XCTAssertEqual(BugReportPayload.prefix("éé", maximumBytes: 3), "é")
		XCTAssertEqual(BugReportPayload.tail("éé", maximumBytes: 3), "é")
	}

	func testConfigurationRejectsUnsafeEndpointsAndHeaders() throws {
		for endpoint in [
			"", "http://reports.example.test", "https://", "https://user:pass@reports.example.test",
			"https://reports.example.test?token=secret", "https://reports.example.test/#secret",
			"https://reports.example.test/line\nbreak",
		] {
			XCTAssertThrowsError(try BugReportConfiguration(endpoint: endpoint, ingestKey: "fixture"), endpoint)
		}
		for key in ["", " ", "key\r\nheader", "key\theader", "clé"] {
			XCTAssertThrowsError(try BugReportConfiguration(endpoint: "https://reports.example.test", ingestKey: key))
		}
		let config = try BugReportConfiguration(endpoint: "https://reports.example.test/prefix/", ingestKey: "fixture")
		XCTAssertEqual(config.submissionURL.absoluteString, "https://reports.example.test/prefix/v1/bugs")
	}
}
