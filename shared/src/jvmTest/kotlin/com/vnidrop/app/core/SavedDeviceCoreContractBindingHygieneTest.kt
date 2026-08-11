package com.vnidrop.app.core

import java.io.File
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * Android host contract: generated UniFFI Kotlin must not expose raw secrets
 * or a generic state-mutation escape hatch. Runs without a device/Keystore.
 */
class SavedDeviceCoreContractBindingHygieneTest {

	@Test
	fun generatedPublicBindingsOmitSecretsAndGenericMutation() {
		val binding = resolveGeneratedBinding()
		val source = binding.readText()

		val forbidden = listOf(
			"SecretMaterial",
			"SecretHandle",
			"SecureSecretStore",
			"executeSql",
			"mutateState",
			"rawSecret",
			"setRawState",
			"iroh.secret",
		)
		for (token in forbidden) {
			assertFalse(
				source.contains(token),
				"generated binding ${binding.path} must not expose `$token`",
			)
		}

		assertTrue(
			source.contains("initializeWithExperimentalSavedDevices"),
			"experimental saved-device initializer must remain public",
		)
		assertTrue(
			source.contains("SavedDevice"),
			"SavedDevice model must remain on the public surface",
		)
		assertTrue(
			source.contains("revision"),
			"CoreEvent.revision must remain on the public surface for recovery",
		)
	}

	private fun resolveGeneratedBinding(): File {
		val candidates = listOf(
			File("build/generated/uniffi/commonMain/kotlin/uniffi/vnidrop/vnidrop.common.kt"),
			File("shared/build/generated/uniffi/commonMain/kotlin/uniffi/vnidrop/vnidrop.common.kt"),
			File("../shared/build/generated/uniffi/commonMain/kotlin/uniffi/vnidrop/vnidrop.common.kt"),
		)
		return candidates.firstOrNull { it.isFile }
			?: error(
				"UniFFI Kotlin binding not found under build/generated; " +
					"run a shared Gobley/UniFFI generate step first",
			)
	}
}
