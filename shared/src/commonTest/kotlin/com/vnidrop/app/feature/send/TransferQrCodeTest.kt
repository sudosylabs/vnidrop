package com.vnidrop.app.feature.send

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class TransferQrCodeTest {
	@Test
	fun ticketPastVersionTwentyCapacityUsesNextAvailableVersion() {
		assertEquals(20, transferQrInformationDensity("a".repeat(858)))
		assertEquals(21, transferQrInformationDensity("a".repeat(859)))
		assertEquals(21, transferQrInformationDensity("a".repeat(876)))
	}

	@Test
	fun renderedQrUsesExpectedVersionSizeAndQuietZone() {
		val qrCode = buildTransferQrCode("a".repeat(876))

		assertEquals(21, qrCode.informationDensity)
		assertEquals(101, qrCode.rawData.size)
		assertEquals(872, qrCode.canvasSize)
	}

	@Test
	fun versionFortyCapacityBoundaryRejectsOversizedTicket() {
		assertEquals(40, transferQrInformationDensity("a".repeat(2_953)))
		assertFailsWith<IllegalArgumentException> {
			transferQrInformationDensity("a".repeat(2_954))
		}
	}
}
