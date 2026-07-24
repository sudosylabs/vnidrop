package com.vnidrop.app.core

import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

internal class CoreLifecycleBusyException(message: String) : IllegalStateException(message)

internal class CoreLifecycleGate {
	private val mutex = Mutex()
	private var reconfiguring = false
	private var inFlightCalls = 0

	suspend fun <C, T> withCall(
		capture: () -> C,
		block: suspend (C) -> T,
	): T {
		val captured = mutex.withLock {
			if (reconfiguring) throw CoreLifecycleBusyException("Core network configuration is changing")
			val value = capture()
			inFlightCalls += 1
			value
		}
		return try {
			block(captured)
		} finally {
			withContext(NonCancellable) {
				mutex.withLock {
					check(inFlightCalls > 0)
					inFlightCalls -= 1
				}
			}
		}
	}

	suspend fun <T> withReconfiguration(block: suspend () -> T): T {
		mutex.withLock {
			if (reconfiguring) throw CoreLifecycleBusyException("Core network configuration is already changing")
			if (inFlightCalls > 0) throw CoreLifecycleBusyException("Core calls are still active")
			reconfiguring = true
		}
		return try {
			block()
		} finally {
			withContext(NonCancellable) {
				mutex.withLock { reconfiguring = false }
			}
		}
	}
}
