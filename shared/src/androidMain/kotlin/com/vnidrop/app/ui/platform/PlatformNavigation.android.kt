package com.vnidrop.app.ui.platform

import androidx.activity.compose.BackHandler
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import com.vnidrop.app.platform.updateSystemAppearance
import com.vnidrop.app.ui.navigation.LocalPageActive

@Composable
internal actual fun PlatformBackHandler(enabled: Boolean, onBack: () -> Unit) {
	BackHandler(enabled = enabled && LocalPageActive.current, onBack = onBack)
}

@Composable
internal actual fun FullscreenDialog(onDismissRequest: () -> Unit, content: @Composable () -> Unit) {
	Dialog(
		onDismissRequest = onDismissRequest,
		properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
	) {
		val window = (LocalView.current.parent as DialogWindowProvider).window
		val isDarkTheme = MaterialTheme.colorScheme.surface.luminance() < 0.5f
		// Dialogs own a separate window, so the activity's system-bar appearance does not apply.
		SideEffect { window.updateSystemAppearance(isDarkTheme) }
		content()
	}
}
