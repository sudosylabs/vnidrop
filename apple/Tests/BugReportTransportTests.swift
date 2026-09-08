import XCTest
@testable import VniDrop

final class BugReportTransportTests: XCTestCase {
	func testAcknowledgementRequiresMatchingTopLevelIDAndBooleanSuccess() throws {
		let reportId = UUID().uuidString
		for body in [
			"", "{}", "{\"ok\":true,\"id\":\"wrong\"}", "{\"ok\":false,\"id\":\"\(reportId)\"}",
			"{\"ok\":1,\"id\":\"\(reportId)\"}", "[{\"ok\":true,\"id\":\"\(reportId)\"}]",
			"{\"nested\":{\"ok\":true,\"id\":\"\(reportId)\"}}",
		] {
			XCTAssertThrowsError(try BugReportResponse(statusCode: 202, body: Data(body.utf8)).validate(reportId: reportId)) {
				XCTAssertEqual($0 as? BugReportError, .unconfirmed)
			}
		}
		let valid = Data("{\"ok\":true,\"id\":\"\(reportId)\"}".utf8)
		XCTAssertNoThrow(try BugReportResponse(statusCode: 202, body: valid).validate(reportId: reportId))
	}

	func testHTTPFailuresMapToActionableErrors() {
		let cases: [(Int, BugReportError)] = [
			(401, .serviceConfiguration), (403, .serviceConfiguration), (404, .serviceConfiguration),
			(429, .rateLimited), (503, .serverError), (413, .invalidPayload), (302, .unconfirmed),
		]
		for (status, error) in cases {
			XCTAssertThrowsError(try BugReportResponse(statusCode: status, body: Data()).validate(reportId: "id")) {
				XCTAssertEqual($0 as? BugReportError, error)
			}
		}
	}

	func testURLSessionReadsAnInterceptedResponseWithoutNetworkAccess() async throws {
		let body = Data(#"{"ok":true,"id":"fixture-id"}"#.utf8)
		let (transport, request) = fixture(.response(202, body))
		let response = try await transport.send(request)
		XCTAssertEqual(response.statusCode, 202)
		XCTAssertEqual(response.body, body)
		try response.validate(reportId: "fixture-id")
	}

	func testURLSessionRejectsOversizedResponseWithoutContentLength() async throws {
		let (transport, request) = fixture(.response(202, Data(count: BugReportResponse.maximumBytes + 1)))
		do {
			_ = try await transport.send(request)
			XCTFail("Unbounded acknowledgement must not be accepted")
		} catch { XCTAssertEqual(error as? BugReportError, .unconfirmed) }
	}

	func testURLSessionPreservesTimeoutFailure() async throws {
		let (transport, request) = fixture(.failure(.timedOut))
		do {
			_ = try await transport.send(request)
			XCTFail("A timeout must not succeed")
		} catch { XCTAssertEqual((error as? URLError)?.code, .timedOut) }
	}

	func testRedirectDelegateRefusesToForwardReportCredentials() async throws {
		let origin = URL(string: "https://reports.example.test")!
		let redirected = URL(string: "https://another.example.test")!
		let session = URLSession(configuration: .ephemeral)
		defer { session.invalidateAndCancel() }
		let task = session.dataTask(with: origin)
		let request = await BugReportRedirectDelegate().urlSession(
			session, task: task,
			willPerformHTTPRedirection: HTTPURLResponse(url: origin, statusCode: 307, httpVersion: nil, headerFields: nil)!,
			newRequest: URLRequest(url: redirected)
		)
		XCTAssertNil(request)
	}

	@MainActor
	func testCancellationStopsAnInFlightURLSessionRequest() async throws {
		let (transport, request) = fixture(.stall)
		let task = Task { try await transport.send(request) }
		let url = try XCTUnwrap(request.url)
		await waitUntil { BugReportURLProtocol.registry.hasStarted(url) }
		XCTAssertTrue(BugReportURLProtocol.registry.hasStarted(url))
		task.cancel()
		do {
			_ = try await task.value
			XCTFail("Cancellation must not succeed")
		} catch {
			XCTAssertTrue(error is CancellationError || (error as? URLError)?.code == .cancelled)
		}
		await waitUntil { BugReportURLProtocol.registry.hasStopped(url) }
		XCTAssertTrue(BugReportURLProtocol.registry.hasStopped(url))
	}

	private func fixture(_ response: BugReportURLProtocol.Fixture) -> (URLSessionBugReportTransport, URLRequest) {
		let url = URL(string: "https://reports.example.test/\(UUID().uuidString)")!
		BugReportURLProtocol.registry.register(url, response)
		addTeardownBlock { BugReportURLProtocol.registry.remove(url) }
		let configuration = URLSessionConfiguration.ephemeral
		configuration.protocolClasses = [BugReportURLProtocol.self]
		return (URLSessionBugReportTransport(configuration: configuration), URLRequest(url: url))
	}
}

private final class BugReportURLProtocol: URLProtocol, @unchecked Sendable {
	enum Fixture: Sendable {
		case response(Int, Data)
		case failure(URLError.Code)
		case stall
	}

	final class Registry: @unchecked Sendable {
		private let lock = NSLock()
		private var fixtures: [URL: Fixture] = [:]
		private var started = Set<URL>()
		private var stopped = Set<URL>()

		func register(_ url: URL, _ fixture: Fixture) { lock.withLock { fixtures[url] = fixture } }
		func remove(_ url: URL) {
			lock.withLock { fixtures[url] = nil; started.remove(url); stopped.remove(url) }
		}
		func start(_ url: URL) -> Fixture? {
			lock.withLock { started.insert(url); return fixtures[url] }
		}
		func stop(_ url: URL) { _ = lock.withLock { stopped.insert(url) } }
		func hasStarted(_ url: URL) -> Bool { lock.withLock { started.contains(url) } }
		func hasStopped(_ url: URL) -> Bool { lock.withLock { stopped.contains(url) } }
	}

	static let registry = Registry()
	override class func canInit(with request: URLRequest) -> Bool { true }
	override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

	override func startLoading() {
		guard let url = request.url, let fixture = Self.registry.start(url) else {
			client?.urlProtocol(self, didFailWithError: URLError(.unsupportedURL))
			return
		}
		switch fixture {
		case .response(let status, let body):
			let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: "HTTP/1.1", headerFields: nil)!
			client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
			client?.urlProtocol(self, didLoad: body)
			client?.urlProtocolDidFinishLoading(self)
		case .failure(let code): client?.urlProtocol(self, didFailWithError: URLError(code))
		case .stall: break
		}
	}

	override func stopLoading() {
		if let url = request.url { Self.registry.stop(url) }
	}
}
