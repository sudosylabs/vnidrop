package com.vnidrop.app.diagnostics

internal fun sanitizeDiagnosticsInstallId(value: String): String {
	val trimmed = value.trim()
	if (trimmed.any { it.code < 0x20 || it.code == 0x7f }) return ""
	return trimmed.takeUtf8Bytes(DiagnosticsJson.MaxInstallIdBytes)
}

internal fun String.takeUtf8Bytes(maxBytes: Int): String {
	require(maxBytes >= 0) { "maxBytes must not be negative" }
	val encoded = encodeToByteArray()
	if (encoded.size <= maxBytes) return this
	for (endIndex in maxBytes downTo (maxBytes - 3).coerceAtLeast(0)) {
		val decoded = runCatching {
			encoded.decodeToString(0, endIndex, throwOnInvalidSequence = true)
		}.getOrNull()
		if (decoded != null) return decoded
	}
	return ""
}
