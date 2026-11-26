package com.vnidrop.app.core

import android.net.Uri
import android.os.ParcelFileDescriptor
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.SdkSuppress
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertNull
import org.junit.runner.RunWith
import uniffi.vnidrop.ReceivedLocatorKind

@RunWith(AndroidJUnit4::class)
@SdkSuppress(minSdkVersion = 29)
class AndroidReceiveDescriptorTest {
	@Test
	fun restartedJournalReclaimsOnlyRecordedStagesFromAnEarlierProcess() {
		val context = InstrumentationRegistry.getInstrumentation().targetContext
		val prefs = context.getSharedPreferences(UUID.randomUUID().toString(), 0)
		val old = Uri.parse("content://test/document/old")
		val active = Uri.parse("content://test/document/active")
		AndroidStageJournal(prefs, "before").remember(old, "one.part")
		val journal = AndroidStageJournal(prefs, "after")
		journal.remember(active, "two.part")
		val removed = mutableListOf<Pair<Uri, String>>()
		repeat(2) { journal.recover { uri, name -> removed.add(uri to name) } }
		assertEquals(listOf(old to "one.part"), removed)
		prefs.edit().clear().commit()
	}

	@Test
	fun unreadableNestedFolderRejectsWholeSelection() {
		val folder = PickedShareFile(value = "content://example/tree/root", displayName = "folder", isDirectory = true)
		var queries = 0
		kotlin.test.assertFailsWith<IllegalStateException> {
			expandShareDirectory(folder) { _, projection ->
				if (++queries == 1) android.database.MatrixCursor(projection).apply {
					addRow(arrayOf<Any?>("a", "a.txt", "text/plain", 1))
					addRow(arrayOf<Any?>("b", "folder", android.provider.DocumentsContract.Document.MIME_TYPE_DIR, null))
				} else null
			}
		}
		assertEquals(2, queries)
	}

	@Test
	fun mediaStorePublishesBytesWrittenToItsDescriptor() {
		val context = InstrumentationRegistry.getInstrumentation().targetContext
		val sink = AndroidMediaStoreDownloadsSink(context)
		val name = "vnidrop-descriptor-${UUID.randomUUID()}.bin"
		val expected = ByteArray(4096) { (it % 251).toByte() }
		sink.startFile(name)
		val uri = Uri.parse(sink.pendingOutputLocator(name))
		try {
			assertNull(sink.openPublishedOutput(name, uri.toString(), published = false))
			ParcelFileDescriptor.AutoCloseOutputStream(
				ParcelFileDescriptor.fromFd(sink.writableFileDescriptor(name)),
			).use { it.write(expected) }

			val published = sink.finishFile(name)
			assertEquals(ReceivedLocatorKind.ANDROID_MEDIA_STORE, published.locatorKind)
			assertEquals(uri.toString(), published.locator)
			val input = context.contentResolver.openInputStream(uri)
			assertContentEquals(expected, checkNotNull(input).use { it.readBytes() })
		} finally {
			sink.abortFile(name, "test cleanup")
			context.contentResolver.delete(uri, null, null)
		}
	}
}
