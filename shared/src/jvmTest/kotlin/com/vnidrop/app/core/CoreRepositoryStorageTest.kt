package com.vnidrop.app.core

import java.nio.file.Files
import kotlin.io.path.createTempDirectory
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class CoreRepositoryStorageTest {
	@Test
	fun cacheClearWaitsForActiveSharesThenRestartsWithTheSameIdentity() = runTest {
		val appData = createTempDirectory("vnidrop-cache-clear")
		val source = Files.write(appData.resolve("source.bin"), ByteArray(64 * 1024) { 5 })
		val repository = CoreRepository()
		try {
			assertTrue(
				repository.initialize(
					appData.toString(),
					RelaySettings(mode = RelayMode.LocalOnly),
				).isSuccess,
			)
			val endpointId = repository.state.value.status?.endpointId
			val share = repository.sharePath(
				path = source.toString(),
				transferName = "source.bin",
				senderName = "Sender",
				accessPolicy = ShareAccessPolicy.RequireApproval,
			).getOrThrow()

			assertTrue(repository.clearTransferCache().isFailure)
			assertTrue(repository.state.value.isInitialized)

			repository.delete(share.transferId).getOrThrow()
			assertTrue(repository.clearTransferCache().getOrThrow() >= 64UL * 1024UL)
			assertTrue(repository.state.value.isInitialized)
			assertEquals(endpointId, repository.state.value.status?.endpointId)
		} finally {
			repository.shutdown()
			appData.toFile().deleteRecursively()
		}
	}
}
