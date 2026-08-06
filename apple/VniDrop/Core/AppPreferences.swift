import Foundation
import Combine

/// Receive-destination descriptor, ported from `core/FileSystemService.kt`.
enum ReceiveFolderKind: String, Codable, Sendable {
	case fileSystemPath
	case iosSecurityScopedUrl
}

struct ReceiveFolder: Equatable, Codable, Sendable {
	let kind: ReceiveFolderKind
	let value: String
	let displayName: String
}

enum FolderAccessStatus {
	case writable
	case permissionRequired
	case unavailable
}

enum RelayPreferenceMode: String, Codable, CaseIterable, Sendable {
	case automatic
	case strictCustom = "custom"
	case customWithDirectFallback = "custom-with-direct-fallback"
	case localOnly = "local-only"

	var usesCustomRelayURLs: Bool {
		self == .strictCustom || self == .customWithDirectFallback
	}
}

struct RelayConfiguration: Equatable, Codable, Sendable {
	var mode: RelayPreferenceMode
	var relayURLs: [String]

	static let automatic = RelayConfiguration(mode: .automatic, relayURLs: [])
}

enum RelayConfigurationValidationError: Error, Equatable, Sendable {
	case missingURL
	case tooManyURLs
	case httpsRequired(index: Int)
	case invalidURL(index: Int)
	case duplicateURL(index: Int)

	var urlIndex: Int? {
		switch self {
		case .httpsRequired(let index), .invalidURL(let index), .duplicateURL(let index): return index
		case .missingURL, .tooManyURLs: return nil
		}
	}
}

enum RelayConfigurationValidator {
	static let maximumRelayCount = 8
	static let maximumRelayURLBytes = 2_048

	static func validate(
		mode: RelayPreferenceMode,
		relayURLs: [String],
		retainedRelayURLs: [String] = []
	) throws -> RelayConfiguration {
		guard mode.usesCustomRelayURLs else {
			return RelayConfiguration(mode: mode, relayURLs: retainedRelayURLs)
		}

		let relayEntries = relayURLs.enumerated().compactMap { index, value -> (Int, String)? in
			let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
			return trimmed.isEmpty ? nil : (index, trimmed)
		}
		guard !relayEntries.isEmpty else {
			throw RelayConfigurationValidationError.missingURL
		}
		guard relayEntries.count <= maximumRelayCount else {
			throw RelayConfigurationValidationError.tooManyURLs
		}

		var seen = Set<String>()
		var normalizedURLs: [String] = []
		for (index, relayURL) in relayEntries {
			guard relayURL.lengthOfBytes(using: .utf8) <= maximumRelayURLBytes,
				relayURL.rangeOfCharacter(from: .whitespacesAndNewlines.union(.controlCharacters)) == nil,
				var components = URLComponents(string: relayURL) else {
				throw RelayConfigurationValidationError.invalidURL(index: index)
			}
			guard components.scheme?.lowercased() == "https" else {
				throw RelayConfigurationValidationError.httpsRequired(index: index)
			}
			guard
				let host = components.host,
				!host.isEmpty,
				components.port.map({ (1...65_535).contains($0) }) ?? true,
				components.user == nil,
				components.password == nil,
				components.query == nil,
				components.fragment == nil,
				components.path.isEmpty || components.path == "/"
			else {
				throw RelayConfigurationValidationError.invalidURL(index: index)
			}

			components.scheme = "https"
			components.host = host.lowercased()
			if components.port == 443 { components.port = nil }
			if components.path == "/" { components.path = "" }
			guard let canonicalURL = components.string, seen.insert(canonicalURL).inserted else {
				throw RelayConfigurationValidationError.duplicateURL(index: index)
			}
			normalizedURLs.append(canonicalURL)
		}

		return RelayConfiguration(mode: mode, relayURLs: normalizedURLs)
	}
}

/// Persisted app preferences, ported from `preferences/AppPreferencesRepository.kt`.
/// Backed by `UserDefaults` instead of DataStore; keys and semantics match.
struct AppPreferences: Equatable {
	var username: String
	var receiveFolder: ReceiveFolder
	var themeMode: ThemeMode
	var diagnosticsInstallId: String
	var relayConfiguration: RelayConfiguration
	/// Idle lifetime applied to grants this device issues from now on.
	var grantLifetime: GrantLifetimeOption
	/// Devices the user declined to remember. Persisted so a repeat transfer
	/// with the same device does not re-ask forever.
	var declinedPairingSuggestions: Set<String>
}

struct AppPreferencesDefaults {
	let username: String
	let receiveFolder: ReceiveFolder
	let themeMode: ThemeMode
}

@MainActor
final class AppPreferencesRepository: ObservableObject {
	@Published private(set) var preferences: AppPreferences

	private let defaults: UserDefaults
	private let fallback: AppPreferencesDefaults

	private enum Key {
		static let username = "username"
		static let receiveFolderKind = "receive_folder_kind"
		static let receiveFolderValue = "receive_folder_value"
		static let receiveFolderDisplayName = "receive_folder_display_name"
		static let themeMode = "theme_mode"
		static let diagnosticsInstallId = "diagnostics_install_id"
		static let relayConfiguration = "relay_configuration"
		static let grantLifetime = "grant_lifetime"
		static let declinedPairingSuggestions = "declined_pairing_suggestions"
	}

	init(defaults: UserDefaults = .standard, fallback: AppPreferencesDefaults) {
		self.defaults = defaults
		self.fallback = fallback
		self.preferences = Self.read(from: defaults, fallback: fallback)
	}

	private static func read(from defaults: UserDefaults, fallback: AppPreferencesDefaults) -> AppPreferences {
		let username = (defaults.string(forKey: Key.username)).flatMap { $0.isEmpty ? nil : $0 } ?? fallback.username
		let folder = resolveReceiveFolder(defaults, fallback: fallback.receiveFolder)
		let themeMode = defaults.string(forKey: Key.themeMode).flatMap(ThemeMode.init(rawValue:)) ?? fallback.themeMode
		let installId = defaults.string(forKey: Key.diagnosticsInstallId) ?? ""
		let grantLifetime = defaults.string(forKey: Key.grantLifetime)
			.flatMap(GrantLifetimeOption.init(rawValue:)) ?? .days90
		let declined = Set(defaults.stringArray(forKey: Key.declinedPairingSuggestions) ?? [])
		return AppPreferences(
			username: username,
			receiveFolder: folder,
			themeMode: themeMode,
			diagnosticsInstallId: installId,
			relayConfiguration: resolveRelayConfiguration(defaults),
			grantLifetime: grantLifetime,
			declinedPairingSuggestions: declined
		)
	}

	private static func resolveRelayConfiguration(_ defaults: UserDefaults) -> RelayConfiguration {
		guard defaults.object(forKey: Key.relayConfiguration) != nil else { return .automatic }
		guard
			let data = defaults.data(forKey: Key.relayConfiguration),
			let configuration = try? JSONDecoder().decode(RelayConfiguration.self, from: data)
		else {
			// A stored profile must never silently fall back to public relays. Strict
			// custom with no URLs makes startup fail closed until Settings repairs it.
			return RelayConfiguration(mode: .strictCustom, relayURLs: [])
		}
		return configuration
	}

	private static func resolveReceiveFolder(_ defaults: UserDefaults, fallback: ReceiveFolder) -> ReceiveFolder {
		let kind = defaults.string(forKey: Key.receiveFolderKind)
			.flatMap(ReceiveFolderKind.init(rawValue:)) ?? fallback.kind
		let value = defaults.string(forKey: Key.receiveFolderValue).flatMap { $0.isEmpty ? nil : $0 } ?? fallback.value
		let displayName = defaults.string(forKey: Key.receiveFolderDisplayName)
			.flatMap { $0.isEmpty ? nil : $0 } ?? fallback.displayName
		return ReceiveFolder(kind: kind, value: value, displayName: displayName)
	}

	private func reload() {
		preferences = Self.read(from: defaults, fallback: fallback)
	}

	func setUsername(_ username: String) {
		defaults.set(username.trimmingCharacters(in: .whitespacesAndNewlines), forKey: Key.username)
		reload()
	}

	func setReceiveFolder(_ folder: ReceiveFolder) {
		defaults.set(folder.kind.rawValue, forKey: Key.receiveFolderKind)
		defaults.set(folder.value, forKey: Key.receiveFolderValue)
		defaults.set(folder.displayName, forKey: Key.receiveFolderDisplayName)
		reload()
	}

	func resetReceiveFolder() {
		setReceiveFolder(fallback.receiveFolder)
	}

	func declinePairingSuggestion(_ endpointId: String) {
		var declined = preferences.declinedPairingSuggestions
		declined.insert(endpointId)
		defaults.set(Array(declined), forKey: Key.declinedPairingSuggestions)
		reload()
	}

	/// Clears the decline so the device can be suggested again, used when the
	/// user pairs with it deliberately.
	func clearDeclinedPairingSuggestion(_ endpointId: String) {
		var declined = preferences.declinedPairingSuggestions
		guard declined.remove(endpointId) != nil else { return }
		defaults.set(Array(declined), forKey: Key.declinedPairingSuggestions)
		reload()
	}

	func setGrantLifetime(_ lifetime: GrantLifetimeOption) {
		defaults.set(lifetime.rawValue, forKey: Key.grantLifetime)
		reload()
	}

	func setThemeMode(_ mode: ThemeMode) {
		defaults.set(mode.rawValue, forKey: Key.themeMode)
		reload()
	}

	func setRelayConfiguration(_ configuration: RelayConfiguration) {
		guard let encoded = try? JSONEncoder().encode(configuration) else { return }
		defaults.set(encoded, forKey: Key.relayConfiguration)
		reload()
	}

	@discardableResult
	func ensureDiagnosticsInstallId() -> String {
		let existing = preferences.diagnosticsInstallId
		if !existing.isEmpty { return existing }
		let created = UUID().uuidString
		defaults.set(created, forKey: Key.diagnosticsInstallId)
		reload()
		return created
	}
}
