import Foundation
import Combine
import VnidropCore

enum AppStartupRecovery: Equatable {
	case identityUnrecoverable
}

/// Top-level app state, ported from `feature/app/AppViewModel.kt`. Initializes the
/// core on launch and tracks the selected destination + theme.
@MainActor
final class AppModel: ObservableObject {
	@Published private(set) var destination: AppDestination = .send
	@Published private(set) var themeMode: ThemeMode = .system
	/// Why startup failed, or nil while it is still in progress or has succeeded.
	/// The startup overlay covers the snackbar, so without this a failed
	/// `initialize` was indistinguishable from an app that never finished loading.
	/// Stays nil for failures `startupRecovery` can offer a repair for, so the
	/// user is shown the repair rather than a dead end.
	@Published private(set) var startupError: UiText?
	/// Untranslated failure detail, kept for the debug overlay only. The friendly
	/// message alone cannot distinguish a missing keychain item from a database
	/// fault, which makes a startup failure undiagnosable on a real device.
	@Published private(set) var startupErrorDetail: String?
	@Published private(set) var startupRecovery: AppStartupRecovery?
	@Published private(set) var isResettingIdentity = false

	private let environment: PlatformEnvironment
	private let repository: CoreGateway
	private let messages: UiMessageController
	private let appDataDir: String
	private let networkConfiguration: RelayConfiguration
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
		self.appDataDir = environment.defaultCoreDataDir
		self.networkConfiguration = preferences.preferences.relayConfiguration

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
		startupRecovery = nil
		let result = await repository.initialize(
			appDataDir: appDataDir,
			networkConfiguration: networkConfiguration
		)
		if case .failure(let error) = result {
			AppLogger.error("lifecycle", "core initialization failed", error)
			// A repairable identity gets the reset flow instead of a generic
			// failure, which would offer only a retry that cannot succeed.
			if error.hasUnrecoverableEndpointIdentity {
				startupRecovery = .identityUnrecoverable
				return
			}
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

	func resetUnrecoverableIdentity() async {
		guard startupRecovery == .identityUnrecoverable, !isResettingIdentity else { return }
		isResettingIdentity = true
		defer { isResettingIdentity = false }
		let result = await repository.resetUnrecoverableIdentity(
			appDataDir: appDataDir,
			networkConfiguration: networkConfiguration
		)
		switch result {
		case .success:
			startupRecovery = nil
			startupError = nil
			startupErrorDetail = nil
		case .failure(let error):
			AppLogger.error("lifecycle", "identity reset failed", error)
			messages.error(error)
		}
	}
}

private extension Error {
	var hasUnrecoverableEndpointIdentity: Bool {
		guard let error = self as? VnidropError else { return false }
		switch error {
		case .SecureStorageMissing, .SecureStorageCorrupted: return true
		default: return false
		}
	}
}
