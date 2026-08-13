import Foundation
import Combine

/// Top-level app state, ported from `feature/app/AppViewModel.kt`. Initializes the
/// core on launch and tracks the selected destination + theme.
@MainActor
final class AppModel: ObservableObject {
	@Published private(set) var destination: AppDestination = .send
	@Published private(set) var themeMode: ThemeMode = .system
	/// Why startup failed, or nil while it is still in progress or has succeeded.
	/// The startup overlay covers the snackbar, so without this a failed
	/// `initialize` was indistinguishable from an app that never finished loading.
	@Published private(set) var startupError: UiText?
	/// Untranslated failure detail, kept for the debug overlay only. The friendly
	/// message alone cannot distinguish a missing keychain item from a database
	/// fault, which makes a startup failure undiagnosable on a real device.
	@Published private(set) var startupErrorDetail: String?

	private let environment: PlatformEnvironment
	private let repository: CoreGateway
	private let messages: UiMessageController
	private let relayConfiguration: RelayConfiguration
	private var cancellables = Set<AnyCancellable>()

	init(
		environment: PlatformEnvironment,
		repository: CoreGateway,
		preferences: AppPreferencesRepository,
		messages: UiMessageController
	) {
		self.environment = environment
		self.repository = repository
		self.messages = messages
		self.relayConfiguration = preferences.preferences.relayConfiguration

		AppLogger.info("lifecycle", "app started", ["platform": environment.name])

		Task { await initializeCore() }

		preferences.$preferences
			.map(\.themeMode)
			.removeDuplicates()
			.sink { [weak self] mode in self?.themeMode = mode }
			.store(in: &cancellables)
	}

	/// Runs core startup, keeping the failure reason for the overlay to show.
	/// Also logged, because a user-facing message alone is not diagnosable.
	func initializeCore() async {
		startupError = nil
		startupErrorDetail = nil
		let result = await repository.initialize(
			appDataDir: environment.defaultCoreDataDir,
			networkConfiguration: relayConfiguration
		)
		if case .failure(let error) = result {
			AppLogger.error("lifecycle", "core initialization failed", error)
			startupError = error.toUiText()
			#if DEBUG
			startupErrorDetail = error.technicalDetail
			#endif
			messages.error(error)
		}
	}

	func retryStartup() {
		guard startupError != nil else { return }
		Task { await initializeCore() }
	}

	func selectDestination(_ destination: AppDestination) {
		guard destination != self.destination else { return }
		self.destination = destination
	}
}
