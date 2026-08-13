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

		Task {
			let result = await repository.initialize(
				appDataDir: appDataDir,
				networkConfiguration: networkConfiguration
			)
			if case .failure(let error) = result {
				if error.hasUnrecoverableEndpointIdentity {
					startupRecovery = .identityUnrecoverable
				} else {
					messages.error(error)
				}
			}
		}

		preferences.$preferences
			.map(\.themeMode)
			.removeDuplicates()
			.sink { [weak self] mode in self?.themeMode = mode }
			.store(in: &cancellables)
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
		case .failure(let error):
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
