package com.vnidrop.app

import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertTrue
import uniffi.vnidrop.CoreEvent
import uniffi.vnidrop.CoreEventSink
import uniffi.vnidrop.VnidropCore
import uniffi.vnidrop.VnidropException

class CoreNativeLoadTest {
	@Test
	fun generatedBindingsCanInitializeRustCore() {
		val coreDir = Files.createTempDirectory("vnidrop-jvm-test")
		val core = try {
			VnidropCore.initialize(
				appDataDir = coreDir.toString(),
				eventSink = object : CoreEventSink {
					override fun onEvent(event: CoreEvent) = Unit
				},
			)
		} catch (_: VnidropException.SecureStorageUnavailable) {
			coreDir.toFile().deleteRecursively()
			return
		} catch (_: VnidropException.SecureStorageLocked) {
			coreDir.toFile().deleteRecursively()
			return
		}

		try {
			assertTrue(core.status().endpointId.isNotBlank())
		} finally {
			core.shutdown()
			coreDir.toFile().deleteRecursively()
		}
	}
}
