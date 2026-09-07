package com.vnidrop.app.feature.receive

import java.util.concurrent.atomic.AtomicBoolean

internal class QrScanSession(private val onResult: (Result<String>) -> Unit) {
	private val closed = AtomicBoolean(false)

	fun complete(result: Result<String>) {
		if (closed.compareAndSet(false, true)) onResult(result)
	}

	fun close() { closed.set(true) }
}
