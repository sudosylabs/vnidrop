package com.vnidrop.app.ui.navigation

import androidx.compose.foundation.interaction.HoverInteraction
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.ui.geometry.Offset
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.ui.state.WindowClass
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals

class NavigationModelTest {
	@Test
	fun androidNavigationInteractionSourceSuppressesPressVisuals() = runTest {
		val source = PresslessInteractionSource()
		val firstInteraction = async(start = CoroutineStart.UNDISPATCHED) { source.interactions.first() }

		source.emit(PressInteraction.Press(Offset.Zero))
		source.emit(HoverInteraction.Enter())

		assertEquals(HoverInteraction.Enter::class, firstInteraction.await()::class)
	}

	@Test
	fun primaryNavigationContainsOnlyProductDestinations() {
		assertEquals(
			listOf(AppDestination.Send, AppDestination.Receive, AppDestination.SavedDevices, AppDestination.Settings),
			primaryNavigationItems.map { it.destination },
		)
		assertEquals(4, primaryNavigationItems.map { it.label }.distinct().size)
	}

	@Test
	fun androidNavigationFollowsMaterialWindowConventions() {
		assertEquals(NavigationStyle.AndroidBottomBar, navigationStyleFor(UiPlatform.Android, WindowClass.Phone))
		assertEquals(NavigationStyle.AndroidRail, navigationStyleFor(UiPlatform.Android, WindowClass.Tablet))
		assertEquals(NavigationStyle.AndroidRail, navigationStyleFor(UiPlatform.Android, WindowClass.Desktop))
	}

	@Test
	fun desktopPlatformsUseSourceListNavigationAtEveryWindowSize() {
		listOf(UiPlatform.Windows, UiPlatform.Linux, UiPlatform.Desktop).forEach { platform ->
			WindowClass.entries.forEach { windowClass ->
				assertEquals(NavigationStyle.DesktopSidebar, navigationStyleFor(platform, windowClass))
			}
		}
	}
}
