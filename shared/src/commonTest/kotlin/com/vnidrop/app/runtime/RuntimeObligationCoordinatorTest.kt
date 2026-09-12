package com.vnidrop.app.runtime

import com.vnidrop.app.BackgroundRuntimeKeeper
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.core.RuntimeObligationFactsModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.notifications.NotificationPermission
import com.vnidrop.app.preferences.AppPreferences
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.support.FakeNotificationService
import com.vnidrop.app.support.FakePreferencesRepository
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class RuntimeObligationCoordinatorTest {
	@Test
	fun applicationLifetimeCoordinatorTracksCoreFactsWithoutAComposableCollector() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			runtimeObligationFactsResult = Result.success(facts(activeInvitationTransfers = 1UL))
		}
		val keeper = RecordingRuntimeKeeper()
		val coordinator = coordinator(core, keeper)
		runCurrent()

		assertEquals(true, keeper.requiredCalls.last())
		core.runtimeObligationFactsResult = Result.success(facts())
		core.mutableSignals.emit(CoreSignal.RuntimeObligationChanged)
		runCurrent()

		assertEquals(false, keeper.requiredCalls.last())
		assertEquals(2, core.runtimeObligationFactsCount)
		coordinator.close()
		coordinator.close()
		assertEquals(false, keeper.requiredCalls.last())
		assertEquals(1, keeper.closeCount)
	}

	@Test
	fun targetedAndInvitationSignalsRefreshNeutralFacts() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			runtimeObligationFactsResult = Result.success(facts(targetedProviderAvailability = 1UL))
		}
		val keeper = RecordingRuntimeKeeper()
		val coordinator = coordinator(core, keeper)
		runCurrent()
		assertEquals(true, keeper.requiredCalls.last())

		core.runtimeObligationFactsResult = Result.success(facts())
		core.mutableSignals.emit(CoreSignal.TargetedTransferChanged)
		runCurrent()
		assertEquals(false, keeper.requiredCalls.last())

		core.runtimeObligationFactsResult = Result.success(facts(invitationProviderAvailability = 1UL))
		core.mutableSignals.emit(CoreSignal.TransfersChanged(7UL))
		runCurrent()
		assertEquals(true, keeper.requiredCalls.last())

		coordinator.close()
		assertEquals(1, keeper.closeCount)
	}

	@Test
	fun desktopDoesNotStartTheAndroidObligationMapping() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			runtimeObligationFactsResult = Result.success(facts(targetedPreparations = 1UL))
		}
		val keeper = RecordingRuntimeKeeper()
		val coordinator = coordinator(core, keeper, UiPlatform.Linux)
		runCurrent()

		assertEquals(emptyList(), keeper.requiredCalls)
		coordinator.close()
		assertEquals(1, keeper.closeCount)
	}

	@Test
	fun savedDeviceWithNotificationsKeepsTheRuntimeWithoutActiveWork() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			savedDevices = listOf(savedDevice())
		}
		val keeper = RecordingRuntimeKeeper()
		val preferences = FakePreferencesRepository(preferences(notificationsEnabled = true))
		val coordinator = coordinator(core, keeper, preferences = preferences)
		runCurrent()

		assertEquals(true, keeper.requiredCalls.last())
		assertEquals(1, core.listSavedDevicesCount)
		coordinator.close()
	}

	@Test
	fun enablingNotificationsStartsSavedDeviceListening() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			savedDevices = listOf(savedDevice())
		}
		val keeper = RecordingRuntimeKeeper()
		val preferences = FakePreferencesRepository(preferences(notificationsEnabled = false))
		coordinator(core, keeper, preferences = preferences)
		runCurrent()
		assertEquals(false, keeper.requiredCalls.last())

		preferences.mutablePreferences.value = preferences(notificationsEnabled = true)
		runCurrent()
		assertEquals(true, keeper.requiredCalls.last())
	}

	@Test
	fun savedDeviceWithoutNotificationsDoesNotKeepTheRuntime() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			savedDevices = listOf(savedDevice())
		}
		val keeper = RecordingRuntimeKeeper()
		coordinator(core, keeper, preferences = FakePreferencesRepository(preferences(notificationsEnabled = false)))
		runCurrent()

		assertEquals(false, keeper.requiredCalls.last())
	}

	@Test
	fun pairingChangeStartsAndStopsSavedDeviceListening() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
		}
		val keeper = RecordingRuntimeKeeper()
		val preferences = FakePreferencesRepository(preferences(notificationsEnabled = true))
		coordinator(core, keeper, preferences = preferences)
		runCurrent()
		assertEquals(false, keeper.requiredCalls.last())

		core.savedDevices = listOf(savedDevice())
		core.mutableSignals.emit(CoreSignal.PairingChanged)
		runCurrent()
		assertEquals(true, keeper.requiredCalls.last())

		core.savedDevices = emptyList()
		core.mutableSignals.emit(CoreSignal.PairingChanged)
		runCurrent()
		assertEquals(false, keeper.requiredCalls.last())
	}

	@Test
	fun deniedNotificationPermissionDoesNotKeepTheRuntimeForSavedDevices() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			savedDevices = listOf(savedDevice())
		}
		val keeper = RecordingRuntimeKeeper()
		val notifications = FakeNotificationService(NotificationPermission.Denied)
		coordinator(
			core,
			keeper,
			preferences = FakePreferencesRepository(preferences(notificationsEnabled = true)),
			notifications = notifications,
		)
		runCurrent()

		assertEquals(false, keeper.requiredCalls.last())
	}

	@Test
	fun activeTransferKeepsTheRuntimeEvenWithoutSavedDevices() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			runtimeObligationFactsResult = Result.success(facts(activeTargetedTransfers = 1UL))
		}
		val keeper = RecordingRuntimeKeeper()
		coordinator(
			core,
			keeper,
			preferences = FakePreferencesRepository(preferences(notificationsEnabled = false)),
		)
		runCurrent()

		assertEquals(true, keeper.requiredCalls.last())
	}

	@Test
	fun listenPolicyRequiresSavedDeviceNotificationsAndPermission() {
		assertFalse(requiresBackgroundRuntime(facts(), 0, true, NotificationPermission.Granted))
		assertFalse(requiresBackgroundRuntime(facts(), 1, false, NotificationPermission.Granted))
		assertFalse(requiresBackgroundRuntime(facts(), 1, true, NotificationPermission.Denied))
		assertTrue(requiresBackgroundRuntime(facts(), 1, true, NotificationPermission.Granted))
		assertTrue(
			requiresBackgroundRuntime(
				facts(activeInvitationTransfers = 1UL),
				0,
				false,
				NotificationPermission.Denied,
			),
		)
	}

	private fun TestScope.coordinator(
		core: FakeCoreGateway,
		keeper: RecordingRuntimeKeeper,
		platform: UiPlatform = UiPlatform.Android,
		preferences: FakePreferencesRepository = FakePreferencesRepository(preferences()),
		notifications: FakeNotificationService = FakeNotificationService(),
	) = RuntimeObligationCoordinator(
		repository = core,
		keeper = keeper,
		platform = platform,
		applicationScope = backgroundScope,
		preferencesRepository = preferences,
		notifications = notifications,
	)

	private class RecordingRuntimeKeeper : BackgroundRuntimeKeeper {
		val requiredCalls = mutableListOf<Boolean>()
		var closeCount = 0

		override fun setRequired(required: Boolean) {
			requiredCalls += required
		}

		override fun close() {
			closeCount += 1
			requiredCalls += false
		}
	}

	private fun facts(
		activeInvitationTransfers: ULong = 0UL,
		invitationProviderAvailability: ULong = 0UL,
		targetedPreparations: ULong = 0UL,
		activeTargetedTransfers: ULong = 0UL,
		targetedProviderAvailability: ULong = 0UL,
	) = RuntimeObligationFactsModel(
		activeInvitationTransfers,
		invitationProviderAvailability,
		targetedPreparations,
		activeTargetedTransfers,
		targetedProviderAvailability,
	)

	private fun preferences(notificationsEnabled: Boolean = false) = AppPreferences(
		username = "User",
		receiveFolder = ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp"),
		themeMode = ThemeMode.System,
		notificationsEnabled = notificationsEnabled,
	)

	private fun savedDevice() = SavedDeviceModel(
		endpointId = "peer",
		localLabel = "Office PC",
		remoteDisplayName = "Linux",
		createdAt = 1L,
		lastAuthenticatedAt = 2L,
	)
}
