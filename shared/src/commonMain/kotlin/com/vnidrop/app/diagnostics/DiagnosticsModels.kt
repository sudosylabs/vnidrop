package com.vnidrop.app.diagnostics

/**
 * Client-side diagnostics payloads. Transport to Cloudflare (or elsewhere) is
 * intentionally abstracted; nothing here assumes a network backend.
 */

data class DeviceSnapshot(
	val deviceName: String?,
	val deviceModel: String?,
	val operatingSystem: String,
	val network: String?,
	val batteryLevel: String?,
)

data class BugReport(
	val id: String,
	val timestampMillis: Long,
	val installId: String,
	val appVersion: String,
	val platform: String,
	val whatHappened: String,
	val expected: String,
	val steps: String,
	val contact: String,
	val includeLogs: Boolean,
	val logs: String,
	val device: DeviceSnapshot,
	val schemaVersion: Int = DiagnosticsSchemaVersion,
)

const val DiagnosticsSchemaVersion: Int = 1
