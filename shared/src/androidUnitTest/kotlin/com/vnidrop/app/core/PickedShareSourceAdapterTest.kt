package com.vnidrop.app.core

import java.io.Closeable
import java.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.coroutines.test.runTest

class PickedShareSourceAdapterTest {
	@Test
	fun partialOpenClosesEveryAcquiredDescriptorEvenIfOneCloseFails() = runTest {
		val closed = mutableListOf<Int>()
		val failure = assertFailsWith<IOException> {
			withOpenedShareDescriptors(listOf(1, 2, 3), open = { source ->
				if (source == 3) throw IOException("open failed")
				Closeable {
					closed += source
					if (source == 2) throw IOException("close failed")
				}
			}) { error("the operation cannot run after a failed open") }
		}
		assertEquals("open failed", failure.message)
		assertEquals(listOf("close failed"), failure.suppressed.map { it.message })
		assertEquals(listOf(2, 1), closed)
	}
}
