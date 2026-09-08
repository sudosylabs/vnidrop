import XCTest
@testable import VniDrop

final class BugReportLogsTests: XCTestCase {
	private func makeDirectory() throws -> URL {
		let directory = FileManager.default.temporaryDirectory.appendingPathComponent("vnidrop-report-\(UUID().uuidString)")
		try FileManager.default.createDirectory(at: directory.appendingPathComponent("logs"), withIntermediateDirectories: true)
		addTeardownBlock { try FileManager.default.removeItem(at: directory) }
		return directory
	}

	func testRedactsSensitiveLogValuesAndKeepsDiagnosticEvents() {
		let secrets = [
			"vnd1:" + String(repeating: "b", count: 80), "vndaddr1:" + String(repeating: "c", count: 80),
			String(repeating: "e", count: 64), "/Users/Alice/Documents/private.png",
			"https://relay.example.test/path?secret=value", "alice@example.test", "192.168.20.44", "fe80::abcd:1234",
		]
		let input = "2026-09-08T00:15:24 transfer_failed phase=export\n" + secrets.joined(separator: "\n")
		let redacted = CoreBugReportLogs.redact(input)
		for secret in secrets { XCTAssertFalse(redacted.contains(secret), secret) }
		XCTAssertTrue(redacted.contains("transfer_failed phase=export"))
		XCTAssertTrue(redacted.contains("2026-09-08T00:15:24"))
		XCTAssertTrue(redacted.contains("[redacted-ticket]"))
		XCTAssertTrue(redacted.contains("[redacted-path]"))
		let fields = CoreBugReportLogs.redact("file_name=\"private picture.png\" phase=export\nendpoint_id=\(String(repeating: "Z", count: 52))")
		XCTAssertFalse(fields.contains("private picture.png"))
		XCTAssertFalse(fields.contains(String(repeating: "Z", count: 52)))
		XCTAssertTrue(fields.contains("phase=export"))
	}

	func testReadsOnlyCoreLogsInRotationOrder() async throws {
		let directory = try makeDirectory()
		let logs = directory.appendingPathComponent("logs")
		try "recent event\n".write(to: logs.appendingPathComponent("vnidrop.log"), atomically: true, encoding: .utf8)
		try "older event\n".write(to: logs.appendingPathComponent("vnidrop.1.log"), atomically: true, encoding: .utf8)
		try "must not attach".write(to: logs.appendingPathComponent("identity.key"), atomically: true, encoding: .utf8)
		let result = try await CoreBugReportLogs(dataDirectory: directory.path).recentLogs()
		XCTAssertEqual(result, "older event\n\nrecent event\n")
	}

	func testSkipsSymbolicLinksToFilesAndLogDirectories() async throws {
		let directory = try makeDirectory()
		let privateFile = directory.appendingPathComponent("private.txt")
		try "private contents".write(to: privateFile, atomically: true, encoding: .utf8)
		let logs = directory.appendingPathComponent("logs")
		try FileManager.default.createSymbolicLink(at: logs.appendingPathComponent("vnidrop.log"), withDestinationURL: privateFile)
		let fileResult = try await CoreBugReportLogs(dataDirectory: directory.path).recentLogs()
		XCTAssertEqual(fileResult, "")
		let aliasRoot = directory.appendingPathComponent("alias")
		try FileManager.default.createDirectory(at: aliasRoot, withIntermediateDirectories: true)
		try FileManager.default.createSymbolicLink(at: aliasRoot.appendingPathComponent("logs"), withDestinationURL: logs)
		let directoryResult = try await CoreBugReportLogs(dataDirectory: aliasRoot.path).recentLogs()
		XCTAssertEqual(directoryResult, "")
	}

	func testLargeLogDiscardsPartialFirstLineBeforeRedaction() async throws {
		let directory = try makeDirectory()
		let text = "vnd1:" + String(repeating: "x", count: BugReportPayload.maximumLogBytes * 5) + "\nrecent event\n"
		try text.write(to: directory.appendingPathComponent("logs/vnidrop.log"), atomically: true, encoding: .utf8)
		let result = try await CoreBugReportLogs(dataDirectory: directory.path).recentLogs()
		XCTAssertEqual(result, "recent event\n")
	}

	func testRecentLogAttachmentIsBounded() async throws {
		let directory = try makeDirectory()
		let text = String(repeating: "transfer progress é🎉\n", count: 50_000)
		try text.write(to: directory.appendingPathComponent("logs/vnidrop.log"), atomically: true, encoding: .utf8)
		let result = try await CoreBugReportLogs(dataDirectory: directory.path).recentLogs()
		XCTAssertLessThanOrEqual(result.utf8.count, BugReportPayload.maximumLogBytes)
		XCTAssertTrue(result.hasSuffix("transfer progress é🎉\n"))
		XCTAssertFalse(result.contains("�"))
	}
}
