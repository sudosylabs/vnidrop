package com.vnidrop.app.ui.navigation

import com.vnidrop.app.UiPlatform
import com.vnidrop.app.ui.state.WindowClass
import kotlin.test.Test
import kotlin.test.assertEquals

class NavigationModelTest {
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
