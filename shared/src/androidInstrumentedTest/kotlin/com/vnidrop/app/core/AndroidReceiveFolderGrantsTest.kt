package com.vnidrop.app.core

import android.content.Intent
import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AndroidReceiveFolderGrantsTest {
	private val read = Intent.FLAG_GRANT_READ_URI_PERMISSION
	private val readWrite = read or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
	private val old = Uri.parse("content://test/tree/old")
	private val next = Uri.parse("content://test/tree/next")
	private val legacyShare = Uri.parse("content://test/tree/old-share")

	@Test
	fun changeKeepsTheOldGrantUntilItsActiveReceiveFinishes() = runBlocking {
		val permissions = mutableMapOf(old to readWrite, legacyShare to read)
		val grants = grants(permissions)
		grants.reconcile(old)
		assertEquals(mapOf(old to readWrite), permissions)

		val entered = CompletableDeferred<Unit>()
		val release = CompletableDeferred<Unit>()
		val download = async {
			grants.withAccess(old) {
				entered.complete(Unit)
				release.await()
			}
		}
		entered.await()
		try {
			grants.update(next) {}
			assertEquals(mapOf(old to readWrite, next to readWrite), permissions)
		} finally {
			release.complete(Unit)
		}
		download.await()
		assertEquals(mapOf(next to readWrite), permissions)
		grants.update(null) {}
		assertEquals(emptyMap(), permissions)
	}

	@Test
	fun failedPreferenceWriteRestoresThePreviousGrantSet() = runBlocking {
		val permissions = mutableMapOf(old to readWrite, next to read)
		val grants = grants(permissions)
		assertFailsWith<IllegalStateException> {
			grants.update(next) { error("Could not save preferences") }
		}
		assertEquals(mapOf(old to readWrite, next to read), permissions)
	}

	private fun grants(permissions: MutableMap<Uri, Int>) = AndroidReceiveFolderGrants(
		persisted = { permissions.toMap() },
		take = { uri, modes -> permissions[uri] = (permissions[uri] ?: 0) or modes },
		release = { uri, modes ->
			val remaining = (permissions[uri] ?: 0) and modes.inv()
			if (remaining == 0) permissions.remove(uri) else permissions[uri] = remaining
		},
	)
}
