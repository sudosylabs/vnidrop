import Foundation

struct BugReportDraft: Equatable, Sendable {
	let whatHappened: String
	let expected: String
	let steps: String
	let contact: String
	let includeLogs: Bool
}

@MainActor
protocol BugReportService {
	func submit(_ draft: BugReportDraft, deviceInfo: DeviceInfo?) async -> Result<Void, Error>
	func previewLogBytes() async -> Int
}

enum BugReportError: Error, Equatable {
	case notConfigured
	case missingWhat
	case missingExpected
	case textTooLong
	case invalidPayload
	case serviceConfiguration
	case rateLimited
	case serverError
	case unconfirmed
	case timedOut
	case connectionFailed
}

@MainActor
final class DiagnosticsBugReportService: BugReportService {
	private let configuration: BugReportConfiguration?
	private let environment: PlatformEnvironment
	private let preferences: AppPreferencesRepository
	private let transport: any BugReportTransport
	private let logs: any BugReportLogReading
	private var pending: (draft: BugReportDraft, payload: BugReportPayload, body: Data)?
	private var isSubmitting = false

	init(
		configuration: BugReportConfiguration?,
		environment: PlatformEnvironment,
		preferences: AppPreferencesRepository,
		transport: any BugReportTransport = URLSessionBugReportTransport(),
		logs: any BugReportLogReading
	) {
		self.configuration = configuration
		self.environment = environment
		self.preferences = preferences
		self.transport = transport
		self.logs = logs
	}

	func submit(_ draft: BugReportDraft, deviceInfo: DeviceInfo?) async -> Result<Void, Error> {
		guard !isSubmitting else { return .failure(BugReportError.unconfirmed) }
		guard let configuration else { return .failure(BugReportError.notConfigured) }
		isSubmitting = true
		defer { isSubmitting = false }
		do {
			try Task.checkCancellation()
			try BugReportPayload.validate(draft)
			if pending?.draft != draft {
				let recentLogs = draft.includeLogs ? try await logs.recentLogs() : ""
				try Task.checkCancellation()
				let payload = BugReportPayload(
					draft: draft, installId: preferences.ensureDiagnosticsInstallId(),
					environment: environment, deviceInfo: deviceInfo, logs: recentLogs
				)
				pending = (draft, payload, try payload.encoded())
			}
			guard let report = pending else { throw BugReportError.invalidPayload }
			var request = URLRequest(url: configuration.submissionURL)
			request.httpMethod = "POST"
			request.httpBody = report.body
			request.setValue("application/json", forHTTPHeaderField: "Content-Type")
			request.setValue("application/json", forHTTPHeaderField: "Accept")
			request.setValue(configuration.ingestKey, forHTTPHeaderField: "X-VniDrop-Key")
			request.setValue(report.payload.installId, forHTTPHeaderField: "X-VniDrop-Install-Id")
			let response = try await transport.send(request)
			try Task.checkCancellation()
			try response.validate(reportId: report.payload.id)
			pending = nil
			return .success(())
		} catch let error as URLError {
			if error.code == .cancelled { return .failure(CancellationError()) }
			return .failure(error.code == .timedOut ? BugReportError.timedOut : BugReportError.connectionFailed)
		} catch {
			// Keep the exact body and ID after an uncertain delivery, including cancellation.
			return .failure(error)
		}
	}

	func previewLogBytes() async -> Int {
		(try? await logs.recentLogs().utf8.count) ?? 0
	}
}
