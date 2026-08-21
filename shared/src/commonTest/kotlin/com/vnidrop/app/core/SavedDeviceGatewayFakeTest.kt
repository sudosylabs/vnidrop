package com.vnidrop.app.core

import com.vnidrop.app.support.FakeCoreGateway
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.coroutines.runBlocking
import uniffi.vnidrop.ShareSource
import uniffi.vnidrop.SourceKind

class SavedDeviceGatewayFakeTest {
	@Test
	fun fakeGatewayExposesSavedDeviceAndTargetedSeams() = runBlocking {
		val gateway = FakeCoreGateway()
		gateway.savedDevices = listOf(
			SavedDeviceModel(
				endpointId = "peer-1",
				localLabel = "Kitchen",
				remoteDisplayName = null,
				createdAt = 1L,
				lastAuthenticatedAt = null,
			),
		)
		gateway.respondTargetedResult = Result.success(
			TargetedOfferResponseModel.Approved("transfer-1"),
		)

		assertEquals("Kitchen", gateway.listSavedDevices().getOrThrow().single().localLabel)
		assertEquals(
			TargetedOfferResponseModel.Approved("transfer-1"),
			gateway.respondToTargetedOffer("transfer-1", accepted = true).getOrThrow(),
		)
		assertTrue(
			gateway.newTargetedTransferPreparation("peer-1").getOrThrow().send(
				sources = listOf(
					ShareSource(SourceKind.PATH, "/tmp/a.txt", "a.txt", false),
				),
				transferName = "a.txt",
			).isFailure,
		)
	}
}
