package com.vnidrop.app.platform

import android.app.Dialog
import androidx.core.view.WindowCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class DialogSystemAppearanceTest {
	@Test
	fun dialogWindowUpdatesBothSystemBarsWhenThemeChanges() {
		val instrumentation = InstrumentationRegistry.getInstrumentation()
		instrumentation.runOnMainSync {
			val dialog = Dialog(instrumentation.targetContext)
			val window = requireNotNull(dialog.window)
			val controller = WindowCompat.getInsetsController(window, window.decorView)
			window.updateSystemAppearance(isDarkTheme = false)
			assertTrue(controller.isAppearanceLightStatusBars)
			assertTrue(controller.isAppearanceLightNavigationBars)
			window.updateSystemAppearance(isDarkTheme = true)
			assertFalse(controller.isAppearanceLightStatusBars)
			assertFalse(controller.isAppearanceLightNavigationBars)
		}
	}
}
