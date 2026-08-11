package com.vnidrop.app

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ExperimentalSavedDevicesGateTest {
	@Test
	fun experimentalChromeIsShownOnAndroidWindowsAndLinuxOnly() {
		assertTrue(showsExperimentalSavedDevices(UiPlatform.Android))
		assertTrue(showsExperimentalSavedDevices(UiPlatform.Windows))
		assertTrue(showsExperimentalSavedDevices(UiPlatform.Linux))
		assertFalse(showsExperimentalSavedDevices(UiPlatform.Desktop))
	}
}
