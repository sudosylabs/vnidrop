package com.vnidrop.app.diagnostics

/** Network boundary for bug reports; keep validation client-side. */
interface DiagnosticsTransport {
	suspend fun sendBugReport(report: BugReport): Result<Unit>
}

internal class DiagnosticsUnavailableException : IllegalStateException("diagnostics delivery is not configured")

/** Fails delivery without leaving the device. Used until a remote endpoint is configured. */
class NoOpDiagnosticsTransport : DiagnosticsTransport {
	override suspend fun sendBugReport(report: BugReport): Result<Unit> =
		Result.failure(DiagnosticsUnavailableException())
}

/**
 * Test double that records calls and can fail on demand.
 */
class RecordingDiagnosticsTransport : DiagnosticsTransport {
	val bugReports = mutableListOf<BugReport>()
	var bugResult: Result<Unit> = Result.success(Unit)

	override suspend fun sendBugReport(report: BugReport): Result<Unit> {
		bugReports += report
		return bugResult
	}
}
