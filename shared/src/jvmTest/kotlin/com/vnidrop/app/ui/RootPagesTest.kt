package com.vnidrop.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.v2.runComposeUiTest
import androidx.compose.ui.unit.dp
import com.vnidrop.app.ui.navigation.AppDestination
import com.vnidrop.app.ui.navigation.LocalPageActive
import com.vnidrop.app.ui.navigation.RootPages
import kotlin.test.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalTestApi::class)
class RootPagesTest {
	@Test
	fun swipeAndTabSelectionStayInSyncAndRestoreScroll() = runComposeUiTest {
		val selected = mutableStateOf(AppDestination.Send)
		setContent {
			Box(Modifier.size(393.dp, 640.dp)) {
				RootPages(selected.value, true, { selected.value = it }) { page ->
					Box(Modifier.fillMaxSize()) {
					val scroll = rememberLazyListState()
					LazyColumn(Modifier.fillMaxSize().testTag(page.name), state = scroll) {
						items(80) { index -> Text("${page.name} $index", Modifier.height(60.dp)) }
					}
					if (LocalPageActive.current) Text("Active ${page.name}")
					}
				}
			}
		}
		onNodeWithTag("Send").performScrollToIndex(30)
		onNodeWithTag("root-pages").performTouchInput { swipeLeft() }
		waitForIdle()
		runOnIdle { assertEquals(AppDestination.Receive, selected.value) }
		onNodeWithText("Active Receive").assertIsDisplayed()
		runOnIdle { selected.value = AppDestination.Settings }
		onNodeWithTag("Settings").assertIsDisplayed()
		runOnIdle { selected.value = AppDestination.Send }
		onNodeWithText("Send 30").assertIsDisplayed()
		onNodeWithText("Active Send").assertIsDisplayed()
	}

	@Test
	fun detailScreenBlocksPageSwipeAndCanResumeIt() = runComposeUiTest {
		val selected = mutableStateOf(AppDestination.Receive)
		val enabled = mutableStateOf(false)
		setContent {
			Box(Modifier.size(393.dp, 640.dp)) {
				RootPages(selected.value, enabled.value, { selected.value = it }) { Text(it.name) }
			}
		}
		onNodeWithTag("root-pages").performTouchInput { swipeRight() }
		runOnIdle {
			assertEquals(AppDestination.Receive, selected.value)
			enabled.value = true
		}
		onNodeWithTag("root-pages").performTouchInput { swipeRight() }
		runOnIdle { assertEquals(AppDestination.Send, selected.value) }
	}
}
