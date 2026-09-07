package com.vnidrop.app.ui.platform

import androidx.compose.runtime.Composable
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

@Composable
internal actual fun PlatformBackHandler(enabled: Boolean, onBack: () -> Unit) = Unit

@Composable
internal actual fun FullscreenDialog(onDismissRequest: () -> Unit, content: @Composable () -> Unit) {
	Dialog(
		onDismissRequest = onDismissRequest,
		properties = DialogProperties(usePlatformDefaultWidth = false),
		content = content,
	)
}
