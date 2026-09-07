package com.vnidrop.app.ui.navigation

import androidx.compose.runtime.Composable

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
) = content(selected)
