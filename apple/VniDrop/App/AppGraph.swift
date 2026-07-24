import Foundation
import Combine

/// Object graph wiring the repositories and coordinators together, ported from
/// `AppGraph.kt`. Owned by the app root for the process lifetime.
@MainActor
final class AppGraph: ObservableObject {
	let dependencies: AppDependencies
	let coreRepository: CoreRepository
	let visibility = AppVisibility()
	let messages = UiMessageController()
	let preferencesRepository: AppPreferencesRepository
	let filePreviewRepository: FilePreviewRepository
	let approvalCoordinator: ApprovalCoordinator
	let transferNotificationCoordinator: TransferNotificationCoordinator

	init(dependencies: AppDependencies, coreRepository: CoreRepository? = nil) {
		self.dependencies = dependencies
		let coreRepository = coreRepository ?? CoreRepository()
		self.coreRepository = coreRepository
		self.filePreviewRepository = FilePreviewRepository(appDataDir: dependencies.environment.defaultCoreDataDir)
		self.preferencesRepository = AppPreferencesRepository(
			fallback: AppPreferencesDefaults(
				username: dependencies.environment.defaultUsername,
				receiveFolder: dependencies.fileSystemService.defaultReceiveFolder(),
				themeMode: .system,
				diagnosticsEnabled: false
			)
		)
		self.approvalCoordinator = ApprovalCoordinator(
			repository: coreRepository,
			notifications: dependencies.notificationService,
			visibility: visibility,
			messages: messages
		)
		self.transferNotificationCoordinator = TransferNotificationCoordinator(
			repository: coreRepository,
			notifications: dependencies.notificationService,
			visibility: visibility,
			messages: messages
		)
		AppLogger.info("lifecycle", "graph created", ["platform": dependencies.environment.name])
	}

	func close() {
		coreRepository.shutdown()
	}
}
