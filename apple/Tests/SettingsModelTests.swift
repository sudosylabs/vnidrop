import XCTest
@testable import VniDrop

/// Ports settings assertions from `feature/ViewModelsTest.kt` — username debounce
/// persistence and the Storage "delete all transfers" flow.
@MainActor
final class SettingsModelTests: XCTestCase {

	private func makeModel(
		_ core: FakeCoreGateway,
		preferences: AppPreferencesRepository,
		fileSystem: FakeFileSystemService = FakeFileSystemService(),
		dataDir: String = NSTemporaryDirectory(),
		bugReports: BugReportService = FakeBugReportService()
	) -> SettingsModel {
		SettingsModel(
			environment: PlatformEnvironment(name: "Test", appVersion: "0.1.0", defaultCoreDataDir: dataDir),
			deviceInfoProvider: FakeDeviceInfoProvider(),
			fileSystemService: fileSystem,
			repository: core,
			preferences: preferences,
			notifications: LocalNotificationService(),
			messages: UiMessageController(),
			bugReports: bugReports
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

	func testBugReportRequiresDescriptionsBeforeCallingService() {
		let service = FakeBugReportService()
		let model = makeModel(FakeCoreGateway(), preferences: Fixtures.preferences(), bugReports: service)
		model.submitBugReport()
		XCTAssertEqual(model.state.bugReportError, .resource(L10n.Bug.reportMissingWhat))
		model.setBugWhatHappened("The transfer failed")
		model.submitBugReport()
		XCTAssertEqual(model.state.bugReportError, .resource(L10n.Bug.reportMissingExpected))
		XCTAssertTrue(service.drafts.isEmpty)
		XCTAssertFalse(model.state.isSubmittingBugReport)
	}

	func testBugReportDoesNotSubmitTwiceAndClearsOnlyAfterSuccess() async {
		let service = FakeBugReportService()
		service.suspend = true
		let model = makeModel(FakeCoreGateway(), preferences: Fixtures.preferences(), bugReports: service)
		XCTAssertFalse(model.state.bugIncludeLogs)
		model.setBugWhatHappened("The transfer failed")
		model.setBugExpected("The transfer completes")
		model.setBugIncludeLogs(true)
		var completions = 0
		model.submitBugReport { completions += 1 }
		model.submitBugReport { completions += 1 }
		XCTAssertTrue(model.state.isSubmittingBugReport)
		await waitUntil { service.continuation != nil }
		XCTAssertEqual(service.drafts.count, 1)
		XCTAssertEqual(model.state.bugWhatHappened, "The transfer failed")
		service.continuation?.resume(returning: .success(()))
		await waitUntil { !model.state.isSubmittingBugReport }
		XCTAssertEqual(completions, 1)
		XCTAssertTrue(model.state.bugWhatHappened.isEmpty)
		XCTAssertTrue(model.state.bugExpected.isEmpty)
		XCTAssertFalse(model.state.bugIncludeLogs)
		XCTAssertNil(model.state.bugReportError)
	}

	func testFailedBugReportKeepsDraftAndShowsActionableError() async {
		let service = FakeBugReportService()
		service.result = .failure(BugReportError.rateLimited)
		let model = makeModel(FakeCoreGateway(), preferences: Fixtures.preferences(), bugReports: service)
		model.setBugWhatHappened("The transfer failed")
		model.setBugExpected("The transfer completes")
		model.setBugContact("tester@example.test")
		model.submitBugReport { XCTFail("A failed report must not dismiss the form") }
		await waitUntil { !model.state.isSubmittingBugReport }
		XCTAssertEqual(model.state.bugWhatHappened, "The transfer failed")
		XCTAssertEqual(model.state.bugExpected, "The transfer completes")
		XCTAssertEqual(model.state.bugContact, "tester@example.test")
		XCTAssertEqual(model.state.bugReportError, .resource(L10n.Apple.reportRateLimited))
	}

	func testClosingBugReportIgnoresLateSuccessAndKeepsDraft() async {
		let service = FakeBugReportService()
		service.suspend = true
		let model = makeModel(FakeCoreGateway(), preferences: Fixtures.preferences(), bugReports: service)
		model.setBugWhatHappened("The transfer failed")
		model.setBugExpected("The transfer completes")
		model.submitBugReport { XCTFail("A closed form must not react to late success") }
		await waitUntil { service.continuation != nil }
		model.cancelBugReport()
		service.continuation?.resume(returning: .success(()))
		await waitUntil { !model.state.isSubmittingBugReport }
		XCTAssertEqual(model.state.bugWhatHappened, "The transfer failed")
		XCTAssertNil(model.state.bugReportError)
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

	// MARK: - Automatic trash purge

	/// A root holding `.Trash/<bytes>` plus a sibling file that must survive.
	private func makeRootWithTrash(bytes: Int) throws -> (root: String, keeper: String) {
		let root = NSTemporaryDirectory() + "vnidrop-trash-\(UUID().uuidString)"
		let trash = root + "/.Trash"
		try FileManager.default.createDirectory(atPath: trash, withIntermediateDirectories: true)
		FileManager.default.createFile(atPath: trash + "/deleted.bin", contents: Data(count: bytes))
		let keeper = root + "/kept.bin"
		FileManager.default.createFile(atPath: keeper, contents: Data(count: 8))
		return (root, keeper)
	}

	func testPurgeTrashRemovesTrashAndLeavesTheRestAlone() throws {
		let (root, keeper) = try makeRootWithTrash(bytes: 4096)

		let freed = SettingsModel.purgeTrash(under: [root, root, "/nonexistent-\(UUID().uuidString)"])

		XCTAssertGreaterThanOrEqual(freed, 4096)
		XCTAssertFalse(FileManager.default.fileExists(atPath: root + "/.Trash"))
		XCTAssertTrue(FileManager.default.fileExists(atPath: keeper))
		try? FileManager.default.removeItem(atPath: root)
	}

	func testTrashIsPurgedAutomaticallyWhereTheUserCannotReachIt() async throws {
		let (dataDir, _) = try makeRootWithTrash(bytes: 2048)
		let (receiveDir, _) = try makeRootWithTrash(bytes: 2048)
		let fileSystem = FakeFileSystemService()
		fileSystem.userCanReachTrash = false
		fileSystem.folder = ReceiveFolder(kind: .fileSystemPath, value: receiveDir, displayName: "Documents")
		let model = makeModel(
			FakeCoreGateway(), preferences: Fixtures.preferences(),
			fileSystem: fileSystem, dataDir: dataDir
		)

		model.purgeUnreachableTrash()
		// Both roots are purged, so waiting on only one races the other: on a
		// loaded machine the receive folder's trash can still be there when the
		// data directory's has already gone.
		await waitUntil {
			!FileManager.default.fileExists(atPath: dataDir + "/.Trash")
				&& !FileManager.default.fileExists(atPath: receiveDir + "/.Trash")
		}

		XCTAssertFalse(FileManager.default.fileExists(atPath: dataDir + "/.Trash"))
		XCTAssertFalse(FileManager.default.fileExists(atPath: receiveDir + "/.Trash"))
		try? FileManager.default.removeItem(atPath: dataDir)
		try? FileManager.default.removeItem(atPath: receiveDir)
	}

	/// macOS: the trash is the user's, so only the explicit action may empty it.
	func testTrashIsLeftAloneWhereTheUserCanReachIt() async throws {
		let (dataDir, _) = try makeRootWithTrash(bytes: 2048)
		let fileSystem = FakeFileSystemService()
		fileSystem.userCanReachTrash = true
		let model = makeModel(
			FakeCoreGateway(), preferences: Fixtures.preferences(),
			fileSystem: fileSystem, dataDir: dataDir
		)

		model.purgeUnreachableTrash()
		try await Task.sleep(nanoseconds: 100_000_000)

		XCTAssertTrue(FileManager.default.fileExists(atPath: dataDir + "/.Trash"))
		try? FileManager.default.removeItem(atPath: dataDir)
	}
}
