import Foundation
import Combine

/// Object graph wiring the repositories and coordinators together, ported from
/// `AppGraph.kt`. Owned by the app root for the process lifetime.
@MainActor
final class AppGraph: ObservableObject {
	let dependencies: AppDependencies
	let coreRepository: CoreRepository
	/// The gateway the feature models and coordinators observe. Normally the real
	/// `coreRepository`; in screenshot builds a fixture is injected so the UI shows
	/// deterministic content without the Rust core (see `ScreenshotSupport`).
	let gateway: CoreGateway
	let visibility = AppVisibility()
	let messages = UiMessageController()
	let preferencesRepository: AppPreferencesRepository
	let filePreviewRepository: FilePreviewRepository
	let approvalCoordinator: ApprovalCoordinator
	let transferNotificationCoordinator: TransferNotificationCoordinator
	let backgroundActivity: BackgroundActivityController

	init(dependencies: AppDependencies, coreRepository: CoreRepository? = nil, coreGateway: CoreGateway? = nil) {
		self.dependencies = dependencies
		let coreRepository = coreRepository ?? CoreRepository()
		self.coreRepository = coreRepository
		let gateway = coreGateway ?? coreRepository
		self.gateway = gateway
		self.filePreviewRepository = FilePreviewRepository(appDataDir: dependencies.environment.defaultCoreDataDir)
		self.preferencesRepository = AppPreferencesRepository(
			fallback: AppPreferencesDefaults(
				username: dependencies.environment.defaultUsername,
				receiveFolder: dependencies.fileSystemService.defaultReceiveFolder(),
				themeMode: .system
			)
		)
		self.approvalCoordinator = ApprovalCoordinator(
			repository: gateway,
			notifications: dependencies.notificationService,
			visibility: visibility,
			messages: messages
		)
		self.transferNotificationCoordinator = TransferNotificationCoordinator(
			repository: gateway,
			notifications: dependencies.notificationService,
			visibility: visibility,
			messages: messages
		)
		self.backgroundActivity = BackgroundActivityController(repository: coreRepository)
		AppLogger.info("lifecycle", "graph created", ["platform": dependencies.environment.name])
	}

	func close() {
		coreRepository.shutdown()
	}
}
