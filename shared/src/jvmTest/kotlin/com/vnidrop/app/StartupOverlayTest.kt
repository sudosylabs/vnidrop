package com.vnidrop.app

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.v2.runComposeUiTest
import androidx.compose.ui.unit.dp
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.theme.VniDropTheme
import kotlin.test.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalTestApi::class)
class StartupOverlayTest {
	@Test
	fun showsIconImmediatelyAndDelaysStatusCopy() = runComposeUiTest {
		val label = "Starting…"
		mainClock.autoAdvance = false

		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Windows) {
				VniDropTheme(isDarkTheme = true) {
					StartupOverlay(label = label)
				}
			}
		}

		onNodeWithTag(StartupIconTestTag).assertIsDisplayed()
		onAllNodesWithTag(StartupLabelTestTag).assertCountEquals(0)
		onNodeWithContentDescription(label).assertIsDisplayed()

		mainClock.advanceTimeBy(StartupDetailsDelayMillis + StartupLabelFadeDurationMillis + 100)
		waitForIdle()

		onNodeWithTag(StartupLabelTestTag).assertIsDisplayed()
	}

	@Test
	fun keepsWindowChromeAboveStartupOverlay() = runComposeUiTest {
		setContent {
			VniDropTheme(isDarkTheme = true) {
				Box(Modifier.size(200.dp)) {
					StartupLayer(
						label = "Starting…",
						showOverlay = true,
						windowChrome = {
							Box(
								Modifier
									.fillMaxWidth()
									.height(48.dp)
									.background(Color.Red),
							)
						},
					)
				}
			}
		}

		val pixel = onRoot().captureToImage().toPixelMap()[1, 1]
		assertEquals(Color.Red, pixel)
	}
}
