package com.vnidrop.app.ui.navigation

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.vnidrop.app.ui.icons.AppIcon
import com.vnidrop.app.ui.icons.PlatformIcon
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_back

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal actual fun AndroidMainNavigation(
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
) {
	BackHandler(enabled = onBack != null) { onBack?.invoke() }
	BoxWithConstraints(Modifier.fillMaxSize()) {
		val rail = maxWidth >= 600.dp
		Row(Modifier.fillMaxSize()) {
			if (rail && showNavigation) AppSidebarNavigation(selected, NavigationStyle.AndroidRail, onSelected)
			Scaffold(
				modifier = Modifier.weight(1f),
				containerColor = MaterialTheme.colorScheme.surface,
				topBar = {
					if (showTopBar) TopAppBar(
						title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
						navigationIcon = {
							if (onBack != null) IconButton(onClick = onBack) {
								PlatformIcon(AppIcon.ArrowBack, stringResource(Res.string.button_back))
							}
						},
					)
				},
				bottomBar = { if (!rail && showNavigation) AppBottomNavigation(selected, onSelected) },
				floatingActionButton = floatingAction,
				snackbarHost = snackbarHost,
			) { insets ->
				Box(Modifier.fillMaxSize().padding(insets).consumeWindowInsets(insets)) {
					CompositionLocalProvider(LocalRootScaffold provides true) {
						RootPages(selected, swipeEnabled, onSelected, content)
					}
				}
			}
		}
	}
}
