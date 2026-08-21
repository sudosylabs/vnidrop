package com.vnidrop.app.runtime

import com.vnidrop.app.BackgroundRuntimeKeeper
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.RuntimeObligationFactsModel
import com.vnidrop.app.support.FakeCoreGateway
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalCoroutinesApi::class)
class RuntimeObligationCoordinatorTest {
	@Test
	fun applicationLifetimeCoordinatorTracksCoreFactsWithoutAComposableCollector() = runTest {
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			runtimeObligationFactsResult = Result.success(facts(activeInvitationTransfers = 1UL))
		}
		val keeper = RecordingRuntimeKeeper()
		val coordinator = RuntimeObligationCoordinator(core, keeper, UiPlatform.Android, backgroundScope)
		runCurrent()

		assertEquals(true, keeper.requiredCalls.last())
		core.runtimeObligationFactsResult = Result.success(facts())
		core.mutableSignals.emit(CoreSignal.RuntimeObligationChanged)
		runCurrent()

		assertEquals(listOf(false, true, false), keeper.requiredCalls)
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
		val coordinator = RuntimeObligationCoordinator(core, keeper, UiPlatform.Android, backgroundScope)
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
		val coordinator = RuntimeObligationCoordinator(core, keeper, UiPlatform.Linux, backgroundScope)
		runCurrent()

		assertEquals(emptyList(), keeper.requiredCalls)
		coordinator.close()
		assertEquals(1, keeper.closeCount)
	}

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
}
