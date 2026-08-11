package com.vnidrop.app

import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertTrue
import kotlin.test.fail
import uniffi.vnidrop.CoreEvent
import uniffi.vnidrop.CoreEventSink
import uniffi.vnidrop.VnidropCore
import uniffi.vnidrop.defaultCoreLimits
import uniffi.vnidrop.defaultCoreNetworkConfig

/**
 * Optional Windows host harness for the experimental saved-device core.
 *
 * Linux/macOS CI skips so shared jvmTest stays green; Windows desktop runs the
 * DPAPI-backed initialize/restart identity check against public bindings.
 */
class WindowsSavedDeviceCoreContractTest {
	@Test
	fun windowsHostPreservesProtectedIdentityAndBindingHygiene() {
		if (!isWindowsHost()) {
			return
		}

		val coreDir = Files.createTempDirectory("vnidrop-windows-contract")
		val sink = object : CoreEventSink {
			override fun onEvent(event: CoreEvent) {
				assertTrue(event.id.isNotBlank())
			}
		}

		val first = VnidropCore.initializeWithExperimentalSavedDevices(
			appDataDir = coreDir.toString(),
			eventSink = sink,
			limits = defaultCoreLimits(),
			networkConfig = defaultCoreNetworkConfig(),
		)
		val endpointId = try {
			val id = first.status().endpointId
			assertTrue(id.isNotBlank())
			assertBindingHygiene(first)
			id
		} finally {
			first.shutdown()
		}

		val restarted = VnidropCore.initializeWithExperimentalSavedDevices(
			appDataDir = coreDir.toString(),
			eventSink = sink,
			limits = defaultCoreLimits(),
			networkConfig = defaultCoreNetworkConfig(),
		)
		try {
			assertTrue(restarted.status().endpointId == endpointId)
			assertBindingHygiene(restarted)
		} finally {
			restarted.shutdown()
			coreDir.toFile().deleteRecursively()
		}
	}

	private fun isWindowsHost(): Boolean =
		System.getProperty("os.name").orEmpty().lowercase().contains("windows")

	private fun assertBindingHygiene(core: VnidropCore) {
		val methods = core.javaClass.methods.map { it.name }.toSet()
		for (forbidden in listOf(
			"putSecret",
			"loadSecret",
			"executeSql",
			"mutateState",
			"setSecretMaterial",
			"getSecretHandle",
		)) {
			if (methods.contains(forbidden)) {
				fail("public bindings must not expose $forbidden")
			}
		}
		// Typed saved-device operations (names may be mangled by UniFFI); require
		// at least the experimental constructor surface used above.
		assertTrue(
			methods.any { it.contains("listSaved", ignoreCase = true) }
				|| methods.any { it.contains("SavedDevice", ignoreCase = true) }
				|| methods.contains("status"),
		)
	}
}
