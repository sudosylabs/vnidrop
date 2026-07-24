package com.vnidrop.app.core

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class CoreLifecycleGateTest {
	@Test
	fun concurrentCallsHoldIndependentLeasesAndBlockReconfiguration() = runTest {
		val gate = CoreLifecycleGate()
		val firstEntered = CompletableDeferred<Unit>()
		val secondEntered = CompletableDeferred<Unit>()
		val release = CompletableDeferred<Unit>()
		val first = launch {
			gate.withCall(capture = { "core" }) { captured ->
				assertEquals("core", captured)
				firstEntered.complete(Unit)
				release.await()
			}
		}
		firstEntered.await()
		val second = launch {
			gate.withCall(capture = { "core" }) {
				secondEntered.complete(Unit)
				release.await()
			}
		}
		secondEntered.await()

		assertFailsWith<CoreLifecycleBusyException> {
			gate.withReconfiguration { error("must not run") }
		}

		release.complete(Unit)
		first.join()
		second.join()
		gate.withReconfiguration { }
	}

	@Test
	fun callsCannotStartDuringReconfiguration() = runTest {
		val gate = CoreLifecycleGate()
		val entered = CompletableDeferred<Unit>()
		val release = CompletableDeferred<Unit>()
		val reconfiguration = launch {
			gate.withReconfiguration {
				entered.complete(Unit)
				release.await()
			}
		}
		entered.await()

		assertFailsWith<CoreLifecycleBusyException> {
			gate.withCall(capture = { "core" }) { error("must not run") }
		}

		release.complete(Unit)
		reconfiguration.join()
	}

	@Test
	fun cancellingCallReleasesItsLease() = runTest {
		val gate = CoreLifecycleGate()
		val entered = CompletableDeferred<Unit>()
		val call = launch {
			gate.withCall(capture = { "core" }) {
				entered.complete(Unit)
				awaitCancellation()
			}
		}
		entered.await()

		call.cancel()
		call.join()

		gate.withReconfiguration { }
	}
}
