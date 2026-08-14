package com.vnidrop.app.core

import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import uniffi.vnidrop.CoreEvent
import uniffi.vnidrop.CoreEventSink
import uniffi.vnidrop.VnidropCore
import uniffi.vnidrop.defaultCoreLimits
import uniffi.vnidrop.defaultCoreNetworkConfig
import uniffi.vnidrop.savedDeviceCapabilities

/**
 * Linux host harness for the saved-device core contract.
 *
 * Identity restart against Secret Service runs only on Linux (CI host). Binding
 * hygiene assertions run on every JVM so macOS/Windows desktop checks stay useful.
 */
class SavedDeviceCoreContractLinuxTest {
	@Test
	fun productionInitRestartsSameIdentityOnLinux() {
		val capabilities = savedDeviceCapabilities()
		assertTrue(capabilities.domainContractVersion >= 1u)

		val isLinux = System.getProperty("os.name").orEmpty().lowercase().contains("linux")
		if (!isLinux) {
			return
		}

		val coreDir = Files.createTempDirectory("vnidrop-linux-contract")
		val sink = object : CoreEventSink {
			override fun onEvent(event: CoreEvent) = Unit
		}
		val first = VnidropCore.initializeWithLimitsAndNetworkConfig(
			appDataDir = coreDir.toString(),
			eventSink = sink,
			limits = defaultCoreLimits(),
			networkConfig = defaultCoreNetworkConfig(),
		)
		val endpointId = try {
			val id = first.status().endpointId
			assertTrue(id.isNotBlank())
			assertFalse(coreDir.resolve("iroh.secret").toFile().exists())
			id
		} finally {
			try {
				first.shutdown()
			} finally {
				first.close()
			}
		}

		val second = VnidropCore.initializeWithLimitsAndNetworkConfig(
			appDataDir = coreDir.toString(),
			eventSink = sink,
			limits = defaultCoreLimits(),
			networkConfig = defaultCoreNetworkConfig(),
		)
		try {
			assertEquals(endpointId, second.status().endpointId)
			assertFalse(coreDir.resolve("iroh.secret").toFile().exists())
			// Public surface used by the contract is present on bindings.
			second.listSavedDevices()
			second.listDeviceRelationships()
			second.listPairingEligibilities()
			second.listBlockedDevices()
			second.listTargetedTransfers()
			second.listEvents(null)
		} finally {
			try {
				second.shutdown()
			} finally {
				second.close()
			}
			coreDir.toFile().deleteRecursively()
		}
	}

	@Test
	fun publicBindingsOmitRawSecretsAndGenericMutation() {
		val instanceMethods = VnidropCore::class.java.methods.map { it.name }.toSet()
		val companionMethods = VnidropCore.Companion::class.java.methods.map { it.name }.toSet()
		val methodNames = instanceMethods + companionMethods
		for (forbidden in listOf(
			"putSecret",
			"getSecret",
			"executeSql",
			"executeSQL",
			"mutateState",
			"applyRawState",
			"setRawState",
			"grantSecret",
			"rawSecret",
			"pairingCapabilityBytes",
		)) {
			assertFalse(
				methodNames.contains(forbidden),
				"VnidropCore must not expose $forbidden",
			)
		}
		assertTrue(
			companionMethods.contains("initializeWithLimitsAndNetworkConfig"),
			"production protected init must be on the public binding",
		)
		assertTrue(
			instanceMethods.contains("setSavedDeviceLabel"),
			"typed rename must be on the public binding",
		)
		assertTrue(
			instanceMethods.any { it.startsWith("listEvents") },
			"event listing must be on the public binding",
		)

		val eventFields = CoreEvent::class.java.declaredFields.map { it.name }.toSet()
		assertTrue(eventFields.contains("revision"), "CoreEvent must carry revision")
		assertTrue(eventFields.contains("id"), "CoreEvent must carry stable id")
		for (forbidden in listOf("secret", "grantBytes", "capabilityBytes", "secretMaterial")) {
			assertFalse(
				eventFields.any { it.equals(forbidden, ignoreCase = true) },
				"CoreEvent must not expose $forbidden",
			)
		}
	}
}
