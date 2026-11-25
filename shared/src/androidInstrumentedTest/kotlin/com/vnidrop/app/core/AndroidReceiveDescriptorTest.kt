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
