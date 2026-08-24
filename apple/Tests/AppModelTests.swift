import XCTest
import VnidropCore
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

	func testMissingEndpointIdentityOffersExplicitResetAndRecoversStartup() async {
		let core = FakeCoreGateway()
		core.initializeResult = .failure(
			VnidropError.SecureStorageMissing(reason: "credential is missing")
		)
		let model = makeModel(core, preferences: Fixtures.preferences())

		await waitUntil { model.startupRecovery == .identityUnrecoverable }
		XCTAssertFalse(core.state.isInitialized)

		await model.resetUnrecoverableIdentity()

		XCTAssertEqual(core.resetUnrecoverableIdentityCount, 1)
		XCTAssertNil(model.startupRecovery)
		XCTAssertTrue(core.state.isInitialized)
	}

	/// An unreachable store is not a repairable identity: the reset writes to the
	/// same store, so offering it (or a retry) would loop with no way out.
	func testUnreachableSecureStorageIsReportedAsItsOwnRecoveryState() async {
		let core = FakeCoreGateway()
		core.initializeResult = .failure(
			VnidropError.SecureStorageUnavailable(reason: "credential store is unavailable")
		)
		let model = makeModel(core, preferences: Fixtures.preferences())

		await waitUntil { model.startupRecovery == .secureStorageUnavailable }
		XCTAssertFalse(core.state.isInitialized)
		// The overlay shows the recovery copy, so a competing generic error would
		// be dead weight the user never sees.
		XCTAssertNil(model.startupError)
	}

	func testUnreachableSecureStorageDoesNotOfferTheIdentityReset() async {
		let core = FakeCoreGateway()
		core.initializeResult = .failure(
			VnidropError.SecureStorageUnavailable(reason: "credential store is unavailable")
		)
		let model = makeModel(core, preferences: Fixtures.preferences())
		await waitUntil { model.startupRecovery == .secureStorageUnavailable }

		await model.resetUnrecoverableIdentity()

		XCTAssertEqual(core.resetUnrecoverableIdentityCount, 0)
		XCTAssertEqual(model.startupRecovery, .secureStorageUnavailable)
	}

	/// A locked keychain does unlock, so it keeps the retryable path.
	func testLockedSecureStorageStaysRetryable() async {
		let core = FakeCoreGateway()
		core.initializeResult = .failure(
			VnidropError.SecureStorageLocked(reason: "credential store is locked")
		)
		let model = makeModel(core, preferences: Fixtures.preferences())

		await waitUntil { model.startupError != nil }
		XCTAssertNil(model.startupRecovery)
	}
}
