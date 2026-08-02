package com.vnidrop.app.diagnostics

import com.vnidrop.app.logging.AppLogger
import kotlinx.coroutines.CancellationException

/**
 * HTTPS client for the Cloudflare diagnostics Worker.
 * No-ops are preferred when [baseUrl] is blank — see [createDiagnosticsTransport].
 */
class HttpDiagnosticsTransport(
	baseUrl: String,
	private val ingestKey: String,
	private val appVersion: String = "",
	private val platform: String = "",
	private val installIdProvider: suspend () -> String = { "" },
	private val post: suspend (url: String, headers: Map<String, String>, body: String) -> PlatformHttpResponse =
		{ url, headers, body -> platformHttpPost(url, headers, body) },
) : DiagnosticsTransport {
	private val root = baseUrl.trim().trimEnd('/')

	init {
		require(root.isEmpty() || root.isAllowedDiagnosticsEndpoint()) {
			"diagnostics endpoint must use HTTPS unless it targets a loopback host"
		}
		require(root.isEmpty() || ingestKey.isNotBlank()) {
			"diagnostics ingest key must be configured when the endpoint is set"
		}
	}

	override suspend fun sendBugReport(report: BugReport): Result<Unit> {
		val body = DiagnosticsJson.bugBody(report)
		return postJson("/v1/bugs", body, report.installId, report.id)
	}

	private suspend fun postJson(
		path: String,
		body: String,
		installId: String,
		expectedId: String,
	): Result<Unit> {
		if (root.isEmpty()) {
			return Result.failure(IllegalStateException("diagnostics endpoint is not configured"))
		}
		if (body.encodeToByteArray().size > DiagnosticsJson.MaxRequestBytes) {
			return Result.failure(DiagnosticsPayloadException("diagnostics $path payload is too large"))
		}
		return try {
			val response = post(
				"$root$path",
				mapOf(
					"X-VniDrop-Key" to ingestKey,
					"X-VniDrop-Install-Id" to installId,
					"Accept" to "application/json",
				),
				body,
			)
			if (response.statusCode !in 200..299) {
				AppLogger.warn(
					"diagnostics",
					"transport rejected $path",
					mapOf("status" to response.statusCode.toString()),
				)
				throw DiagnosticsHttpException(response.statusCode, path)
			}
			if (!response.body.isSuccessfulDiagnosticsAcknowledgement(expectedId)) {
				throw DiagnosticsProtocolException(path)
			}
			Result.success(Unit)
		} catch (cancelled: CancellationException) {
			throw cancelled
		} catch (error: Throwable) {
			Result.failure(error)
		}
	}
}

internal class DiagnosticsHttpException(
	val statusCode: Int,
	path: String,
) : IllegalStateException("diagnostics $path failed: HTTP $statusCode")

internal class DiagnosticsProtocolException(path: String) :
	IllegalStateException("diagnostics $path returned an invalid acknowledgement")

internal class DiagnosticsPayloadException(message: String) : IllegalArgumentException(message)

/**
 * Builds transport from compile-time config. Empty endpoint and key → [NoOpDiagnosticsTransport].
 */
fun createDiagnosticsTransport(
	appVersion: String,
	platform: String,
	installIdProvider: suspend () -> String,
): DiagnosticsTransport = buildDiagnosticsTransport(
	included = DiagnosticsBuildConfig.INCLUDED,
	endpoint = DiagnosticsBuildConfig.ENDPOINT,
	ingestKey = DiagnosticsBuildConfig.INGEST_KEY,
	appVersion = appVersion,
	platform = platform,
	installIdProvider = installIdProvider,
)

internal fun buildDiagnosticsTransport(
	included: Boolean = true,
	endpoint: String,
	ingestKey: String,
	appVersion: String,
	platform: String,
	installIdProvider: suspend () -> String,
): DiagnosticsTransport {
	if (!included) return NoOpDiagnosticsTransport()
	val normalizedEndpoint = endpoint.trim()
	val normalizedIngestKey = ingestKey.trim()
	if (normalizedEndpoint.isEmpty() && normalizedIngestKey.isEmpty()) return NoOpDiagnosticsTransport()
	check(normalizedEndpoint.isNotEmpty() && normalizedIngestKey.isNotEmpty()) {
		"diagnostics endpoint and ingest key must be configured together"
	}
	return HttpDiagnosticsTransport(
		baseUrl = normalizedEndpoint,
		ingestKey = normalizedIngestKey,
		appVersion = appVersion,
		platform = platform,
		installIdProvider = installIdProvider,
	)
}

private fun String.isAllowedDiagnosticsEndpoint(): Boolean {
	if (any(Char::isWhitespace) || '?' in this || '#' in this) return false
	val schemeSeparator = indexOf("://")
	if (schemeSeparator <= 0) return false
	val scheme = substring(0, schemeSeparator).lowercase()
	val authority = substring(schemeSeparator + 3).substringBefore('/')
	if (authority.isEmpty() || '@' in authority) return false
	val host = when {
		authority.startsWith('[') -> {
			val end = authority.indexOf(']')
			if (end <= 1) return false
			val suffix = authority.substring(end + 1)
			if (suffix.isNotEmpty() && !suffix.isValidPortSuffix()) return false
			authority.substring(1, end)
		}
		else -> {
			if (authority.count { it == ':' } > 1) return false
			val portSeparator = authority.indexOf(':')
			if (portSeparator >= 0 && !authority.substring(portSeparator).isValidPortSuffix()) return false
			authority.substringBefore(':')
		}
	}.lowercase()
	if (host.isEmpty()) return false
	if (scheme == "https") return true
	return scheme == "http" && host.isLoopbackHost()
}

private fun String.isValidPortSuffix(): Boolean =
	startsWith(':') && drop(1).toIntOrNull() in 1..65_535

private fun String.isLoopbackHost(): Boolean {
	if (this == "localhost" || this == "::1") return true
	val octets = split('.')
	return octets.size == 4 &&
		octets.first() == "127" &&
		octets.all { it.toIntOrNull() in 0..255 }
}
