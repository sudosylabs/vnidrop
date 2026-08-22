package com.vnidrop.app.ui.components

import androidx.compose.runtime.Composable

@Composable
internal actual fun PlatformContextMenu(
	items: List<AppContextMenuItem>,
	content: @Composable () -> Unit,
) {
	content()
}
