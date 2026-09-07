package com.vnidrop.app.feature.receive

import kotlin.test.Test
import kotlin.test.assertEquals

class QrScanSessionTest {
	@Test
	fun repeatedFramesDeliverOnlyOnce() {
		val results = mutableListOf<String>()
		val session = QrScanSession { results += it.getOrThrow() }
		session.complete(Result.success("first"))
		session.complete(Result.success("second"))
		assertEquals(listOf("first"), results)
	}

	@Test
	fun cancelledSessionCannotDeliverIntoItsReplacement() {
		val results = mutableListOf<String>()
		val cancelled = QrScanSession { results += it.getOrThrow() }
		cancelled.close()
		val replacement = QrScanSession { results += it.getOrThrow() }
		cancelled.complete(Result.success("queued before cancellation"))
		replacement.complete(Result.success("replacement"))
		assertEquals(listOf("replacement"), results)
	}
}
