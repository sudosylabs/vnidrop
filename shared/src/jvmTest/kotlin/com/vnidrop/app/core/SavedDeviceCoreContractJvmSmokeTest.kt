package com.vnidrop.app.core

import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import uniffi.vnidrop.CoreEvent
import uniffi.vnidrop.CoreEventSink
import uniffi.vnidrop.CoreNetworkConfig
import uniffi.vnidrop.CoreRelayMode
import uniffi.vnidrop.VnidropCore
import uniffi.vnidrop.VnidropException
import uniffi.vnidrop.defaultCoreLimits

/**
 * JVM-adjacent Android contract smoke.
 *
 * Full Keystore-backed identity/restart lives in the Rust
 * `platform_contract_android` harness. Here we prove the regenerated UniFFI
 * surface exposes revision-bearing events and the experimental initializer,
 * exercising protected init when the host credential store is available.
 */
class SavedDeviceCoreContractJvmSmokeTest {

	@Test
	fun eventRevisionsSurviveListenerRestartOnPublicBindings() {
		val coreDir = Files.createTempDirectory("vnidrop-android-contract-jvm")
		val events = mutableListOf<CoreEvent>()
		val sink = object : CoreEventSink {
			override fun onEvent(event: CoreEvent) {
				events += event
			}
		}
		val network = CoreNetworkConfig(CoreRelayMode.LOCAL_ONLY, emptyList())

		val first = VnidropCore.initializeWithNetworkConfig(
			coreDir.toString(),
			sink,
			network,
		)
		val endpointId = try {
			val id = first.status().endpointId
			assertTrue(id.isNotBlank())
			id
		} finally {
			first.shutdown()
		}

		val second = VnidropCore.initializeWithNetworkConfig(
			coreDir.toString(),
			sink,
			network,
		)
		try {
			assertEquals(endpointId, second.status().endpointId)
			val listed = second.listEvents(null)
			assertTrue(listed.isNotEmpty() || events.isNotEmpty())
			for (event in listed.ifEmpty { events }) {
				assertTrue(event.id.isNotBlank())
				assertTrue(event.revision >= 1uL)
			}
		} finally {
			second.shutdown()
			coreDir.toFile().deleteRecursively()
		}
	}

	@Test
	fun experimentalProtectedInitWorksWhenHostSecretStoreIsAvailable() {
		val coreDir = Files.createTempDirectory("vnidrop-android-contract-experimental")
		val sink = object : CoreEventSink {
			override fun onEvent(event: CoreEvent) = Unit
		}
		val network = CoreNetworkConfig(CoreRelayMode.LOCAL_ONLY, emptyList())
		val core = try {
			VnidropCore.initializeWithExperimentalSavedDevices(
				coreDir.toString(),
				sink,
				defaultCoreLimits(),
				network,
			)
		} catch (_: VnidropException.SecureStorageUnavailable) {
			// Desktop hosts may lack a usable credential store; Android Keystore
			// restart is covered by crates/vnidrop platform_contract_android.
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
