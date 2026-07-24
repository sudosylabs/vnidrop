package com.vnidrop.app.feature

import com.vnidrop.app.DeviceInfo
import com.vnidrop.app.PlatformEnvironment
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.CoreStatus
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.PickedShareFile
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.core.ReceiverDeliveryStatus
import com.vnidrop.app.core.ReceiverRequestModel
import com.vnidrop.app.core.RelayMode
import com.vnidrop.app.core.RelaySettings
import com.vnidrop.app.core.Share
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.diagnostics.BreadcrumbBuffer
import com.vnidrop.app.diagnostics.BugReportService
import com.vnidrop.app.diagnostics.DiagnosticsTransport
import com.vnidrop.app.diagnostics.NoOpDiagnosticsTransport
import com.vnidrop.app.diagnostics.RecordingDiagnosticsTransport
import com.vnidrop.app.feature.app.AppViewModel
import com.vnidrop.app.feature.receive.ReceiveHistoryDeleteTarget
import com.vnidrop.app.feature.receive.ReceiveViewModel
import com.vnidrop.app.feature.send.SendViewModel
import com.vnidrop.app.feature.settings.SettingsSection
import com.vnidrop.app.feature.settings.RelaySettingsApplyError
import com.vnidrop.app.feature.settings.RelaySettingsInputError
import com.vnidrop.app.feature.settings.SettingsViewModel
import com.vnidrop.app.notifications.NotificationPermission
import com.vnidrop.app.preferences.AppPreferences
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.support.FakeFileSystemService
import com.vnidrop.app.support.FakeFilePreviewRepository
import com.vnidrop.app.support.FakeNotificationService
import com.vnidrop.app.support.FakePreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiText
import com.vnidrop.app.ui.navigation.AppDestination
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertContentEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue
import uniffi.vnidrop.VnidropException
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_show_in_files
import vnidrop.shared.generated.resources.error_permission
import vnidrop.shared.generated.resources.error_destination_exists
import vnidrop.shared.generated.resources.receive_open_files_failed

@OptIn(ExperimentalCoroutinesApi::class)
class ViewModelsTest {
	@AfterTest
	fun resetDispatcher() {
		Dispatchers.resetMain()
	}

	@Test
	fun appViewModelInitializesCoreAndOwnsNavigation() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway()
		val viewModel = AppViewModel(environment(), core, preferences(), UiMessageController())
		advanceUntilIdle()
		assertTrue(core.state.value.isInitialized)
		assertEquals(listOf(RelaySettings()), core.initializedRelaySettings)
		viewModel.selectDestination(AppDestination.Settings)
		assertEquals(AppDestination.Settings, viewModel.state.value.destination)
	}

	@Test
	fun appViewModelInitializesCoreWithSavedRelaySettings() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val custom = RelaySettings(RelayMode.StrictCustom, listOf("https://relay.example.com"))
		val preferences = preferences().apply {
			mutablePreferences.value = mutablePreferences.value.copy(relaySettings = custom)
		}
		val core = FakeCoreGateway()

		AppViewModel(environment(), core, preferences, UiMessageController())
		advanceUntilIdle()

		assertEquals(listOf(custom), core.initializedRelaySettings)
	}

	@Test
	fun settingsEnablesNotificationsOnlyAfterPermission() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val preferences = preferences()
		val notifications = FakeNotificationService(NotificationPermission.Granted)
		val viewModel = settingsViewModel(preferences, notifications)
		advanceUntilIdle()
		viewModel.setNotificationsEnabled(true)
		advanceUntilIdle()
		assertTrue(preferences.mutablePreferences.value.notificationsEnabled)
		viewModel.setNotificationsEnabled(false)
		advanceUntilIdle()
		assertFalse(preferences.mutablePreferences.value.notificationsEnabled)
		assertEquals(1, notifications.cancelAllCount)
	}

	@Test
	fun settingsUsernameDraftIsNotOverwrittenByPersistedEcho() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val preferences = preferences()
		val viewModel = settingsViewModel(preferences)
		advanceUntilIdle()

		viewModel.setUsername("Ada ")
		// Immediate local state must keep the trailing space so multi-word names can be typed.
		assertEquals("Ada ", viewModel.state.value.username)
		assertEquals("Receiver", preferences.mutablePreferences.value.username)

		testScheduler.advanceTimeBy(400)
		advanceUntilIdle()
		assertEquals("Ada", preferences.mutablePreferences.value.username)
		assertEquals("Ada ", viewModel.state.value.username)

		preferences.setThemeMode(ThemeMode.Dark)
		advanceUntilIdle()
		assertEquals("Ada ", viewModel.state.value.username)
	}

	@Test
	fun settingsClearsTransferCacheExplicitly() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			clearTransferCacheResult = Result.success(3_700_000_000UL)
		}
		val viewModel = settingsViewModel(repository = core)
		advanceUntilIdle()

		viewModel.clearTransferCache()
		advanceUntilIdle()

		assertEquals(1, core.clearTransferCacheCount)
		assertFalse(viewModel.state.value.isClearingTransferCache)
	}

	@Test
	fun settingsDoesNotClearTransferCacheDuringActiveNetworkWork() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = CoreState(status = CoreStatus("endpoint", 1UL, 0UL))
		}
		val viewModel = settingsViewModel(repository = core)
		advanceUntilIdle()

		viewModel.clearTransferCache()
		advanceUntilIdle()

		assertEquals(0, core.clearTransferCacheCount)
	}

	@Test
	fun settingsDeleteAllTransfersImmediatelyClearsUnusedCache() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = CoreState(
				isInitialized = true,
				transfers = listOf(receivedTransfer(41UL, TransferStatus.Done)),
			)
		}
		val viewModel = settingsViewModel(repository = core)
		advanceUntilIdle()

		viewModel.deleteAllTransfers()
		advanceUntilIdle()

		assertEquals(listOf(41UL), core.deletedTransfers)
		assertEquals(1, core.clearTransferCacheCount)
		assertFalse(viewModel.state.value.isDeletingTransfers)
	}

	@Test
	fun settingsAppliesNormalizedCustomRelaysAndPersistsThem() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val preferences = preferences()
		val core = FakeCoreGateway()
		val viewModel = settingsViewModel(preferences = preferences, repository = core)
		advanceUntilIdle()

		viewModel.setRelayMode(RelayMode.StrictCustom)
		viewModel.setRelayUrl(0, " HTTPS://Relay.Example.com/ ")
		viewModel.addRelayUrl()
		viewModel.setRelayUrl(1, "https://backup.example.com:443")
		viewModel.applyRelaySettings()
		advanceUntilIdle()

		val expected = RelaySettings(
			RelayMode.StrictCustom,
			listOf("https://relay.example.com", "https://backup.example.com"),
		)
		assertEquals(expected, preferences.mutablePreferences.value.relaySettings)
		assertEquals(listOf(expected), core.initializedRelaySettings)
		assertEquals(expected, viewModel.state.value.savedRelaySettings)
		assertFalse(viewModel.state.value.hasRelaySettingsChanges)
	}

	@Test
	fun settingsRejectsInvalidRelayUrlWithoutEnteringLoadingState() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway()
		val viewModel = settingsViewModel(repository = core)
		advanceUntilIdle()

		viewModel.setRelayMode(RelayMode.StrictCustom)
		viewModel.setRelayUrl(0, "relay.example.com")
		viewModel.applyRelaySettings()

		assertFalse(viewModel.state.value.isApplyingRelaySettings)
		assertEquals(RelaySettingsInputError.HttpsRequired(1), viewModel.state.value.relayInputError)
		assertEquals(emptyList(), core.initializedRelaySettings)
	}

	@Test
	fun settingsEditsCustomRelaysAsIndividualRows() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val viewModel = settingsViewModel()
		advanceUntilIdle()

		viewModel.setRelayMode(RelayMode.StrictCustom)
		viewModel.setRelayUrl(0, "https://first.example.com")
		viewModel.addRelayUrl()
		viewModel.setRelayUrl(1, "https://second.example.com")
		viewModel.removeRelayUrl(0)

		assertEquals(listOf("https://second.example.com"), viewModel.state.value.relayUrls)
	}

	@Test
	fun settingsAppliesCustomFallbackAndLocalOnlyWhileRetainingRelayDrafts() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val preferences = preferences()
		val core = FakeCoreGateway()
		val viewModel = settingsViewModel(preferences = preferences, repository = core)
		advanceUntilIdle()

		viewModel.setRelayMode(RelayMode.CustomWithDirectFallback)
		viewModel.setRelayUrl(0, "https://relay.example.com")
		viewModel.applyRelaySettings()
		advanceUntilIdle()
		viewModel.setRelayMode(RelayMode.LocalOnly)
		viewModel.applyRelaySettings()
		advanceUntilIdle()

		val fallback = RelaySettings(
			RelayMode.CustomWithDirectFallback,
			listOf("https://relay.example.com"),
		)
		val localOnly = RelaySettings(
			RelayMode.LocalOnly,
			listOf("https://relay.example.com"),
		)
		assertEquals(listOf(fallback, localOnly), core.initializedRelaySettings)
		assertEquals(localOnly, preferences.mutablePreferences.value.relaySettings)
		assertFalse(viewModel.state.value.hasRelaySettingsChanges)
	}

	@Test
	fun settingsIgnoresDuplicateApplyBeforeRestartBegins() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway()
		val viewModel = settingsViewModel(repository = core)
		advanceUntilIdle()

		viewModel.setRelayMode(RelayMode.StrictCustom)
		viewModel.setRelayUrl(0, "https://relay.example.com")
		viewModel.applyRelaySettings()
		viewModel.applyRelaySettings()
		assertTrue(viewModel.state.value.isApplyingRelaySettings)
		advanceUntilIdle()

		assertEquals(1, core.initializedRelaySettings.size)
	}

	@Test
	fun settingsDoesNotRestartNetworkWhileTransfersAreActive() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = CoreState(status = CoreStatus("endpoint", 0UL, 1UL))
		}
		val viewModel = settingsViewModel(repository = core)
		advanceUntilIdle()
		assertEquals("endpoint", viewModel.state.value.endpointId)

		viewModel.setRelayMode(RelayMode.StrictCustom)
		viewModel.setRelayUrl(0, "https://relay.example.com")
		viewModel.applyRelaySettings()
		advanceUntilIdle()

		assertEquals(emptyList(), core.initializedRelaySettings)
		assertEquals(RelaySettingsApplyError.ActiveTransfers, viewModel.state.value.relayApplyError)
	}

	@Test
	fun settingsRestoresPreviousNetworkConfigurationWhenApplyFails() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			initializeHandler = { settings ->
				if (settings.mode == RelayMode.StrictCustom) Result.failure(IllegalStateException("unreachable"))
				else Result.success(Unit)
			}
		}
		val preferences = preferences()
		val viewModel = settingsViewModel(preferences = preferences, repository = core)
		advanceUntilIdle()

		viewModel.setRelayMode(RelayMode.StrictCustom)
		viewModel.setRelayUrl(0, "https://relay.example.com")
		viewModel.applyRelaySettings()
		advanceUntilIdle()

		assertEquals(
			listOf(
				RelaySettings(RelayMode.StrictCustom, listOf("https://relay.example.com")),
				RelaySettings(),
			),
			core.initializedRelaySettings,
		)
		assertEquals(RelaySettings(), preferences.mutablePreferences.value.relaySettings)
		assertEquals(RelaySettingsApplyError.ApplyFailed, viewModel.state.value.relayApplyError)
	}

	@Test
	fun settingsKeepsNotificationsDisabledWhenPermissionIsDenied() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val preferences = preferences()
		val viewModel = settingsViewModel(preferences, FakeNotificationService(NotificationPermission.Denied))
		advanceUntilIdle()
		viewModel.setNotificationsEnabled(true)
		advanceUntilIdle()
		assertFalse(preferences.mutablePreferences.value.notificationsEnabled)
	}

	@Test
	fun settingsCompletesNotificationOptInAfterSystemSettingsGrant() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val preferences = preferences()
		val notifications = FakeNotificationService(NotificationPermission.Denied)
		val viewModel = settingsViewModel(preferences, notifications)
		advanceUntilIdle()

		viewModel.openNotificationSettings()
		advanceUntilIdle()
		assertEquals(1, notifications.openSettingsCount)

		notifications.mutablePermission.value = NotificationPermission.Granted
		viewModel.refreshNotificationPermission()
		advanceUntilIdle()

		assertTrue(preferences.mutablePreferences.value.notificationsEnabled)
		assertEquals(NotificationPermission.Granted, viewModel.state.value.notificationPermission)
	}

	@Test
	fun settingsReportsUnsupportedNotificationPlatforms() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val preferences = preferences()
		val viewModel = settingsViewModel(preferences, FakeNotificationService(NotificationPermission.Unsupported))
		advanceUntilIdle()
		viewModel.setNotificationsEnabled(true)
		advanceUntilIdle()
		assertFalse(preferences.mutablePreferences.value.notificationsEnabled)
		assertEquals(NotificationPermission.Unsupported, viewModel.state.value.notificationPermission)
	}

	@Test
	fun settingsTogglesDiagnosticsPreference() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val preferences = preferences()
		val viewModel = settingsViewModel(preferences, diagnosticsIncluded = true)
		advanceUntilIdle()
		assertFalse(viewModel.state.value.diagnosticsEnabled)
		viewModel.setDiagnosticsEnabled(true)
		advanceUntilIdle()
		assertTrue(preferences.mutablePreferences.value.diagnosticsEnabled)
		assertTrue(viewModel.state.value.diagnosticsEnabled)
	}

	@Test
	fun settingsIgnoresDiagnosticsOptInWhenExcluded() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val preferences = preferences()
		val viewModel = settingsViewModel(preferences)
		advanceUntilIdle()

		viewModel.setDiagnosticsEnabled(true)
		advanceUntilIdle()

		assertFalse(preferences.mutablePreferences.value.diagnosticsEnabled)
		assertFalse(viewModel.state.value.diagnosticsEnabled)
	}

	@Test
	fun settingsSubmitsBugReportAndClearsForm() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val preferences = preferences()
		val viewModel = settingsViewModel(preferences)
		advanceUntilIdle()
		viewModel.setBugWhatHappened("Transfer stuck")
		viewModel.setBugExpected("It should finish")
		viewModel.submitBugReport()
		advanceUntilIdle()
		assertEquals("", viewModel.state.value.bugWhatHappened)
		assertEquals("", viewModel.state.value.bugExpected)
	}

	@Test
	fun settingsKeepsBugReportWhenDeliveryIsUnavailable() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val viewModel = settingsViewModel(transport = NoOpDiagnosticsTransport())
		advanceUntilIdle()
		viewModel.selectSection(SettingsSection.BugReport)
		viewModel.setBugWhatHappened("Transfer stuck")
		viewModel.setBugExpected("It should finish")

		viewModel.submitBugReport()
		advanceUntilIdle()

		assertEquals("Transfer stuck", viewModel.state.value.bugWhatHappened)
		assertEquals("It should finish", viewModel.state.value.bugExpected)
		assertEquals(SettingsSection.BugReport, viewModel.state.value.selectedSection)
		assertFalse(viewModel.state.value.isSubmittingBugReport)
	}

	@Test
	fun settingsUsesDefaultReceiveFolderWhenPlatformDoesNotSupportCustomFolders() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val appDocuments = ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/app/Documents", "Documents")
		val externalFolder = ReceiveFolder(ReceiveFolderKind.AndroidTreeUri, "content://external", "External")
		val preferences = preferences().apply {
			mutablePreferences.value = mutablePreferences.value.copy(receiveFolder = externalFolder)
		}
		val fileSystem = FakeFileSystemService(appDocuments).apply { supportsCustomFolders = false }
		val viewModel = settingsViewModel(preferences = preferences, fileSystem = fileSystem)

		advanceUntilIdle()

		assertFalse(viewModel.state.value.supportsCustomReceiveFolders)
		assertEquals(appDocuments, viewModel.state.value.receiveFolder)
		assertEquals(com.vnidrop.app.core.FolderAccessStatus.Writable, viewModel.state.value.folderAccessStatus)
		viewModel.chooseReceiveFolder()
		assertEquals(null, withTimeoutOrNull(1) { viewModel.effectFlow.first() })
	}

	@Test
	fun sendViewModelOwnsSelectedFileState() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val fileSystem = FakeFileSystemService(folder)
		val viewModel = SendViewModel(FakeCoreGateway(), fileSystem, preferences(), FakeFilePreviewRepository(), UiMessageController())
		viewModel.openComposer()
		val selected = PickedShareFile("/tmp/photo.jpg", "photo.jpg", 42UL, isTemporaryCopy = true)
		viewModel.onFilesPicked(listOf(selected))
		assertEquals("photo.jpg", viewModel.state.value.transferName)
		assertEquals(42UL, viewModel.state.value.selectedFile?.sizeBytes)
		viewModel.clearSelectedSource()
		advanceUntilIdle()
		assertEquals(null, viewModel.state.value.selectedFile)
		assertEquals(listOf(selected), fileSystem.discardedPickedFiles)
	}

	@Test
	fun sendViewModelTracksReceiverCompletionForCatalogProgress() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val accepted = ReceiverRequestModel(
			id = "request-7",
			transferId = 7UL,
			remoteEndpointId = "peer-a",
			transferName = "Photo",
			receiverName = "Receiver",
			receiverDeviceName = null,
			appVersion = "1.0",
			status = ReceiverDeliveryStatus.Accepted,
			reason = null,
			requestedAt = 1L,
			respondedAt = 2L,
			completedAt = null,
		)
		val core = FakeCoreGateway().apply {
			mutableState.value = CoreState(isInitialized = true, transfers = listOf(sentTransfer(7UL)))
			requests[7UL] = listOf(accepted)
		}
		val viewModel = SendViewModel(core, FakeFileSystemService(folder), preferences(), FakeFilePreviewRepository(), UiMessageController())
		advanceUntilIdle()
		assertEquals(ReceiverDeliveryStatus.Accepted, viewModel.state.value.receiversByTransfer.getValue(7UL).single().status)

		core.requests[7UL] = listOf(accepted.copy(status = ReceiverDeliveryStatus.Completed, completedAt = 3L))
		core.mutableSignals.emit(CoreSignal.ReceiverHistoryChanged(7UL))
		advanceUntilIdle()

		assertEquals(ReceiverDeliveryStatus.Completed, viewModel.state.value.receiversByTransfer.getValue(7UL).single().status)
	}

	@Test
	fun sendComposerClosesAfterSuccessfulAtomicShareCreation() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = CoreState(isInitialized = true)
			shareResult = Result.success(Share(7UL, "ticket", "photo.jpg", "hash", 1UL, 42UL))
		}
		val previews = FakeFilePreviewRepository()
		val fileSystem = FakeFileSystemService(folder)
		val viewModel = SendViewModel(core, fileSystem, preferences(), previews, UiMessageController())
		advanceUntilIdle()
		viewModel.openComposer()
		val thumbnail = ByteArray(12).also {
			it[0] = 0x89.toByte(); it[1] = 'P'.code.toByte(); it[2] = 'N'.code.toByte(); it[3] = 'G'.code.toByte()
		}
		val selected = PickedShareFile("/tmp/photo.jpg", "photo.jpg", 42UL, thumbnail, isTemporaryCopy = true)
		viewModel.onFilesPicked(listOf(selected))
		viewModel.setAccessPolicy(ShareAccessPolicy.AnyoneWithTransfer)
		viewModel.createShare()
		advanceUntilIdle()

		assertFalse(viewModel.state.value.isComposerOpen)
		assertEquals(null, viewModel.state.value.selectedFile)
		assertEquals(ShareAccessPolicy.AnyoneWithTransfer, core.lastShareAccessPolicy)
		assertEquals(7UL, core.state.value.transfers.first().transferId)
		assertContentEquals(thumbnail, previews.previews.value.getValue(7UL))
		assertEquals(listOf(selected), fileSystem.discardedPickedFiles)
	}

	@Test
	fun sendComposerStaysOpenWhenShareCreationFails() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply { mutableState.value = CoreState(isInitialized = true) }
		val viewModel = SendViewModel(core, FakeFileSystemService(folder), preferences(), FakeFilePreviewRepository(), UiMessageController())
		advanceUntilIdle()
		viewModel.openComposer()
		viewModel.onFilesPicked(listOf(com.vnidrop.app.core.PickedShareFile("/tmp/photo.jpg", "photo.jpg", 42UL)))
		viewModel.createShare()
		advanceUntilIdle()

		assertTrue(viewModel.state.value.isComposerOpen)
		assertEquals("photo.jpg", viewModel.state.value.selectedFile?.displayName)
		assertFalse(viewModel.state.value.isSharing)
	}

	@Test
	fun sendViewModelSupportsMultipleFilesAndDefaultName() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = CoreState(isInitialized = true)
			shareResult = Result.success(Share(9UL, "ticket", "2 files", "hash", 2UL, 84UL))
		}
		val viewModel = SendViewModel(core, FakeFileSystemService(folder), preferences(), FakeFilePreviewRepository(), UiMessageController())
		advanceUntilIdle()
		viewModel.onFilesPicked(
			listOf(
				com.vnidrop.app.core.PickedShareFile("/tmp/a.jpg", "a.jpg", 40UL),
				com.vnidrop.app.core.PickedShareFile("/tmp/b.jpg", "b.jpg", 44UL),
			),
		)
		assertEquals("2 files", viewModel.state.value.transferName)
		assertEquals(2, viewModel.state.value.selectedFiles.size)
		viewModel.createShare()
		advanceUntilIdle()
		assertEquals(2, core.lastShareSourceCount)
		assertTrue(viewModel.state.value.selectedFiles.isEmpty())
	}

	@Test
	fun sendViewModelNamesFolderSelectionAfterFolderDisplayName() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val viewModel = SendViewModel(FakeCoreGateway(), FakeFileSystemService(folder), preferences(), FakeFilePreviewRepository(), UiMessageController())
		advanceUntilIdle()
		viewModel.onFilesPicked(
			listOf(
				com.vnidrop.app.core.PickedShareFile(
					value = "/tmp/photos",
					displayName = "photos",
					isDirectory = true,
				),
			),
		)
		assertEquals("photos", viewModel.state.value.transferName)
		assertTrue(viewModel.state.value.selectedFiles.single().isDirectory)
	}

	@Test
	fun sendDeletionRemovesCoreTransferAndOwnedPreview() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = CoreState(isInitialized = true, transfers = listOf(
				com.vnidrop.app.core.Transfer(
					localId = "send-7", transferId = 7UL,
					direction = com.vnidrop.app.core.TransferDirection.Send,
					status = com.vnidrop.app.core.TransferStatus.Sharing,
					peerId = null, transferName = "Photo", contentHash = "hash",
					fileCount = 1UL, totalSize = 42UL, ticket = "ticket",
					accessPolicy = ShareAccessPolicy.RequireApproval, createdAt = 1, updatedAt = 1,
				),
			))
		}
		val previews = FakeFilePreviewRepository()
		previews.save(7UL, byteArrayOf(1, 2, 3))
		val viewModel = SendViewModel(core, FakeFileSystemService(folder), preferences(), previews, UiMessageController())
		advanceUntilIdle()
		viewModel.openTransfer(7UL)
		viewModel.requestDeleteTransfer()
		viewModel.confirmDeleteTransfer()
		advanceUntilIdle()

		assertEquals(listOf(7UL), core.deletedTransfers)
		assertFalse(7UL in previews.previews.value)
		assertEquals(null, viewModel.state.value.selectedTransferId)
		assertFalse(viewModel.state.value.isDeleteConfirmationOpen)
	}

	@Test
	fun receiveViewModelBuildsStateFromPreferences() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			inspectionResult = Result.success(sampleTicketInspection())
		}
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), UiMessageController())
		advanceUntilIdle()
		viewModel.onInvitationResult(com.vnidrop.app.feature.receive.ReceiveMethod.InvitationFile, Result.success("ticket"))
		advanceUntilIdle()
		assertTrue(viewModel.state.value.canReceive(coreInitialized = true))
		assertEquals("Receiver", viewModel.state.value.receiverName)
	}

	@Test
	fun receiveViewModelDeletesOneTerminalHistoryItem() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = CoreState(isInitialized = true, transfers = listOf(receivedTransfer(21UL, TransferStatus.Done)))
		}
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), UiMessageController())
		advanceUntilIdle()

		viewModel.requestDeleteHistoryItem(21UL)
		viewModel.confirmHistoryDelete()
		advanceUntilIdle()

		assertEquals(listOf(21UL), core.deletedTransfers)
		assertEquals(null, viewModel.state.value.historyDeleteTarget)
	}

	@Test
	fun receiveViewModelClearHistoryUsesAtomicCoreOperationAndKeepsActiveReceive() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			clearReceiveHistoryResult = Result.success(2UL)
			mutableState.value = CoreState(
				isInitialized = true,
				transfers = listOf(
					receivedTransfer(21UL, TransferStatus.Done),
					receivedTransfer(22UL, TransferStatus.Failed),
					receivedTransfer(23UL, TransferStatus.Receiving),
				),
			)
		}
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), UiMessageController())
		advanceUntilIdle()

		viewModel.requestClearHistory()
		assertEquals(ReceiveHistoryDeleteTarget.All, viewModel.state.value.historyDeleteTarget)
		viewModel.confirmHistoryDelete()
		advanceUntilIdle()

		assertEquals(1, core.clearReceiveHistoryCount)
		assertEquals(listOf(23UL), core.state.value.transfers.map(Transfer::transferId))
	}

	@Test
	fun receiveViewModelKeepsDeleteConfirmationAfterFailure() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			deleteResult = Result.failure(IllegalStateException("database busy"))
			mutableState.value = CoreState(isInitialized = true, transfers = listOf(receivedTransfer(21UL, TransferStatus.Done)))
		}
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), UiMessageController())
		advanceUntilIdle()

		viewModel.requestDeleteHistoryItem(21UL)
		viewModel.confirmHistoryDelete()
		advanceUntilIdle()

		assertEquals(ReceiveHistoryDeleteTarget.Transfer(21UL), viewModel.state.value.historyDeleteTarget)
		assertFalse(viewModel.state.value.isDeletingHistory)
	}

	@Test
	fun receiveViewModelCompletesSuccessfulReceiveAndResetsAcquisition() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			inspectionResult = Result.success(sampleTicketInspection())
		}
		val messages = UiMessageController()
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), messages)
		advanceUntilIdle()
		viewModel.onInvitationResult(com.vnidrop.app.feature.receive.ReceiveMethod.InvitationFile, Result.success("ticket-abc"))
		advanceUntilIdle()

		viewModel.receive()
		advanceUntilIdle()

		assertEquals(1, core.receiveCount)
		assertEquals("ticket-abc", core.lastReceiveTicket)
		assertEquals("Receiver", core.lastReceiveReceiverName)
		assertFalse(viewModel.state.value.isAcquisitionOpen)
		assertEquals("", viewModel.state.value.ticket)
		assertFalse(viewModel.state.value.isReceiving)
		val completed = messages.messages.first()
		assertEquals(null, completed.actionLabel)
		assertEquals(null, completed.onAction)
	}

	@Test
	fun receiveViewModelOffersCompletedFolderInFilesWhenPlatformSupportsIt() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			inspectionResult = Result.success(sampleTicketInspection())
		}
		val fileSystem = FakeFileSystemService(folder).apply { canRevealFolder = true }
		val messages = UiMessageController()
		val viewModel = ReceiveViewModel(core, fileSystem, preferences(), messages)
		advanceUntilIdle()
		viewModel.onInvitationResult(com.vnidrop.app.feature.receive.ReceiveMethod.InvitationFile, Result.success("ticket"))
		advanceUntilIdle()

		viewModel.receive()
		advanceUntilIdle()
		val completed = messages.messages.first()

		assertEquals(UiText.Resource(Res.string.button_show_in_files), completed.actionLabel)
		assertNotNull(completed.onAction).invoke()
		advanceUntilIdle()
		assertEquals(listOf(folder), fileSystem.revealedFolders)
	}

	@Test
	fun receiveViewModelUsesPlatformEffectiveFolderInsteadOfPersistedExternalFolder() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val internalFolder = ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/app/Documents", "Documents")
		val fileSystem = FakeFileSystemService(folder).apply { effectiveFolder = internalFolder }
		val viewModel = ReceiveViewModel(FakeCoreGateway(), fileSystem, preferences(), UiMessageController())

		advanceUntilIdle()

		assertEquals(internalFolder, viewModel.state.value.receiveFolder)
	}

	@Test
	fun receiveViewModelReportsWhenCompletedFolderCannotOpen() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			inspectionResult = Result.success(sampleTicketInspection())
		}
		val fileSystem = FakeFileSystemService(folder).apply {
			canRevealFolder = true
			revealFolderResult = Result.failure(IllegalStateException("Files unavailable"))
		}
		val messages = UiMessageController()
		val viewModel = ReceiveViewModel(core, fileSystem, preferences(), messages)
		advanceUntilIdle()
		viewModel.onInvitationResult(com.vnidrop.app.feature.receive.ReceiveMethod.InvitationFile, Result.success("ticket"))
		advanceUntilIdle()
		viewModel.receive()
		advanceUntilIdle()

		val completed = messages.messages.first()
		assertNotNull(completed.onAction).invoke()
		advanceUntilIdle()

		assertEquals(
			UiText.Resource(Res.string.receive_open_files_failed),
			messages.messages.first().text,
		)
	}

	@Test
	fun receiveViewModelKeepsReviewStateWhenReceiveFails() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			inspectionResult = Result.success(sampleTicketInspection())
			receiveResult = Result.failure(IllegalStateException("sender refused"))
		}
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), UiMessageController())
		advanceUntilIdle()
		viewModel.onInvitationResult(com.vnidrop.app.feature.receive.ReceiveMethod.QrCode, Result.success("ticket-xyz"))
		advanceUntilIdle()

		viewModel.receive()
		advanceUntilIdle()

		assertEquals(1, core.receiveCount)
		assertTrue(viewModel.state.value.isAcquisitionOpen)
		assertEquals("ticket-xyz", viewModel.state.value.ticket)
		assertFalse(viewModel.state.value.isReceiving)
		assertTrue(viewModel.state.value.inspection != null)
		assertEquals(
			UiText.Resource(Res.string.error_permission),
			viewModel.state.value.lastReceiveError,
		)
	}

	@Test
	fun receiveViewModelDoesNotOfferBlindRetryForDestinationCollision() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			inspectionResult = Result.success(sampleTicketInspection())
			receiveResult = Result.failure(VnidropException.DestinationExists("target exists"))
		}
		val messages = UiMessageController()
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), messages)
		advanceUntilIdle()
		viewModel.onInvitationResult(com.vnidrop.app.feature.receive.ReceiveMethod.QrCode, Result.success("ticket"))
		advanceUntilIdle()

		viewModel.receive()
		advanceUntilIdle()
		val message = messages.messages.first()

		assertEquals(UiText.Resource(Res.string.error_destination_exists), message.text)
		assertEquals(null, message.actionLabel)
		assertEquals(null, message.onAction)
	}

	@Test
	fun receiveViewModelCancelUsesActiveTransferId() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = CoreState(
				isInitialized = true,
				transfers = listOf(receivedTransfer(33UL, TransferStatus.Receiving)),
			)
		}
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), UiMessageController())
		advanceUntilIdle()
		viewModel.cancelActiveReceive()
		advanceUntilIdle()
		assertEquals(listOf(33UL), core.cancelledTransfers)
	}

	@Test
	fun receiveViewModelClearsTicketWhenInspectionFailsButKeepsAcquisitionOpen() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			inspectionResult = Result.failure(IllegalArgumentException("invalid ticket"))
		}
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), UiMessageController())
		advanceUntilIdle()

		viewModel.onInvitationResult(com.vnidrop.app.feature.receive.ReceiveMethod.InvitationFile, Result.success("bad-ticket"))
		advanceUntilIdle()

		assertTrue(viewModel.state.value.isAcquisitionOpen)
		assertEquals("", viewModel.state.value.ticket)
		assertEquals(null, viewModel.state.value.inspection)
		assertFalse(viewModel.state.value.isInspecting)
	}

	@Test
	fun receiveViewModelIgnoresDeleteForActiveReceive() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = CoreState(
				isInitialized = true,
				transfers = listOf(receivedTransfer(21UL, TransferStatus.Receiving)),
			)
		}
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), UiMessageController())
		advanceUntilIdle()

		viewModel.requestDeleteHistoryItem(21UL)
		assertEquals(null, viewModel.state.value.historyDeleteTarget)
	}

	@Test
	fun receiveViewModelDismissResetsIdleAcquisitionButNotWhileReceiving() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			inspectionResult = Result.success(sampleTicketInspection())
			// Keep receive suspended so dismiss can be asserted mid-transfer.
			receiveResult = Result.success(Unit)
			receiveSuspend = true
		}
		val viewModel = ReceiveViewModel(core, FakeFileSystemService(folder), preferences(), UiMessageController())
		advanceUntilIdle()

		viewModel.openAcquisition()
		viewModel.dismissAcquisition()
		assertFalse(viewModel.state.value.isAcquisitionOpen)

		viewModel.onInvitationResult(com.vnidrop.app.feature.receive.ReceiveMethod.InvitationFile, Result.success("ticket"))
		advanceUntilIdle()
		viewModel.receive()
		// Start receive but do not finish the suspended core call yet.
		testScheduler.runCurrent()
		assertTrue(viewModel.state.value.isReceiving)
		viewModel.dismissAcquisition()
		assertTrue(viewModel.state.value.isAcquisitionOpen)
		assertEquals("ticket", viewModel.state.value.ticket)

		core.completeSuspendedReceive()
		advanceUntilIdle()
		assertFalse(viewModel.state.value.isAcquisitionOpen)
	}

	private fun preferences() = FakePreferencesRepository(
		AppPreferences(
			username = "Receiver",
			receiveFolder = folder,
			themeMode = ThemeMode.System,
			notificationsEnabled = false,
			diagnosticsEnabled = false,
			diagnosticsInstallId = "test-install",
		),
	)

	private fun environment() = PlatformEnvironment("Test", "1.0", "/tmp/vnidrop")

	private fun settingsViewModel(
		preferences: PreferencesRepository = preferences(),
		notifications: FakeNotificationService = FakeNotificationService(),
		transport: DiagnosticsTransport = RecordingDiagnosticsTransport(),
		fileSystem: FakeFileSystemService = FakeFileSystemService(folder),
		diagnosticsIncluded: Boolean = false,
		repository: FakeCoreGateway = FakeCoreGateway(),
	) = SettingsViewModel(
		environment(),
		{ DeviceInfo("Device", "Model", "OS", "Wi-Fi", "80%") },
		fileSystem,
		repository,
		preferences,
		notifications,
		UiMessageController(),
		BugReportService(
			preferencesRepository = preferences,
			transport = transport,
			breadcrumbs = BreadcrumbBuffer(),
			appVersion = "1.0",
			platform = "Test",
			logReader = { "sample log line" },
		),
		diagnosticsIncluded = diagnosticsIncluded,
	)

	private fun receivedTransfer(id: ULong, status: TransferStatus) = Transfer(
		localId = "receive-$id",
		transferId = id,
		direction = TransferDirection.Receive,
		status = status,
		peerId = null,
		transferName = "Received $id",
		contentHash = "hash-$id",
		fileCount = 1UL,
		totalSize = 42UL,
		ticket = null,
		accessPolicy = ShareAccessPolicy.RequireApproval,
		createdAt = 1L,
		updatedAt = 1L,
	)

	private fun sentTransfer(id: ULong) = Transfer(
		localId = "send-$id",
		transferId = id,
		direction = TransferDirection.Send,
		status = TransferStatus.Sharing,
		peerId = null,
		transferName = "Sent $id",
		contentHash = "hash-$id",
		fileCount = 1UL,
		totalSize = 42UL,
		ticket = "ticket-$id",
		accessPolicy = ShareAccessPolicy.RequireApproval,
		createdAt = 1L,
		updatedAt = 1L,
	)

	private fun sampleTicketInspection() = com.vnidrop.app.core.TicketInspectionModel(
		kind = "vnidrop",
		metadata = com.vnidrop.app.core.TransferMetadataModel(1UL, "Photo", null, "hash", 1UL, 42UL),
	)

	private companion object {
		val folder = ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "Downloads")
	}
}
