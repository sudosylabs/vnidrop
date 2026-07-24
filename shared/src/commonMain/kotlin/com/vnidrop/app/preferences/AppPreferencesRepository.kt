package com.vnidrop.app.preferences

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.preferencesOf
import androidx.datastore.preferences.core.stringPreferencesKey
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.core.RelayMode
import com.vnidrop.app.core.RelaySettings
import com.vnidrop.app.ui.theme.ThemeMode
import com.vnidrop.app.util.randomUuidString
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import okio.Path.Companion.toPath

data class AppPreferences(
	val username: String,
	val receiveFolder: ReceiveFolder,
	val themeMode: ThemeMode,
	val notificationsEnabled: Boolean,
	/** Master opt-in for automatic telemetry + crash upload. Bug reports remain available always. */
	val diagnosticsEnabled: Boolean = false,
	/** Stable anonymous install id; never an account or advertising id. */
	val diagnosticsInstallId: String = "",
	val relaySettings: RelaySettings = RelaySettings(),
)

class AppPreferencesDefaults(
	val username: String,
	val receiveFolder: ReceiveFolder,
	val themeMode: ThemeMode,
	val notificationsEnabled: Boolean = false,
	val diagnosticsEnabled: Boolean = false,
)

interface PreferencesRepository {
	val preferences: Flow<AppPreferences>
	suspend fun setUsername(username: String)
	suspend fun setReceiveFolder(folder: ReceiveFolder)
	suspend fun resetReceiveFolder()
	suspend fun setThemeMode(mode: ThemeMode)
	suspend fun setNotificationsEnabled(enabled: Boolean)
	suspend fun setDiagnosticsEnabled(enabled: Boolean)
	suspend fun setRelaySettings(settings: RelaySettings)
	/** Ensures a durable install id exists and returns it. */
	suspend fun ensureDiagnosticsInstallId(): String
}

class AppPreferencesRepository(
	private val dataStore: DataStore<Preferences>,
	private val defaults: AppPreferencesDefaults,
) : PreferencesRepository {
	override val preferences: Flow<AppPreferences> = dataStore.data
		.catch {
			emit(preferencesOf(PreferenceKeys.RelayMode to RelayMode.StrictCustom.name))
		}
		.map { prefs ->
			val storedRelayMode = prefs[PreferenceKeys.RelayMode]
			val parsedRelayMode = storedRelayMode?.let(::relayModeOrNull)
			val relayMode = when (storedRelayMode) {
				null -> RelayMode.Automatic
				else -> parsedRelayMode ?: RelayMode.StrictCustom
			}
			val relayUrls = if (storedRelayMode != null && parsedRelayMode == null) {
				emptyList()
			} else {
				prefs[PreferenceKeys.RelayUrls]
					.orEmpty()
					.lineSequence()
					.map(String::trim)
					.filter(String::isNotEmpty)
					.distinct()
					.toList()
			}
			AppPreferences(
				username = prefs[PreferenceKeys.Username]?.takeIf { it.isNotBlank() } ?: defaults.username,
				receiveFolder = resolveReceiveFolder(prefs, defaults.receiveFolder),
				themeMode = prefs[PreferenceKeys.ThemeMode]?.let { themeModeOrNull(it) } ?: defaults.themeMode,
				notificationsEnabled = prefs[PreferenceKeys.NotificationsEnabled] ?: defaults.notificationsEnabled,
				diagnosticsEnabled = prefs[PreferenceKeys.DiagnosticsEnabled] ?: defaults.diagnosticsEnabled,
				diagnosticsInstallId = prefs[PreferenceKeys.DiagnosticsInstallId].orEmpty(),
				relaySettings = RelaySettings(
					mode = relayMode,
					relayUrls = relayUrls,
				),
			)
		}

	override suspend fun setUsername(username: String) {
		dataStore.edit { prefs ->
			prefs[PreferenceKeys.Username] = username.trim()
		}
	}

	override suspend fun setReceiveFolder(folder: ReceiveFolder) {
		dataStore.edit { prefs ->
			prefs[PreferenceKeys.ReceiveFolderKind] = folder.kind.name
			prefs[PreferenceKeys.ReceiveFolderValue] = folder.value
			prefs[PreferenceKeys.ReceiveFolderDisplayName] = folder.displayName
		}
	}

	override suspend fun resetReceiveFolder() {
		setReceiveFolder(defaults.receiveFolder)
	}

	override suspend fun setThemeMode(mode: ThemeMode) {
		dataStore.edit { prefs ->
			prefs[PreferenceKeys.ThemeMode] = mode.name
		}
	}

	override suspend fun setNotificationsEnabled(enabled: Boolean) {
		dataStore.edit { prefs ->
			prefs[PreferenceKeys.NotificationsEnabled] = enabled
		}
	}

	override suspend fun setDiagnosticsEnabled(enabled: Boolean) {
		dataStore.edit { prefs ->
			prefs[PreferenceKeys.DiagnosticsEnabled] = enabled
		}
	}

	override suspend fun setRelaySettings(settings: RelaySettings) {
		dataStore.edit { prefs ->
			prefs[PreferenceKeys.RelayMode] = settings.mode.name
			prefs[PreferenceKeys.RelayUrls] = settings.relayUrls.joinToString("\n")
		}
	}

	override suspend fun ensureDiagnosticsInstallId(): String {
		val existing = preferences.first().diagnosticsInstallId
		if (existing.isNotBlank()) return existing
		val created = randomUuidString()
		dataStore.edit { prefs ->
			if (prefs[PreferenceKeys.DiagnosticsInstallId].isNullOrBlank()) {
				prefs[PreferenceKeys.DiagnosticsInstallId] = created
			}
		}
		return preferences.first().diagnosticsInstallId.ifBlank { created }
	}
}

fun createAppPreferencesDataStore(appDataDir: String): DataStore<Preferences> =
	PreferenceDataStoreFactory.createWithPath(
		produceFile = { "$appDataDir/$AppPreferencesFileName".toPath() },
	)

private object PreferenceKeys {
	val Username = stringPreferencesKey("username")
	val ReceiveFolderKind = stringPreferencesKey("receive_folder_kind")
	val ReceiveFolderValue = stringPreferencesKey("receive_folder_value")
	val ReceiveFolderDisplayName = stringPreferencesKey("receive_folder_display_name")
	val ThemeMode = stringPreferencesKey("theme_mode")
	val NotificationsEnabled = booleanPreferencesKey("notifications_enabled")
	val DiagnosticsEnabled = booleanPreferencesKey("diagnostics_enabled")
	val DiagnosticsInstallId = stringPreferencesKey("diagnostics_install_id")
	val RelayMode = stringPreferencesKey("relay_mode")
	val RelayUrls = stringPreferencesKey("relay_urls")
}

private fun resolveReceiveFolder(prefs: Preferences, defaults: ReceiveFolder): ReceiveFolder {
	val kind = prefs[PreferenceKeys.ReceiveFolderKind]?.let { receiveFolderKindOrNull(it) }
		?: defaults.kind
	val value = prefs[PreferenceKeys.ReceiveFolderValue]?.takeIf { it.isNotBlank() }
		?: defaults.value
	val displayName = prefs[PreferenceKeys.ReceiveFolderDisplayName]?.takeIf { it.isNotBlank() }
		?: defaults.displayName
	// Older Android builds defaulted to app-private "Downloads" paths that are
	// invisible in the system Downloads UI. Promote those back to the shared
	// public Downloads default so receive matches desktop expectations.
	if (kind == ReceiveFolderKind.FileSystemPath && isLegacyAndroidAppDownloadsPath(value)) {
		return defaults
	}
	return ReceiveFolder(kind = kind, value = value, displayName = displayName)
}

private fun isLegacyAndroidAppDownloadsPath(path: String): Boolean {
	// Typical: /storage/emulated/0/Android/data/<pkg>/files/Download[s]
	val normalized = path.replace('\\', '/')
	return normalized.contains("/Android/data/") &&
		(normalized.endsWith("/files/Download") ||
			normalized.endsWith("/files/Downloads") ||
			normalized.contains("/files/Download/") ||
			normalized.contains("/files/Downloads/"))
}

private fun receiveFolderKindOrNull(raw: String): ReceiveFolderKind? =
	runCatching { ReceiveFolderKind.valueOf(raw) }.getOrNull()

private fun themeModeOrNull(raw: String): ThemeMode? =
	runCatching { ThemeMode.valueOf(raw) }.getOrNull()

private fun relayModeOrNull(raw: String): RelayMode? =
	if (raw == "Custom") RelayMode.StrictCustom
	else runCatching { RelayMode.valueOf(raw) }.getOrNull()

private const val AppPreferencesFileName = "app_preferences.preferences_pb"
