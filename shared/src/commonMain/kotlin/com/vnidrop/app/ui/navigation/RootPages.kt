package com.vnidrop.app.ui.navigation

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filterNotNull

internal val LocalRootScaffold = staticCompositionLocalOf { false }
internal val LocalPageActive = staticCompositionLocalOf { true }

@Composable
internal fun RootPages(
	selected: AppDestination,
	swipeEnabled: Boolean,
	onSelected: (AppDestination) -> Unit,
	content: @Composable (AppDestination) -> Unit,
) {
	val pages = AppDestination.entries
	val pager = rememberPagerState(initialPage = pages.indexOf(selected)) { pages.size }
	val onSelectedState = rememberUpdatedState(onSelected)
	LaunchedEffect(selected) {
		if (pager.currentPage != pages.indexOf(selected)) pager.animateScrollToPage(pages.indexOf(selected))
	}
	LaunchedEffect(pager) {
		snapshotFlow { if (pager.isScrollInProgress) null else pager.settledPage }
			.filterNotNull().distinctUntilChanged().drop(1)
			.collect { onSelectedState.value(pages[it]) }
	}
	HorizontalPager(
		state = pager,
		key = { pages[it].name },
		userScrollEnabled = swipeEnabled,
		modifier = Modifier.fillMaxSize().testTag("root-pages"),
	) { page ->
		CompositionLocalProvider(LocalPageActive provides (pages[page] == selected)) { content(pages[page]) }
	}
}

@Composable
internal expect fun AndroidMainNavigation(
	selected: AppDestination,
	title: String,
	showTopBar: Boolean,
	showNavigation: Boolean,
	swipeEnabled: Boolean,
	onBack: (() -> Unit)?,
	onSelected: (AppDestination) -> Unit,
	floatingAction: @Composable () -> Unit,
	snackbarHost: @Composable () -> Unit,
	content: @Composable (AppDestination) -> Unit,
)
