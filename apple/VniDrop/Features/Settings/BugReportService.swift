import Foundation

/// Bug-report draft, ported from `diagnostics/BugReportService.kt`.
struct BugReportDraft {
	let whatHappened: String
	let expected: String
	let steps: String
	let contact: String
	let includeLogs: Bool
}

/// Bug-report submission. The full diagnostics transport (URLSession + build
/// config) lands in the diagnostics phase; this protocol is the stable seam.
@MainActor
protocol BugReportService {
	func submit(_ draft: BugReportDraft, deviceInfo: DeviceInfo?) async -> Result<Void, Error>
	func previewLogBytes() async -> Int
}

/// Offline-safe no-op used until the diagnostics transport is configured.
struct NoopBugReportService: BugReportService {
	func submit(_ draft: BugReportDraft, deviceInfo: DeviceInfo?) async -> Result<Void, Error> {
		.failure(InvitationError.bugReportingUnavailable)
	}
	func previewLogBytes() async -> Int { 0 }
}
