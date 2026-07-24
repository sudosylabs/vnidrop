import XCTest
@testable import VniDrop

/// Ports app-level assertions: core initialization on launch, destination
/// selection guard, and theme following preferences.
@MainActor
final class AppModelTests: XCTestCase {

	private func makeModel(_ core: FakeCoreGateway, preferences: AppPreferencesRepository) -> AppModel {
		AppModel(
			environment: PlatformEnvironment(name: "Test", appVersion: "0.1.0", defaultCoreDataDir: NSTemporaryDirectory()),
			repository: core,
			preferences: preferences,
			messages: UiMessageController()
		)
	}

	func testInitializesCoreOnLaunch() async {
		let core = FakeCoreGateway()
		_ = makeModel(core, preferences: Fixtures.preferences())
		await waitUntil { core.state.isInitialized }
		XCTAssertTrue(core.state.isInitialized)
		XCTAssertEqual(core.initializedNetworkConfigurations, [.automatic])
	}

	func testInitializesCoreWithSavedCustomRelayConfiguration() async {
		let core = FakeCoreGateway()
		let preferences = Fixtures.preferences()
		let configuration = RelayConfiguration(mode: .strictCustom, relayURLs: ["https://relay.example"])
		preferences.setRelayConfiguration(configuration)
		_ = makeModel(core, preferences: preferences)

		await waitUntil { core.state.isInitialized }
		XCTAssertEqual(core.initializedNetworkConfigurations, [configuration])
	}

	func testSelectDestination() {
		let model = makeModel(FakeCoreGateway(), preferences: Fixtures.preferences())
		XCTAssertEqual(model.destination, .send)
		model.selectDestination(.settings)
		XCTAssertEqual(model.destination, .settings)
		model.selectDestination(.settings) // no-op guard
		XCTAssertEqual(model.destination, .settings)
	}

	func testThemeModeFollowsPreferences() async {
		let prefs = Fixtures.preferences()
		let model = makeModel(FakeCoreGateway(), preferences: prefs)
		prefs.setThemeMode(.dark)
		await waitUntil { model.themeMode == .dark }
		XCTAssertEqual(model.themeMode, .dark)
	}
}
