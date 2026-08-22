package com.vnidrop.app.ui.components

import androidx.compose.foundation.ContextMenuArea
import androidx.compose.foundation.ContextMenuItem
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics

@Composable
internal actual fun PlatformContextMenu(
	items: List<AppContextMenuItem>,
	content: @Composable () -> Unit,
) {
	if (items.isEmpty()) {
		content()
	} else {
		ContextMenuArea(
			items = { items.map { ContextMenuItem(it.label, it.onClick) } },
		) {
			Box(
				Modifier.semantics {
					customActions = items.map { item ->
						CustomAccessibilityAction(item.label) {
							item.onClick()
							true
						}
					}
				},
			) { content() }
		}
	}
}
