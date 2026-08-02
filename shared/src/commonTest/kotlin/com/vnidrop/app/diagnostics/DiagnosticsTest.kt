package com.vnidrop.app.diagnostics

import com.vnidrop.app.DeviceInfo
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.preferences.AppPreferences
import com.vnidrop.app.support.FakePreferencesRepository
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class DiagnosticsTest {
	@Test
	fun diagnosticsJsonKeepsEscapedBugPayloadWithinWorkerLimit() {
		val report = bugReport(
			logs = "\n".repeat(BugReportService.MaxLogBytes),
			includeLogs = true,
		)

		val body = DiagnosticsJson.bugBody(report)

		assertTrue(body.encodeToByteArray().size <= DiagnosticsJson.MaxRequestBytes)
		assertTrue(body.contains("\"includeLogs\":true"))
		assertTrue(body.endsWith("}"))
	}

	@Test
	fun httpTransportPostsBugReportToExpectedPath() = runTest {
		val calls = mutableListOf<Pair<String, String>>()
		val transport = HttpDiagnosticsTransport(
			baseUrl = "https://diag.example",
			ingestKey = "secret",
			appVersion = "1.0",
			platform = "Test",
			post = { url, headers, body ->
				assertEquals("secret", headers["X-VniDrop-Key"])
				calls += url to body
				PlatformHttpResponse(202, """{"ok":true,"id":"b","stored":1}""")
			},
		)

		val bugResult = transport.sendBugReport(bugReport())

		assertTrue(bugResult.isSuccess)
		assertEquals("https://diag.example/v1/bugs", calls.single().first)
	}

	@Test
	fun httpTransportParsesEscapedAcknowledgementWithNestedUnknownFields() = runTest {
		val transport = HttpDiagnosticsTransport(
			baseUrl = "https://diag.example",
			ingestKey = "secret",
			post = { _, _, _ ->
				PlatformHttpResponse(
					202,
					"""{"ok":true,"id":"b","metadata":{"stored":1,"flags":[true,null]}}""",
				)
			},
		)

		assertTrue(transport.sendBugReport(bugReport()).isSuccess)
	}

	@Test
	fun httpTransportFailsOnHttpError() = runTest {
		val transport = HttpDiagnosticsTransport(
			baseUrl = "https://diag.example",
			ingestKey = "secret",
			post = { _, _, _ -> PlatformHttpResponse(401, """{"error":"unauthorized"}""") },
		)
		assertTrue(transport.sendBugReport(bugReport()).isFailure)
	}

	@Test
	fun httpTransportPropagatesCancellation() = runTest {
		val transport = HttpDiagnosticsTransport(
			baseUrl = "https://diag.example",
			ingestKey = "secret",
			post = { _, _, _ -> throw CancellationException("cancelled") },
		)

		assertFailsWith<CancellationException> {
			transport.sendBugReport(bugReport())
		}
	}

	@Test
	fun httpTransportRejectsMissingOrNegativeAcknowledgement() = runTest {
		val responses = ArrayDeque(
			listOf(
				PlatformHttpResponse(202, ""),
				PlatformHttpResponse(202, """{"ok":false}"""),
				PlatformHttpResponse(202, """{,"ok":true}"""),
				PlatformHttpResponse(202, """{"ok":true,"id":"different"}"""),
				PlatformHttpResponse(202, """{"ok":true,"id":"b","id":"b"}"""),
			),
		)
		val transport = HttpDiagnosticsTransport(
			baseUrl = "https://diag.example",
			ingestKey = "secret",
			post = { _, _, _ -> responses.removeFirst() },
		)

		repeat(5) {
			val result = transport.sendBugReport(bugReport())
			assertIs<DiagnosticsProtocolException>(result.exceptionOrNull())
		}
	}

	@Test
	fun httpTransportRejectsAmbiguousOrMalformedJsonAcknowledgement() = runTest {
		val responses = ArrayDeque(
			listOf(
				PlatformHttpResponse(202, """{"ok":true,"ok":true,"id":"b"}"""),
				PlatformHttpResponse(202, """{"ok":true,"id":true}"""),
				PlatformHttpResponse(202, """{"ok":true,"id":"b","stored":01}"""),
				PlatformHttpResponse(202, """{"ok":true,"id":"b",}"""),
				PlatformHttpResponse(202, """{"ok":true,"id":"b"} trailing"""),
			),
		)
		val transport = HttpDiagnosticsTransport(
			baseUrl = "https://diag.example",
			ingestKey = "secret",
			post = { _, _, _ -> responses.removeFirst() },
		)

		repeat(5) {
			val result = transport.sendBugReport(bugReport())
			assertIs<DiagnosticsProtocolException>(result.exceptionOrNull())
		}
	}

	@Test
	fun httpTransportRequiresHttpsExceptForLoopbackDevelopment() {
		assertFailsWith<IllegalArgumentException> {
			HttpDiagnosticsTransport("http://diag.example", "secret")
		}
		assertFailsWith<IllegalArgumentException> {
			HttpDiagnosticsTransport("http://[::1].example", "secret")
		}
		HttpDiagnosticsTransport("http://localhost:8787", "secret")
		HttpDiagnosticsTransport("http://127.0.0.1:8787", "secret")
		HttpDiagnosticsTransport("http://[::1]:8787", "secret")
		assertFailsWith<IllegalArgumentException> {
			HttpDiagnosticsTransport("https://diag.example", "")
		}
	}

	@Test
	fun createTransportIsNoOpForExplicitEmptyConfiguration() {
		val transport = buildDiagnosticsTransport(
			endpoint = "",
			ingestKey = "",
			appVersion = "1.0",
			platform = "Test",
			installIdProvider = { "id" },
		)
		assertIs<NoOpDiagnosticsTransport>(transport)
	}

	@Test
	fun excludedTransportIgnoresIncompleteConfiguration() {
		val transport = buildDiagnosticsTransport(
			included = false,
			endpoint = "https://diag.example",
			ingestKey = "",
			appVersion = "1.0",
			platform = "Test",
			installIdProvider = { "id" },
		)
		assertIs<NoOpDiagnosticsTransport>(transport)
	}

	@Test
	fun noOpTransportReportsUnavailableDelivery() = runTest {
		val result = NoOpDiagnosticsTransport().sendBugReport(bugReport())
		assertIs<DiagnosticsUnavailableException>(result.exceptionOrNull())
	}

	@Test
	fun logRedactorScrubsTicketsPathsAndEndpointIds() {
		val input = """
			ticket=abcdefghijklmnopqrstuvwxyz012345
			endpoint_id=peerABCDEFGHIJKLMNOP
			path=/Users/me/secret/file.bin
			uri=content://com.android.providers/downloads/1
			ok=value
		""".trimIndent()
		val redacted = LogRedactor.redact(input)
		assertFalse(redacted.contains("abcdefghijklmnopqrstuvwxyz012345"))
		assertFalse(redacted.contains("peerABCDEFGHIJKLMNOP"))
		assertFalse(redacted.contains("/users/me/secret/file.bin", ignoreCase = true))
		assertFalse(redacted.contains("content://"))
		assertTrue(redacted.contains("ok=value"))
		assertTrue(redacted.contains("[redacted-ticket]"))
		assertTrue(redacted.contains("[redacted-endpoint]"))
	}

	@Test
	fun breadcrumbBufferKeepsOnlyLatestEntries() {
		val buffer = BreadcrumbBuffer(capacity = 3)
		buffer.add("a")
		buffer.add("b")
		buffer.add("c")
		buffer.add("d")
		assertEquals(listOf("b", "c", "d"), buffer.snapshot().map { it.name })
	}

	@Test
	fun bugReportRequiresWhatAndExpected() = runTest {
		val service = BugReportService(
			preferencesRepository = fakePrefs(),
			transport = RecordingDiagnosticsTransport(),
			breadcrumbs = BreadcrumbBuffer(),
			appVersion = "1.0",
			platform = "Test",
			logReader = { "logs" },
		)
		assertTrue(service.submit(BugReportDraft("", "expected"), device()).isFailure)
		assertTrue(service.submit(BugReportDraft("what", ""), device()).isFailure)
	}

	@Test
	fun bugReportConvertsTransportExceptionsToFailure() = runTest {
		val throwingTransport = object : DiagnosticsTransport {
			override suspend fun sendBugReport(report: BugReport): Result<Unit> {
				throw IllegalStateException("offline")
			}
		}
		val service = BugReportService(
			preferencesRepository = fakePrefs(),
			transport = throwingTransport,
			breadcrumbs = BreadcrumbBuffer(),
			appVersion = "1.0",
			platform = "Test",
		)

		val result = service.submit(BugReportDraft("what", "expected"), device())

		assertTrue(result.isFailure)
		assertEquals("offline", result.exceptionOrNull()?.message)
	}

	@Test
	fun bugReportSubmitsWithRedactedLogs() = runTest {
		val transport = RecordingDiagnosticsTransport()
		val service = BugReportService(
			preferencesRepository = fakePrefs(),
			transport = transport,
			breadcrumbs = BreadcrumbBuffer(),
			appVersion = "1.0",
			platform = "Test",
			logReader = { LogRedactor.redact("ticket=abcdefghijklmnopqrstuvwxyz012345 plain") },
		)
		val result = service.submit(
			BugReportDraft(
				whatHappened = "crash on receive",
				expected = "receive succeeds",
				steps = "open ticket",
				contact = "user@example.com",
				includeLogs = true,
			),
			device(),
		)
		assertTrue(result.isSuccess)
		assertEquals(1, transport.bugReports.size)
		val report = transport.bugReports.single()
		assertEquals("crash on receive", report.whatHappened)
		assertTrue(report.logs.contains("[redacted-ticket]"))
		assertFalse(report.logs.contains("abcdefghijklmnopqrstuvwxyz012345"))
		assertEquals("test-install", report.installId)
	}

	@Test
	fun bugReportRedactsLogsBeforeApplyingUtf8Limit() {
		val secret = "abcdefghijklmnopqrstuvwxyz012345"
		val rawLogs = "ticket=$secret\n".repeat(6_000) + "tail-marker"
		val service = BugReportService(
			preferencesRepository = fakePrefs(),
			transport = RecordingDiagnosticsTransport(),
			breadcrumbs = BreadcrumbBuffer(),
			appVersion = "1.0",
			platform = "Test",
			logReader = { rawLogs },
		)

		val logs = service.assemble(BugReportDraft("what", "expected"), device(), "install").logs

		assertFalse(logs.contains(secret))
		assertTrue(logs.endsWith("tail-marker"))
		assertTrue(logs.encodeToByteArray().size <= BugReportService.MaxLogBytes)
	}

	@Test
	fun bugReportLogLimitCountsUtf8Bytes() {
		val service = BugReportService(
			preferencesRepository = fakePrefs(),
			transport = RecordingDiagnosticsTransport(),
			breadcrumbs = BreadcrumbBuffer(),
			appVersion = "1.0",
			platform = "Test",
			logReader = { "🙂".repeat(60_000) },
		)

		val logs = service.assemble(BugReportDraft("what", "expected"), device(), "install").logs

		assertEquals(BugReportService.MaxLogBytes, logs.encodeToByteArray().size)
		assertEquals(BugReportService.MaxLogBytes, service.previewLogBytes())
	}

	private fun bugReport(
		id: String = "b",
		logs: String = "",
		includeLogs: Boolean = false,
	) = BugReport(
		id = id,
		timestampMillis = 1L,
		installId = "i",
		appVersion = "1.0",
		platform = "Test",
		whatHappened = "w",
		expected = "e",
		steps = "",
		contact = "",
		includeLogs = includeLogs,
		logs = logs,
		device = DeviceSnapshot(null, null, "OS", null, null),
		breadcrumbs = emptyList(),
	)

	private fun fakePrefs() = FakePreferencesRepository(
		AppPreferences(
			username = "User",
			receiveFolder = ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp"),
			themeMode = ThemeMode.System,
			notificationsEnabled = false,
			diagnosticsInstallId = "test-install",
		),
	)

	private fun device() = DeviceInfo("Phone", "Pixel", "Android 15", "Wi-Fi", "90%")
}
