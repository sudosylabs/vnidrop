package com.vnidrop.app.diagnostics

import com.vnidrop.app.preferences.PreferencesRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

/**
 * Owns user-initiated bug reports for the app process.
 *
 * Telemetry and crash auto-reporting were removed; only [bugReports] remains,
 * and it only sends when the user submits a report from Settings.
 */
class DiagnosticsCoordinator(
	val preferencesRepository: PreferencesRepository,
	val transport: DiagnosticsTransport,
	val bugReports: BugReportService,
	private val scope: CoroutineScope,
) {
	fun start() {
		// Install id is useful for bug-report correlation.
		scope.launch {
			preferencesRepository.ensureDiagnosticsInstallId()
		}
	}

	companion object {
		fun create(
			appVersion: String,
			platform: String,
			preferencesRepository: PreferencesRepository,
			scope: CoroutineScope,
			transport: DiagnosticsTransport = NoOpDiagnosticsTransport(),
		): DiagnosticsCoordinator {
			val bugReports = BugReportService(
				preferencesRepository = preferencesRepository,
				transport = transport,
				appVersion = appVersion,
				platform = platform,
			)
			return DiagnosticsCoordinator(
				preferencesRepository = preferencesRepository,
				transport = transport,
				bugReports = bugReports,
				scope = scope,
			)
		}
	}
}
