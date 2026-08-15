package com.vnidrop.app.platform

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class AppVisibilityTest {
	@Test
	fun desktopWindowFocusControlsForegroundVisibility() {
		val visibility = AppVisibility(initiallyForeground = true)

		visibility.setWindowFocused(false)
		assertFalse(visibility.isForeground.value)

		visibility.setWindowFocused(true)
		assertTrue(visibility.isForeground.value)
	}
}
