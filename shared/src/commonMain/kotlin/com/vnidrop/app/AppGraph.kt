package com.vnidrop.app

import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreRepository
import com.vnidrop.app.diagnostics.DiagnosticsCoordinator
import com.vnidrop.app.diagnostics.createDiagnosticsTransport
import com.vnidrop.app.feature.approvals.ApprovalCoordinator
import com.vnidrop.app.feature.saveddevices.PairingPromptCoordinator
import com.vnidrop.app.feature.saveddevices.TargetedOfferCoordinator
import com.vnidrop.app.feature.send.AppFilePreviewRepository
import com.vnidrop.app.feature.send.createPlatformPreviewStore
import com.vnidrop.app.logging.AppLogger
import com.vnidrop.app.notifications.TransferNotificationCoordinator
import com.vnidrop.app.platform.AppVisibility
import com.vnidrop.app.preferences.AppPreferencesDefaults
import com.vnidrop.app.preferences.AppPreferencesRepository
import com.vnidrop.app.preferences.createAppPreferencesDataStore
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel

class AppGraph(
	val dependencies: AppDependencies,
	private val applicationScope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate),
	val coreRepository: CoreGateway = CoreRepository(),
) {
	val visibility = AppVisibility()
	val messages = UiMessageController()
	val filePreviewRepository = AppFilePreviewRepository(
		createPlatformPreviewStore(dependencies.environment.defaultCoreDataDir),
	)
	val preferencesRepository = AppPreferencesRepository(
		dataStore = createAppPreferencesDataStore(dependencies.environment.defaultCoreDataDir),
		defaults = AppPreferencesDefaults(
			username = dependencies.environment.defaultUsername,
			receiveFolder = dependencies.fileSystemService.defaultReceiveFolder(),
			themeMode = ThemeMode.System,
			notificationsEnabled = false,
		),
	)
	val diagnostics = DiagnosticsCoordinator.create(
		appVersion = dependencies.environment.appVersion,
		platform = dependencies.environment.name,
		preferencesRepository = preferencesRepository,
		scope = applicationScope,
		transport = createDiagnosticsTransport(
			appVersion = dependencies.environment.appVersion,
			platform = dependencies.environment.name,
			installIdProvider = { preferencesRepository.ensureDiagnosticsInstallId() },
		),
	)
	val approvalCoordinator = ApprovalCoordinator(
		repository = coreRepository,
		preferencesRepository = preferencesRepository,
		notifications = dependencies.localNotificationService,
		visibility = visibility,
		messages = messages,
		scope = applicationScope,
	)
	val pairingPromptCoordinator = PairingPromptCoordinator(
		repository = coreRepository,
		preferencesRepository = preferencesRepository,
		messages = messages,
		scope = applicationScope,
	)
	val targetedOfferCoordinator = TargetedOfferCoordinator(
		repository = coreRepository,
		fileSystemService = dependencies.fileSystemService,
		preferencesRepository = preferencesRepository,
		messages = messages,
		scope = applicationScope,
	)
	val transferNotificationCoordinator = TransferNotificationCoordinator(
		repository = coreRepository,
		preferencesRepository = preferencesRepository,
		notifications = dependencies.localNotificationService,
		visibility = visibility,
		messages = messages,
		scope = applicationScope,
	)

	init {
		AppLogger.initialize(dependencies.environment.defaultCoreDataDir)
		diagnostics.start()
	}

	fun close() {
		coreRepository.shutdown()
		applicationScope.cancel()
	}
}
