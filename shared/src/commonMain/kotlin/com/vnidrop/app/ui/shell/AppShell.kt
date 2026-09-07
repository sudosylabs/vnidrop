package com.vnidrop.app.ui.shell

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.ui.navigation.AppBottomNavigation
import com.vnidrop.app.ui.navigation.AppDestination
import com.vnidrop.app.ui.navigation.AppSidebarNavigation
import com.vnidrop.app.ui.navigation.LocalRootScaffold
import com.vnidrop.app.ui.navigation.NavigationStyle
import com.vnidrop.app.ui.navigation.navigationStyleFor
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.LocalVniDropColors

@Composable
fun AppShell(
	modifier: Modifier = Modifier,
	selectedDestination: AppDestination,
	windowClass: WindowClass,
	uiPlatform: UiPlatform,
	mainContentTopStartRadius: Dp = 0.dp,
	useNativeWindowBackdrop: Boolean = false,
	showNavigation: Boolean = true,
	onDestinationSelected: (AppDestination) -> Unit,
	overlay: @Composable BoxScope.() -> Unit = {},
	floatingAction: (@Composable BoxScope.() -> Unit)? = null,
	content: @Composable () -> Unit,
) {
	val colors = LocalVniDropColors.current
	val navigationStyle = navigationStyleFor(uiPlatform, windowClass)
	val windowSurface = if (useNativeWindowBackdrop) Color.Transparent else colors.backgroundDashCanvas
	Surface(
		modifier = modifier
			.fillMaxSize()
			.background(windowSurface),
		color = windowSurface,
		contentColor = colors.foregroundDefault,
	) {
		if (navigationStyle == NavigationStyle.AndroidBottomBar) {
			PhoneShell(
				selectedDestination = selectedDestination,
				onDestinationSelected = onDestinationSelected,
				showNavigation = showNavigation,
				overlay = overlay,
				floatingAction = floatingAction,
				content = content,
			)
		} else {
			WideShell(
				selectedDestination = selectedDestination,
				navigationStyle = navigationStyle,
				mainContentTopStartRadius = mainContentTopStartRadius,
				useNativeWindowBackdrop = useNativeWindowBackdrop,
				onDestinationSelected = onDestinationSelected,
				overlay = overlay,
				floatingAction = floatingAction,
				content = content,
			)
		}
	}
}

@Composable
private fun WideShell(
	selectedDestination: AppDestination,
	navigationStyle: NavigationStyle,
	mainContentTopStartRadius: Dp,
	useNativeWindowBackdrop: Boolean,
	onDestinationSelected: (AppDestination) -> Unit,
	overlay: @Composable BoxScope.() -> Unit,
	floatingAction: (@Composable BoxScope.() -> Unit)?,
	content: @Composable () -> Unit,
) {
	val colors = LocalVniDropColors.current
	val roundedContent = mainContentTopStartRadius > 0.dp
	Row(
		modifier = Modifier
			.fillMaxSize()
			.then(
				if (roundedContent && !useNativeWindowBackdrop) {
					Modifier.background(colors.backgroundSurface200)
				} else {
					Modifier
				},
			),
	) {
		AppSidebarNavigation(
			selected = selectedDestination,
			style = navigationStyle,
			useNativeWindowBackdrop = useNativeWindowBackdrop,
			onDestinationSelected = onDestinationSelected,
		)
		Box(
			modifier = Modifier
				.weight(1f)
				.fillMaxSize()
				.then(
					if (roundedContent) {
						Modifier.clip(RoundedCornerShape(topStart = mainContentTopStartRadius))
					} else {
						Modifier
					},
				)
				.background(colors.backgroundDashCanvas),
		) {
			content()
			floatingAction?.invoke(this)
			Box(Modifier.fillMaxSize().padding(bottom = if (floatingAction == null) 0.dp else 72.dp)) { overlay() }
		}
	}
}

@Composable
private fun PhoneShell(
	selectedDestination: AppDestination,
	onDestinationSelected: (AppDestination) -> Unit,
	showNavigation: Boolean,
	overlay: @Composable BoxScope.() -> Unit,
	floatingAction: (@Composable BoxScope.() -> Unit)?,
	content: @Composable () -> Unit,
) {
	Scaffold(
		containerColor = LocalVniDropColors.current.backgroundDashCanvas,
		contentWindowInsets = WindowInsets(0, 0, 0, 0),
		bottomBar = {
			if (showNavigation) AppBottomNavigation(
				selected = selectedDestination,
				onDestinationSelected = onDestinationSelected,
			)
		},
		floatingActionButton = { floatingAction?.let { Box(content = it) } },
		snackbarHost = { Box(content = overlay) },
	) { padding ->
		Box(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding)) { content() }
	}
}

@Composable
fun ScreenScrollContainer(
	modifier: Modifier = Modifier,
	contentPadding: PaddingValues = PaddingValues(16.dp),
	content: @Composable () -> Unit,
) {
	LazyColumn(
		modifier = modifier
			.fillMaxSize()
			.then(if (LocalRootScaffold.current) Modifier else Modifier.statusBarsPadding()),
		contentPadding = contentPadding,
	) {
		item {
			content()
		}
	}
}
