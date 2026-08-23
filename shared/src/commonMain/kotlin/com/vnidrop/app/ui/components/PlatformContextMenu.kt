package com.vnidrop.app.ui.components

import androidx.compose.runtime.Composable

internal data class AppContextMenuItem(
	val label: String,
	val onClick: () -> Unit,
)

@Composable
internal expect fun PlatformContextMenu(
	items: List<AppContextMenuItem>,
	content: @Composable () -> Unit,
)
