import Foundation

protocol BugReportTransport: Sendable {
	func send(_ request: URLRequest) async throws -> BugReportResponse
}

struct BugReportResponse: Sendable {
	static let maximumBytes = 16 * 1024
	let statusCode: Int
	let body: Data

	func validate(reportId: String) throws {
		switch statusCode {
		case 200..<300: break
		case 401, 403, 404: throw BugReportError.serviceConfiguration
		case 429: throw BugReportError.rateLimited
		case 500..<600: throw BugReportError.serverError
		case 400, 413, 415, 422: throw BugReportError.invalidPayload
		default: throw BugReportError.unconfirmed
		}
		struct Acknowledgement: Decodable {
			let ok: Bool
			let id: String
		}
		guard body.count <= Self.maximumBytes,
			let ack = try? JSONDecoder().decode(Acknowledgement.self, from: body),
			ack.ok, ack.id == reportId else { throw BugReportError.unconfirmed }
	}
}

struct URLSessionBugReportTransport: BugReportTransport {
	private let configuration: URLSessionConfiguration

	init(configuration: URLSessionConfiguration = .ephemeral) {
		let configuration = configuration.copy() as! URLSessionConfiguration
		configuration.urlCache = nil
		configuration.httpCookieStorage = nil
		configuration.urlCredentialStorage = nil
		configuration.httpShouldSetCookies = false
		configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
		configuration.timeoutIntervalForRequest = 10
		configuration.timeoutIntervalForResource = 30
		self.configuration = configuration
	}

	func send(_ request: URLRequest) async throws -> BugReportResponse {
		let session = URLSession(configuration: configuration)
		defer { session.invalidateAndCancel() }
		let (bytes, response) = try await session.bytes(for: request, delegate: BugReportRedirectDelegate())
		defer { bytes.task.cancel() }
		guard let response = response as? HTTPURLResponse else { throw BugReportError.unconfirmed }
		guard (200..<300).contains(response.statusCode) else {
			return BugReportResponse(statusCode: response.statusCode, body: Data())
		}
		guard response.expectedContentLength <= Int64(BugReportResponse.maximumBytes) else {
			throw BugReportError.unconfirmed
		}
		return try await withTaskCancellationHandler {
			var body = Data()
			for try await byte in bytes {
				try Task.checkCancellation()
				guard body.count < BugReportResponse.maximumBytes else { throw BugReportError.unconfirmed }
				body.append(byte)
			}
			return BugReportResponse(statusCode: response.statusCode, body: body)
		} onCancel: {
			bytes.task.cancel()
		}
	}
}

final class BugReportRedirectDelegate: NSObject, URLSessionTaskDelegate {
	func urlSession(
		_ session: URLSession, task: URLSessionTask,
		willPerformHTTPRedirection response: HTTPURLResponse, newRequest request: URLRequest
	) async -> URLRequest? {
		// An ingestion key and user report must never be forwarded to another endpoint.
		nil
	}
}
