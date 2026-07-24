import XCTest
@testable import VniDrop

/// Ports settings assertions from `feature/ViewModelsTest.kt` — username debounce
/// persistence and the Storage "delete all transfers" flow.
@MainActor
final class SettingsModelTests: XCTestCase {

	private func makeModel(_ core: FakeCoreGateway, preferences: AppPreferencesRepository) -> SettingsModel {
		SettingsModel(
			environment: PlatformEnvironment(name: "Test", appVersion: "0.1.0", defaultCoreDataDir: NSTemporaryDirectory()),
			deviceInfoProvider: FakeDeviceInfoProvider(),
			fileSystemService: FakeFileSystemService(),
			repository: core,
			preferences: preferences,
			notifications: LocalNotificationService(),
			messages: UiMessageController(),
			bugReports: NoopBugReportService(),
			diagnosticsIncluded: false
		)
	}

	func testUsernameChangeDebouncesAndPersists() async {
		let prefs = Fixtures.preferences(username: "Original")
		let model = makeModel(FakeCoreGateway(), preferences: prefs)

		model.setUsername("Alice")
		XCTAssertEqual(model.state.username, "Alice") // immediate local echo
		await waitUntil { prefs.preferences.username == "Alice" } // persisted after debounce
		XCTAssertEqual(prefs.preferences.username, "Alice")
	}

	func testDeleteAllTransfersDeletesEveryTransfer() async {
		let core = FakeCoreGateway()
		let model = makeModel(core, preferences: Fixtures.preferences())
		core.setState(CoreState(isInitialized: true, transfers: [
			Fixtures.transfer(id: 2, direction: .send, status: .sharing),
			Fixtures.transfer(id: 3, direction: .receive, status: .done),
		]))

		model.deleteAllTransfers()
		await waitUntil { core.deletedTransfers.count == 2 }
		XCTAssertEqual(Set(core.deletedTransfers), [2, 3])
	}

	func testNetworkSettingsExposeCurrentEndpointId() {
		let core = FakeCoreGateway()
		let model = makeModel(core, preferences: Fixtures.preferences())

		core.setState(CoreState(
			isInitialized: true,
			status: CoreStatus(endpointId: "endpoint-for-allowlist", activeTransfers: 0, activeShares: 0)
		))

		XCTAssertEqual(model.state.endpointId, "endpoint-for-allowlist")
	}

	func testApplyCustomRelayRestartsCoreThenPersistsConfiguration() async {
		let core = FakeCoreGateway()
		let preferences = Fixtures.preferences()
		let model = makeModel(core, preferences: preferences)
		model.setRelayMode(.strictCustom)
		model.setRelayURL("  https://relay.example/  ", at: 0)

		model.applyRelayConfiguration()

		await waitUntil { preferences.preferences.relayConfiguration.mode == .strictCustom }
		let expected = RelayConfiguration(mode: .strictCustom, relayURLs: ["https://relay.example"])
		XCTAssertEqual(preferences.preferences.relayConfiguration, expected)
		XCTAssertEqual(core.initializedNetworkConfigurations, [expected])
		XCTAssertFalse(model.state.relayConfigurationIsDirty)
	}

	func testApplyingAutomaticRetainsLastCustomRelayURLs() async {
		let core = FakeCoreGateway()
		let preferences = Fixtures.preferences()
		let relayURLs = ["https://relay.example", "https://backup.example"]
		preferences.setRelayConfiguration(RelayConfiguration(mode: .strictCustom, relayURLs: relayURLs))
		let model = makeModel(core, preferences: preferences)

		model.setRelayMode(.automatic)
		model.applyRelayConfiguration()

		await waitUntil { preferences.preferences.relayConfiguration.mode == .automatic }
		XCTAssertEqual(preferences.preferences.relayConfiguration.relayURLs, relayURLs)
		XCTAssertEqual(core.initializedNetworkConfigurations, [
			RelayConfiguration(mode: .automatic, relayURLs: relayURLs),
		])
		model.setRelayMode(.strictCustom)
		XCTAssertEqual(model.state.relayURLs, relayURLs)
	}

	func testApplyRelayIsBlockedWhileShareIsActive() async {
		let core = FakeCoreGateway()
		let preferences = Fixtures.preferences()
		let model = makeModel(core, preferences: preferences)
		core.setState(CoreState(
			isInitialized: true,
			status: CoreStatus(endpointId: "endpoint", activeTransfers: 0, activeShares: 1)
		))
		model.setRelayMode(.strictCustom)
		model.setRelayURL("https://relay.example", at: 0)

		model.applyRelayConfiguration()
		await Task.yield()

		XCTAssertTrue(core.initializedNetworkConfigurations.isEmpty)
		XCTAssertEqual(preferences.preferences.relayConfiguration, .automatic)
		XCTAssertEqual(model.state.relayApplyErrorKey, "relay_apply_active_transfers")
	}

	func testRepositoryActiveWorkRejectionDoesNotAttemptRollback() async {
		let core = FakeCoreGateway()
		core.initializeResult = .failure(CoreNetworkLifecycleError.activeNetworkWork)
		let preferences = Fixtures.preferences()
		let model = makeModel(core, preferences: preferences)
		let attempted = RelayConfiguration(mode: .strictCustom, relayURLs: ["https://relay.example"])
		model.setRelayMode(.strictCustom)
		model.setRelayURL(attempted.relayURLs[0], at: 0)

		model.applyRelayConfiguration()
		await waitUntil {
			core.initializedNetworkConfigurations.count == 1 && !model.state.isApplyingRelayConfiguration
		}

		XCTAssertEqual(core.initializedNetworkConfigurations, [attempted])
		XCTAssertEqual(preferences.preferences.relayConfiguration, .automatic)
		XCTAssertTrue(model.state.hasActiveNetworkWork)
		XCTAssertEqual(model.state.relayApplyErrorKey, "relay_apply_active_transfers")
	}

	func testFailedRelayApplyRollsBackWithoutPersisting() async {
		let core = FakeCoreGateway()
		core.initializeResults = [.failure(TestError.unimplemented), .success(())]
		let preferences = Fixtures.preferences()
		let model = makeModel(core, preferences: preferences)
		let attempted = RelayConfiguration(mode: .strictCustom, relayURLs: ["https://relay.example"])
		model.setRelayMode(.strictCustom)
		model.setRelayURL(attempted.relayURLs[0], at: 0)

		model.applyRelayConfiguration()
		await waitUntil { core.initializedNetworkConfigurations.count == 2 }

		XCTAssertEqual(core.initializedNetworkConfigurations, [attempted, .automatic])
		XCTAssertEqual(preferences.preferences.relayConfiguration, .automatic)
		XCTAssertEqual(model.state.relayApplyErrorKey, "relay_apply_failed")
	}
}
