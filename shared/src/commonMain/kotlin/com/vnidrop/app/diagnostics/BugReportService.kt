package com.vnidrop.app.diagnostics

import com.vnidrop.app.DeviceInfo
import com.vnidrop.app.logging.AppLogger
import com.vnidrop.app.logging.platformNowMillis
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.util.randomUuidString
import kotlinx.coroutines.CancellationException

data class BugReportDraft(
	val whatHappened: String,
	val expected: String,
	val steps: String = "",
	val contact: String = "",
	val includeLogs: Boolean = true,
)

/**
 * User-initiated bug reports. Always allowed regardless of diagnostics opt-in.
 */
class BugReportService(
	private val preferencesRepository: PreferencesRepository,
	private val transport: DiagnosticsTransport,
	private val appVersion: String,
	private val platform: String,
	private val logReader: () -> String = {
		AppLogger.readLatestLogs(AppLogger.DefaultBugReportLogBytes)
	},
) {
	fun assemble(
		draft: BugReportDraft,
		deviceInfo: DeviceInfo?,
		installId: String,
	): BugReport {
		val logs = if (draft.includeLogs) readReportLogs() else ""
		return BugReport(
			id = randomUuidString(),
			timestampMillis = platformNowMillis(),
			installId = sanitizeDiagnosticsInstallId(installId),
			appVersion = appVersion.takeUtf8Bytes(DiagnosticsJson.MaxAppVersionBytes),
			platform = platform.takeUtf8Bytes(DiagnosticsJson.MaxPlatformBytes),
			whatHappened = draft.whatHappened.trim().takeUtf8Bytes(MaxFieldBytes),
			expected = draft.expected.trim().takeUtf8Bytes(MaxFieldBytes),
			steps = draft.steps.trim().takeUtf8Bytes(MaxFieldBytes),
			contact = draft.contact.trim().takeUtf8Bytes(MaxContactBytes),
			includeLogs = draft.includeLogs,
			logs = logs,
			device = DeviceSnapshot(
				deviceName = deviceInfo?.deviceName?.takeUtf8Bytes(128),
				deviceModel = deviceInfo?.deviceModel?.takeUtf8Bytes(128),
				operatingSystem = (deviceInfo?.operatingSystem ?: platform).takeUtf8Bytes(192),
				network = deviceInfo?.network?.takeUtf8Bytes(96),
				batteryLevel = deviceInfo?.batteryLevel?.takeUtf8Bytes(64),
			),
		)
	}

	suspend fun submit(draft: BugReportDraft, deviceInfo: DeviceInfo?): Result<BugReport> {
		val what = draft.whatHappened.trim()
		val expected = draft.expected.trim()
		if (what.isEmpty()) {
			return Result.failure(IllegalArgumentException("Describe what happened."))
		}
		if (expected.isEmpty()) {
			return Result.failure(IllegalArgumentException("Describe what you expected."))
		}
		val installId = preferencesRepository.ensureDiagnosticsInstallId()
		val report = assemble(draft, deviceInfo, installId)
		val send = try {
			transport.sendBugReport(report)
		} catch (cancelled: CancellationException) {
			throw cancelled
		} catch (error: Throwable) {
			Result.failure(error)
		}
		return send.fold(
			onSuccess = {
				AppLogger.info("bug_report", "submitted", mapOf("id" to report.id))
				Result.success(report)
			},
			onFailure = { error ->
				AppLogger.error("bug_report", "submit failed", error)
				Result.failure(error)
			},
		)
	}

	fun previewLogBytes(): Int = readReportLogs().encodeToByteArray().size

	private fun readReportLogs(): String =
		LogRedactor.redact(logReader()).takeUtf8Bytes(MaxLogBytes)

	companion object {
		internal const val MaxLogBytes = 192 * 1024
		private const val MaxFieldBytes = 4_000
		private const val MaxContactBytes = 320
	}
}
