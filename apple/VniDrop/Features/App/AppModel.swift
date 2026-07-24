import Foundation
import Combine

/// Top-level app state, ported from `feature/app/AppViewModel.kt`. Initializes the
/// core on launch and tracks the selected destination + theme.
@MainActor
final class AppModel: ObservableObject {
	@Published private(set) var destination: AppDestination = .send
	@Published private(set) var themeMode: ThemeMode = .system

	private let environment: PlatformEnvironment
	private let repository: CoreGateway
	private let messages: UiMessageController
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

		AppLogger.info("lifecycle", "app started", ["platform": environment.name])

		Task {
			let result = await repository.initialize(
				appDataDir: environment.defaultCoreDataDir,
				networkConfiguration: preferences.preferences.relayConfiguration
			)
			if case .failure(let error) = result { messages.error(error) }
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
}
