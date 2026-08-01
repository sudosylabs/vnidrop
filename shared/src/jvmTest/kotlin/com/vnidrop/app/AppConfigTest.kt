package com.vnidrop.app

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Verifies the build-time [AppConfig] (generated from the shared `app.properties`)
 * exposes the expected, well-formed values to the shared UI.
 */
class AppConfigTest {
	@Test
	fun privacyPolicyUrlIsTheExpectedHttpsEndpoint() {
		assertTrue(AppConfig.PRIVACY_POLICY_URL.startsWith("https://"), "must be https")
		assertEquals("https://vnidrop.sudosy.fr/privacy/", AppConfig.PRIVACY_POLICY_URL)
	}

	@Test
	fun privacyPolicyUrlMatchesTheSharedConfigFile() {
		// Cross-check the generated constant against the single source of truth so a
		// broken codegen (or drift) is caught, not just a hardcoded copy.
		assertEquals(privacyUrlFromAppProperties(), AppConfig.PRIVACY_POLICY_URL)
	}

	private fun privacyUrlFromAppProperties(): String {
		var dir: File? = File(System.getProperty("user.dir")).absoluteFile
		repeat(8) {
			val candidate = File(dir, "app.properties")
			if (candidate.isFile) {
				return candidate.readLines()
					.firstOrNull { it.startsWith("PRIVACY_POLICY_URL=") }
					?.substringAfter("PRIVACY_POLICY_URL=")
					?: error("PRIVACY_POLICY_URL missing in ${candidate.path}")
			}
			dir = dir?.parentFile
		}
		error("app.properties not found from ${System.getProperty("user.dir")}")
	}
}
