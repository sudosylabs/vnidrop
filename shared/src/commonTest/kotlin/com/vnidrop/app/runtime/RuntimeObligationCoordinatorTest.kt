package com.vnidrop.app.runtime

import com.vnidrop.app.BackgroundRuntimeKeeper
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.RuntimeObligationFactsModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.support.FakeCoreGateway
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
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
	fun savedDeviceWithListenIntentKeepsTheRuntimeWithoutActiveWork() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			savedDevices = listOf(savedDevice())
		}
		val keeper = RecordingRuntimeKeeper()
		val coordinator = coordinator(core, keeper, listenIntent = listenIntent(optedIn = true))
		runCurrent()

		assertEquals(true, keeper.requiredCalls.last())
		assertEquals(1, core.listSavedDevicesCount)
		coordinator.close()
	}

	@Test
	fun enablingListenIntentStartsSavedDeviceListening() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			savedDevices = listOf(savedDevice())
		}
		val keeper = RecordingRuntimeKeeper()
		val intent = listenIntent(optedIn = false)
		coordinator(core, keeper, listenIntent = intent)
		runCurrent()
		assertEquals(false, keeper.requiredCalls.last())

		intent.value = SavedDeviceListenIntent(optedIn = true, permissionGranted = true)
		runCurrent()
		assertEquals(true, keeper.requiredCalls.last())
	}

	@Test
	fun savedDeviceWithoutListenIntentDoesNotKeepTheRuntime() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			savedDevices = listOf(savedDevice())
		}
		val keeper = RecordingRuntimeKeeper()
		coordinator(core, keeper, listenIntent = listenIntent(optedIn = false))
		runCurrent()

		assertEquals(false, keeper.requiredCalls.last())
	}

	@Test
	fun pairingChangeStartsAndStopsSavedDeviceListening() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
		}
		val keeper = RecordingRuntimeKeeper()
		coordinator(core, keeper, listenIntent = listenIntent(optedIn = true))
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
		coordinator(
			core,
			keeper,
			listenIntent = listenIntent(optedIn = true, permissionGranted = false),
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
		coordinator(core, keeper, listenIntent = listenIntent(optedIn = false, permissionGranted = false))
		runCurrent()

		assertEquals(true, keeper.requiredCalls.last())
	}

	@Test
	fun listenPolicyUnionsCoreObligationAndSavedDeviceListen() {
		assertFalse(requiresBackgroundRuntime(coreObligation = false, savedDeviceListen = false))
		assertTrue(requiresBackgroundRuntime(coreObligation = true, savedDeviceListen = false))
		assertTrue(requiresBackgroundRuntime(coreObligation = false, savedDeviceListen = true))
		assertTrue(requiresBackgroundRuntime(coreObligation = true, savedDeviceListen = true))
	}

	@Test
	fun listenActiveRequiresSavedDevicesAndIntent() {
		val granted = SavedDeviceListenIntent(optedIn = true, permissionGranted = true)
		assertFalse(savedDeviceListenActive(0, granted))
		assertFalse(savedDeviceListenActive(1, SavedDeviceListenIntent(optedIn = false, permissionGranted = true)))
		assertFalse(savedDeviceListenActive(1, SavedDeviceListenIntent(optedIn = true, permissionGranted = false)))
		assertTrue(savedDeviceListenActive(1, granted))
	}

	private fun TestScope.coordinator(
		core: FakeCoreGateway,
		keeper: RecordingRuntimeKeeper,
		platform: UiPlatform = UiPlatform.Android,
		listenIntent: MutableStateFlow<SavedDeviceListenIntent> = listenIntent(),
	) = RuntimeObligationCoordinator(
		repository = core,
		keeper = keeper,
		platform = platform,
		applicationScope = backgroundScope,
		listenIntent = listenIntent,
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

	private fun listenIntent(
		optedIn: Boolean = false,
		permissionGranted: Boolean = true,
	) = MutableStateFlow(SavedDeviceListenIntent(optedIn, permissionGranted))

	private fun savedDevice() = SavedDeviceModel(
		endpointId = "peer",
		localLabel = "Office PC",
		remoteDisplayName = "Linux",
		createdAt = 1L,
		lastAuthenticatedAt = 2L,
	)
}
