import XCTest
@testable import VniDrop

@MainActor
final class FakeBugReportService: BugReportService {
	var result: Result<Void, Error> = .success(())
	var drafts: [BugReportDraft] = []
	var suspend = false
	var continuation: CheckedContinuation<Result<Void, Error>, Never>?

	func submit(_ draft: BugReportDraft, deviceInfo: DeviceInfo?) async -> Result<Void, Error> {
		drafts.append(draft)
		if suspend { return await withCheckedContinuation { continuation = $0 } }
		return result
	}

	func previewLogBytes() async -> Int { 32 }
}

actor FakeBugReportLogs: BugReportLogReading {
	private(set) var reads = 0
	var text: String
	init(text: String = "safe log") { self.text = text }
	func recentLogs() -> String {
		reads += 1
		return text
	}
}

actor FakeBugReportTransport: BugReportTransport {
	private(set) var requests: [URLRequest] = []
	private var failures: [URLError.Code]
	init(failures: [URLError.Code] = []) { self.failures = failures }

	func send(_ request: URLRequest) throws -> BugReportResponse {
		requests.append(request)
		if !failures.isEmpty { throw URLError(failures.removeFirst()) }
		let report = try JSONSerialization.jsonObject(with: XCTUnwrap(request.httpBody)) as! [String: Any]
		return BugReportResponse(statusCode: 202, body: try JSONSerialization.data(withJSONObject: [
			"ok": true, "id": try XCTUnwrap(report["id"] as? String),
		]))
	}
}

@MainActor
final class BugReportServiceTests: XCTestCase {
	private let environment = PlatformEnvironment(name: "iOS", appVersion: "0.3.4", defaultCoreDataDir: "/unused")
	private let draft = BugReportDraft(
		whatHappened: " Transfer failed ", expected: " Files arrive ", steps: "1. Send", contact: "", includeLogs: false
	)

	private func makeService(
		transport: FakeBugReportTransport, logs: FakeBugReportLogs,
		preferences: AppPreferencesRepository? = nil, configured: Bool = true
	) throws -> DiagnosticsBugReportService {
		DiagnosticsBugReportService(
			configuration: configured ? try BugReportConfiguration(endpoint: "https://reports.example.test", ingestKey: "fixture") : nil,
			environment: environment, preferences: preferences ?? Fixtures.preferences(), transport: transport, logs: logs
		)
	}

	func testSubmissionUsesBackendContractAndDoesNotReadOptedOutLogs() async throws {
		let transport = FakeBugReportTransport()
		let logs = FakeBugReportLogs()
		let preferences = Fixtures.preferences()
		let service = try makeService(transport: transport, logs: logs, preferences: preferences)
		try await service.submit(draft, deviceInfo: nil).get()
		let requests = await transport.requests
		let request = try XCTUnwrap(requests.first)
		XCTAssertEqual(request.url?.absoluteString, "https://reports.example.test/v1/bugs")
		XCTAssertEqual(request.httpMethod, "POST")
		XCTAssertEqual(request.value(forHTTPHeaderField: "X-VniDrop-Key"), "fixture")
		XCTAssertEqual(request.value(forHTTPHeaderField: "X-VniDrop-Install-Id"), preferences.preferences.diagnosticsInstallId)
		let body = try JSONSerialization.jsonObject(with: XCTUnwrap(request.httpBody)) as! [String: Any]
		XCTAssertEqual(body["schemaVersion"] as? Int, 1)
		XCTAssertEqual(body["whatHappened"] as? String, "Transfer failed")
		XCTAssertEqual(body["expected"] as? String, "Files arrive")
		XCTAssertEqual(body["appVersion"] as? String, "0.3.4")
		XCTAssertEqual(body["platform"] as? String, "iOS")
		XCTAssertEqual(body["includeLogs"] as? Bool, false)
		XCTAssertEqual(body["logs"] as? String, "")
		let reads = await logs.reads
		XCTAssertEqual(reads, 0)
	}

	func testUncertainRetryReusesExactBodyAndSuccessStartsANewReport() async throws {
		let transport = FakeBugReportTransport(failures: [.timedOut])
		let logs = FakeBugReportLogs()
		let service = try makeService(transport: transport, logs: logs)
		let result = await service.submit(draft, deviceInfo: nil)
		if case .failure(let error) = result {
			XCTAssertEqual(error as? BugReportError, .timedOut)
		} else { XCTFail("The first request must fail") }
		try await service.submit(draft, deviceInfo: nil).get()
		try await service.submit(draft, deviceInfo: nil).get()
		let requests = await transport.requests
		XCTAssertEqual(requests.count, 3)
		XCTAssertEqual(requests[0].httpBody, requests[1].httpBody)
		XCTAssertNotEqual(requests[1].httpBody, requests[2].httpBody)
	}

	func testChangedDraftDropsPreviouslyAttachedLogsOnRetry() async throws {
		let transport = FakeBugReportTransport(failures: [.networkConnectionLost])
		let logs = FakeBugReportLogs()
		let service = try makeService(transport: transport, logs: logs)
		let withLogs = BugReportDraft(whatHappened: draft.whatHappened, expected: draft.expected, steps: draft.steps, contact: "", includeLogs: true)
		_ = await service.submit(withLogs, deviceInfo: nil)
		try await service.submit(draft, deviceInfo: nil).get()
		let requests = await transport.requests
		let first = try JSONSerialization.jsonObject(with: XCTUnwrap(requests[0].httpBody)) as! [String: Any]
		let second = try JSONSerialization.jsonObject(with: XCTUnwrap(requests[1].httpBody)) as! [String: Any]
		XCTAssertEqual(first["logs"] as? String, "safe log")
		XCTAssertEqual(second["logs"] as? String, "")
		XCTAssertNotEqual(first["id"] as? String, second["id"] as? String)
		let reads = await logs.reads
		XCTAssertEqual(reads, 1)
	}

	func testRetryDoesNotResampleAttachedLogsOrDeviceInformation() async throws {
		let transport = FakeBugReportTransport(failures: [.timedOut])
		let logs = FakeBugReportLogs()
		let service = try makeService(transport: transport, logs: logs)
		let withLogs = BugReportDraft(whatHappened: draft.whatHappened, expected: draft.expected, steps: "", contact: "", includeLogs: true)
		_ = await service.submit(withLogs, deviceInfo: nil)
		try await service.submit(withLogs, deviceInfo: DeviceInfo(
			deviceName: "Updated", deviceModel: "Test", operatingSystem: "TestOS", network: "Changed", batteryLevel: nil
		)).get()
		let requests = await transport.requests
		let reads = await logs.reads
		XCTAssertEqual(requests[0].httpBody, requests[1].httpBody)
		XCTAssertEqual(reads, 1)
	}

	func testInvalidDraftIsRejectedBeforeReadingLogsOrSending() async throws {
		let transport = FakeBugReportTransport()
		let logs = FakeBugReportLogs()
		let service = try makeService(transport: transport, logs: logs)
		let invalid = BugReportDraft(whatHappened: String(repeating: "é", count: 2001), expected: "Expected", steps: "", contact: "", includeLogs: true)
		let result = await service.submit(invalid, deviceInfo: nil)
		if case .failure(let error) = result { XCTAssertEqual(error as? BugReportError, .textTooLong) }
		else { XCTFail("Oversized text must not be silently truncated") }
		let requests = await transport.requests
		let reads = await logs.reads
		XCTAssertTrue(requests.isEmpty)
		XCTAssertEqual(reads, 0)
	}

	func testUnconfiguredBuildNeverReadsLogsOrCallsTransport() async throws {
		let transport = FakeBugReportTransport()
		let logs = FakeBugReportLogs()
		let service = try makeService(transport: transport, logs: logs, configured: false)
		let withLogs = BugReportDraft(whatHappened: draft.whatHappened, expected: draft.expected, steps: "", contact: "", includeLogs: true)
		let result = await service.submit(withLogs, deviceInfo: nil)
		if case .failure(let error) = result {
			XCTAssertEqual(error as? BugReportError, .notConfigured)
		} else { XCTFail("Unconfigured builds must not report success") }
		let requests = await transport.requests
		let reads = await logs.reads
		XCTAssertTrue(requests.isEmpty)
		XCTAssertEqual(reads, 0)
	}

	func testCancelledDeliveryPreservesReportForRetry() async throws {
		let transport = FakeBugReportTransport(failures: [.cancelled])
		let service = try makeService(transport: transport, logs: FakeBugReportLogs())
		let result = await service.submit(draft, deviceInfo: nil)
		if case .failure(let error) = result { XCTAssertTrue(error is CancellationError) }
		else { XCTFail("Cancelled delivery must not succeed") }
		try await service.submit(draft, deviceInfo: nil).get()
		let requests = await transport.requests
		XCTAssertEqual(requests[0].httpBody, requests[1].httpBody)
	}
}
