import XCTest
@testable import VniDrop

/// Ports `preferences/AppPreferencesRepositoryTest.kt` — values persist to the
/// backing store and reload identically.
@MainActor
final class AppPreferencesRepositoryTests: XCTestCase {

	private func defaults() -> UserDefaults { UserDefaults(suiteName: "vnidrop.prefs.\(UUID().uuidString)")! }
	private func fallback() -> AppPreferencesDefaults {
		AppPreferencesDefaults(
			username: "Default",
			receiveFolder: ReceiveFolder(kind: .fileSystemPath, value: "/tmp", displayName: "Downloads"),
			themeMode: .system
		)
	}

	func testMissingRelayProfileDefaultsToAutomatic() {
		let repo = AppPreferencesRepository(defaults: defaults(), fallback: fallback())
		XCTAssertEqual(repo.preferences.username, "Default")
		XCTAssertEqual(repo.preferences.themeMode, .system)
		XCTAssertEqual(repo.preferences.relayConfiguration, .automatic)
	}

	func testValuesPersistAndReload() {
		let store = defaults()
		let fb = fallback()
		let repo = AppPreferencesRepository(defaults: store, fallback: fb)
		repo.setUsername("Bob")
		repo.setThemeMode(.dark)
		repo.setReceiveFolder(ReceiveFolder(kind: .iosSecurityScopedUrl, value: "file:///x", displayName: "Custom"))
		repo.setRelayConfiguration(RelayConfiguration(
			mode: .strictCustom,
			relayURLs: ["https://relay-one.example", "https://relay-two.example:443"]
		))

		// A fresh repository over the same store reflects the persisted values.
		let reloaded = AppPreferencesRepository(defaults: store, fallback: fb)
		XCTAssertEqual(reloaded.preferences.username, "Bob")
		XCTAssertEqual(reloaded.preferences.themeMode, .dark)
		XCTAssertEqual(reloaded.preferences.receiveFolder.displayName, "Custom")
		XCTAssertEqual(reloaded.preferences.receiveFolder.kind, .iosSecurityScopedUrl)
		XCTAssertEqual(reloaded.preferences.relayConfiguration, RelayConfiguration(
			mode: .strictCustom,
			relayURLs: ["https://relay-one.example", "https://relay-two.example:443"]
		))
		XCTAssertNotNil(store.data(forKey: "relay_configuration"))
		XCTAssertNil(store.object(forKey: "relay_mode"))
		XCTAssertNil(store.object(forKey: "relay_urls"))
	}

	func testCorruptedRelayProfileFailsClosed() {
		let store = defaults()
		store.set(Data("{".utf8), forKey: "relay_configuration")

		let repo = AppPreferencesRepository(defaults: store, fallback: fallback())

		XCTAssertEqual(
			repo.preferences.relayConfiguration,
			RelayConfiguration(mode: .strictCustom, relayURLs: [])
		)
	}

	func testUnknownRelayModeFailsClosed() {
		let store = defaults()
		store.set(
			Data(#"{"mode":"future-mode","relayURLs":["https://relay.example"]}"#.utf8),
			forKey: "relay_configuration"
		)

		let repo = AppPreferencesRepository(defaults: store, fallback: fallback())

		XCTAssertEqual(
			repo.preferences.relayConfiguration,
			RelayConfiguration(mode: .strictCustom, relayURLs: [])
		)
	}

	func testResetReceiveFolderRestoresFallback() {
		let store = defaults()
		let fb = fallback()
		let repo = AppPreferencesRepository(defaults: store, fallback: fb)
		repo.setReceiveFolder(ReceiveFolder(kind: .fileSystemPath, value: "/custom", displayName: "Custom"))
		repo.resetReceiveFolder()
		XCTAssertEqual(repo.preferences.receiveFolder.value, "/tmp")
	}
}
