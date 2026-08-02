package com.vnidrop.app.diagnostics

/**
 * Minimal JSON encoding for diagnostics payloads (no kotlinx.serialization dependency).
 */
internal object DiagnosticsJson {
	internal const val MaxRequestBytes = 256 * 1024
	internal const val MaxInstallIdBytes = 80
	internal const val MaxAppVersionBytes = 40
	internal const val MaxPlatformBytes = 40
	private const val MaxBreadcrumbsJsonBytes = 16_000
	private const val MaxBreadcrumbs = 40

	fun bugBody(report: BugReport): String {
		val logs = if (report.includeLogs) report.logs else ""
		val complete = buildBugBody(report, logs)
		if (complete.encodeToByteArray().size <= MaxRequestBytes || logs.isEmpty()) return complete

		var best = buildBugBody(report, "")
		if (best.encodeToByteArray().size > MaxRequestBytes) return best
		var minimumBytes = 0
		var maximumBytes = logs.encodeToByteArray().size
		while (minimumBytes <= maximumBytes) {
			val candidateBytes = minimumBytes + (maximumBytes - minimumBytes) / 2
			val candidate = buildBugBody(report, logs.takeUtf8Bytes(candidateBytes))
			if (candidate.encodeToByteArray().size <= MaxRequestBytes) {
				best = candidate
				minimumBytes = candidateBytes + 1
			} else {
				maximumBytes = candidateBytes - 1
			}
		}
		return best
	}

	private fun buildBugBody(report: BugReport, logs: String): String = buildString {
		append('{')
		appendJsonField("id", report.id)
		append(',')
		append("\"timestampMillis\":")
		append(report.timestampMillis)
		append(',')
		appendJsonField("installId", report.installId)
		append(',')
		appendJsonField("appVersion", report.appVersion)
		append(',')
		appendJsonField("platform", report.platform)
		append(',')
		appendJsonField("whatHappened", report.whatHappened)
		append(',')
		appendJsonField("expected", report.expected)
		append(',')
		appendJsonField("steps", report.steps)
		append(',')
		appendJsonField("contact", report.contact)
		append(',')
		append("\"includeLogs\":")
		append(report.includeLogs)
		append(',')
		appendJsonField("logs", logs)
		append(',')
		append("\"schemaVersion\":")
		append(report.schemaVersion)
		append(',')
		append("\"device\":{")
		appendJsonField("deviceName", report.device.deviceName.orEmpty())
		append(',')
		appendJsonField("deviceModel", report.device.deviceModel.orEmpty())
		append(',')
		appendJsonField("operatingSystem", report.device.operatingSystem)
		append(',')
		appendJsonField("network", report.device.network.orEmpty())
		append(',')
		appendJsonField("batteryLevel", report.device.batteryLevel.orEmpty())
		append("},")
		append("\"breadcrumbs\":")
		appendBreadcrumbs(report.breadcrumbs)
		append('}')
	}

	private fun StringBuilder.appendBreadcrumbs(crumbs: List<Breadcrumb>) {
		append('[')
		var encodedBytes = 2
		var appended = 0
		for (crumb in crumbs) {
			if (appended == MaxBreadcrumbs) break
			val name = sanitizeDiagnosticName(crumb.name)
			if (name.isBlank() || crumb.timestampMillis < 0) continue
			val encoded = buildString {
				append('{')
				appendJsonField("name", name)
				append(',')
				append("\"timestampMillis\":")
				append(crumb.timestampMillis)
				append(',')
				append("\"properties\":")
				appendStringMap(crumb.properties)
				append('}')
			}
			val additionBytes = encoded.encodeToByteArray().size + if (appended == 0) 0 else 1
			if (encodedBytes + additionBytes > MaxBreadcrumbsJsonBytes) break
			if (appended > 0) append(',')
			append(encoded)
			encodedBytes += additionBytes
			appended += 1
		}
		append(']')
	}

	private fun StringBuilder.appendStringMap(map: Map<String, String>) {
		append('{')
		sanitizeDiagnosticProperties(map).entries.forEachIndexed { index, (key, value) ->
			if (index > 0) append(',')
			appendJsonField(key, value)
		}
		append('}')
	}

	private fun StringBuilder.appendJsonField(key: String, value: String) {
		append('"')
		append(escape(key))
		append("\":\"")
		append(escape(value))
		append('"')
	}

	internal fun escape(raw: String): String = buildString(raw.length + 8) {
		for (ch in raw) {
			when (ch) {
				'\\' -> append("\\\\")
				'"' -> append("\\\"")
				'\n' -> append("\\n")
				'\r' -> append("\\r")
				'\t' -> append("\\t")
				else -> if (ch.code < 0x20) {
					append("\\u")
					append(ch.code.toString(16).padStart(4, '0'))
				} else {
					append(ch)
				}
			}
		}
	}
}
