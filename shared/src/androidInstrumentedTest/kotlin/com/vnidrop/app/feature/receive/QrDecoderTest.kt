package com.vnidrop.app.feature.receive

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Matrix
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.vnidrop.app.feature.send.buildTransferQrCode
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import org.junit.runner.RunWith
import zxingcpp.BarcodeReader

@RunWith(AndroidJUnit4::class)
class QrDecoderTest {
	private val reader = BarcodeReader(BarcodeReader.Options(
		formats = setOf(BarcodeReader.Format.QR_CODE), tryRotate = true, tryInvert = true, tryHarder = true,
	))

	@Test
	fun bundledDecoderReadsDenseInvitationAndRotatedCode() {
		val invitation = "vnidrop-test:" + "abcdefgh01234567".repeat(110)
		val bytes = buildTransferQrCode(invitation).renderToBytes()
		val transparent = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
		val bitmap = Bitmap.createBitmap(transparent.width, transparent.height, Bitmap.Config.ARGB_8888)
		// Match the white QR surface shown by TransferSharePanel; camera frames have no alpha.
		bitmap.eraseColor(Color.WHITE)
		Canvas(bitmap).drawBitmap(transparent, 0f, 0f, null)
		transparent.recycle()
		try {
			assertEquals(invitation, reader.read(bitmap).single().text)
			val rotated = Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, Matrix().apply { postRotate(90f) }, false)
			try { assertEquals(invitation, reader.read(rotated).single().text) }
			finally { rotated.recycle() }
		} finally { bitmap.recycle() }
	}

	@Test
	fun emptyCameraImageDoesNotProduceAnInvitation() {
		val bitmap = Bitmap.createBitmap(640, 480, Bitmap.Config.ARGB_8888)
		try {
			bitmap.eraseColor(Color.GRAY)
			assertTrue(reader.read(bitmap).isEmpty())
		} finally { bitmap.recycle() }
	}
}
