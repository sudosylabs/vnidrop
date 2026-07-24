package com.vnidrop.app.feature.settings

import com.vnidrop.app.core.RelayMode
import com.vnidrop.app.core.RelaySettings
import com.vnidrop.app.core.usesCustomRelayUrls

sealed interface RelaySettingsInputError {
	data object MissingUrl : RelaySettingsInputError
	data class TooManyUrls(val maximum: Int) : RelaySettingsInputError
	data class HttpsRequired(val line: Int) : RelaySettingsInputError
	data class InvalidUrl(val line: Int) : RelaySettingsInputError
	data class DuplicateUrl(val line: Int) : RelaySettingsInputError
}

data class RelaySettingsValidation(
	val settings: RelaySettings? = null,
	val error: RelaySettingsInputError? = null,
)

fun validateRelaySettings(
	mode: RelayMode,
	relayUrls: List<String>,
	retainedUrls: List<String> = emptyList(),
): RelaySettingsValidation {
	if (!mode.usesCustomRelayUrls) {
		return RelaySettingsValidation(RelaySettings(mode, retainedUrls))
	}
	val lines = relayUrls
		.mapIndexedNotNull { index, raw -> raw.trim().takeIf(String::isNotEmpty)?.let { index + 1 to it } }
		.toList()
	if (lines.isEmpty()) return RelaySettingsValidation(error = RelaySettingsInputError.MissingUrl)
	if (lines.size > MaximumRelayUrls) {
		return RelaySettingsValidation(error = RelaySettingsInputError.TooManyUrls(MaximumRelayUrls))
	}

	val normalized = mutableListOf<String>()
	for ((line, raw) in lines) {
		when (val result = normalizeRelayUrl(raw)) {
			RelayUrlResult.HttpsRequired -> {
				return RelaySettingsValidation(error = RelaySettingsInputError.HttpsRequired(line))
			}
			RelayUrlResult.Invalid -> {
				return RelaySettingsValidation(error = RelaySettingsInputError.InvalidUrl(line))
			}
			is RelayUrlResult.Valid -> {
				if (result.url in normalized) {
					return RelaySettingsValidation(error = RelaySettingsInputError.DuplicateUrl(line))
				}
				normalized += result.url
			}
		}
	}
	return RelaySettingsValidation(RelaySettings(mode, normalized))
}

private sealed interface RelayUrlResult {
	data object HttpsRequired : RelayUrlResult
	data object Invalid : RelayUrlResult
	data class Valid(val url: String) : RelayUrlResult
}

private fun normalizeRelayUrl(raw: String): RelayUrlResult {
	if (!raw.startsWith(HttpsPrefix, ignoreCase = true)) return RelayUrlResult.HttpsRequired
	if (raw.encodeToByteArray().size > MaximumRelayUrlLength || raw.any { it.isWhitespace() || it.isISOControl() }) {
		return RelayUrlResult.Invalid
	}
	val remainder = raw.substring(HttpsPrefix.length)
	if (remainder.isEmpty() || '?' in remainder || '#' in remainder) return RelayUrlResult.Invalid
	val authority = remainder.substringBefore('/')
	val path = remainder.removePrefix(authority)
	if (!isValidAuthority(authority) || path !in setOf("", "/")) return RelayUrlResult.Invalid

	val normalizedAuthority = if (authority.startsWith('[')) {
		val closingBracket = authority.indexOf(']')
		authority.substring(0, closingBracket + 1).lowercase() + authority.substring(closingBracket + 1)
	} else {
		val portSeparator = authority.lastIndexOf(':').takeIf { authority.count { char -> char == ':' } == 1 }
		if (portSeparator == null) authority.lowercase()
		else authority.substring(0, portSeparator).lowercase() + authority.substring(portSeparator)
	}
	return RelayUrlResult.Valid("$HttpsPrefix${normalizedAuthority.removeSuffix(":443")}")
}

private fun isValidAuthority(authority: String): Boolean {
	if (authority.isBlank() || '@' in authority) return false
	if (authority.startsWith('[')) {
		val closingBracket = authority.indexOf(']')
		if (closingBracket <= 1) return false
		val address = authority.substring(1, closingBracket)
		if (!isValidIpv6Address(address)) return false
		return isValidPortSuffix(authority.substring(closingBracket + 1))
	}
	if (authority.count { it == ':' } > 1) return false
	val host = authority.substringBeforeLast(':', authority)
	val portSuffix = authority.removePrefix(host)
	if (host.isBlank() || host.startsWith('.') || host.endsWith('.') || host.startsWith('-') || host.endsWith('-')) return false
	if (host.any { !it.isLetterOrDigit() && it != '.' && it != '-' }) return false
	return isValidPortSuffix(portSuffix)
}

private fun isValidIpv6Address(address: String): Boolean {
	if (address.isEmpty() || ":::" in address) return false
	val compressionIndex = address.indexOf("::")
	val hasCompression = compressionIndex >= 0
	if (hasCompression && address.indexOf("::", compressionIndex + 2) >= 0) return false
	if (!hasCompression && (address.startsWith(':') || address.endsWith(':'))) return false

	val left = if (hasCompression) address.substring(0, compressionIndex) else address
	val right = if (hasCompression) address.substring(compressionIndex + 2) else ""
	val segments = buildList {
		if (left.isNotEmpty()) addAll(left.split(':'))
		if (right.isNotEmpty()) addAll(right.split(':'))
	}
	if (segments.any(String::isEmpty)) return false

	var addressUnits = 0
	for ((index, segment) in segments.withIndex()) {
		if ('.' in segment) {
			if (index != segments.lastIndex || !isValidIpv4Tail(segment)) return false
			addressUnits += 2
		} else {
			if (segment.length !in 1..4 || segment.any { !it.isHexDigit() }) return false
			addressUnits += 1
		}
	}
	return if (hasCompression) addressUnits < 8 else addressUnits == 8
}

private fun isValidIpv4Tail(address: String): Boolean {
	val octets = address.split('.')
	return octets.size == 4 && octets.all { octet ->
		octet.isNotEmpty() && octet.all(Char::isDigit) && octet.toIntOrNull() in 0..255
	}
}

private fun Char.isHexDigit(): Boolean =
	this in '0'..'9' || lowercaseChar() in 'a'..'f'

private fun isValidPortSuffix(suffix: String): Boolean {
	if (suffix.isEmpty()) return true
	if (!suffix.startsWith(':')) return false
	val port = suffix.drop(1).toIntOrNull() ?: return false
	return port in 1..65535
}

private const val HttpsPrefix = "https://"
internal const val MaximumRelayUrls = 8
private const val MaximumRelayUrlLength = 2_048
