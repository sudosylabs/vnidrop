package com.vnidrop.app.feature.receive

import java.io.ByteArrayInputStream
import java.io.InputStream
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class InvitationStreamTest {
	@Test
	fun shortReadsAreJoinedIntoTheWholeInvitation() {
		val invitation = ByteArray(20_000) { (it % 251).toByte() }
		assertContentEquals(invitation, TrickleStream(invitation).readInvitationBytes())
	}

	@Test
	fun invitationAtTheLimitIsAccepted() {
		val invitation = ByteArray(MaxVniDropInvitationBytes) { 'a'.code.toByte() }
		val bytes = ByteArrayInputStream(invitation).readInvitationBytes()
		assertEquals(MaxVniDropInvitationBytes, bytes.size)
		assertEquals(MaxVniDropInvitationBytes, decodeInvitationBytes(bytes).length)
	}

	@Test
	fun oversizeInvitationStopsOneBytePastTheLimitAndIsRejected() {
		val stream = ByteArrayInputStream(ByteArray(MaxVniDropInvitationBytes * 4) { 'a'.code.toByte() })
		val bytes = stream.readInvitationBytes()
		assertEquals(MaxVniDropInvitationBytes + 1, bytes.size)
		assertFailsWith<IllegalArgumentException> { decodeInvitationBytes(bytes) }
	}

	private class TrickleStream(private val bytes: ByteArray) : InputStream() {
		private var position = 0

		override fun read(): Int = if (position < bytes.size) bytes[position++].toInt() and 0xFF else -1

		override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
			if (position >= bytes.size) return -1
			val count = minOf(length, 3, bytes.size - position)
			bytes.copyInto(buffer, offset, position, position + count)
			position += count
			return count
		}
	}
}
