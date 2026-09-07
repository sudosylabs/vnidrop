package com.vnidrop.app.platform

import android.app.Activity
import android.os.Build
import android.view.View
import android.view.Window
import android.view.WindowInsetsController
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalView

@Composable
actual fun PlatformSystemAppearance(isDarkTheme: Boolean) {
	val view = LocalView.current
	if (view.isInEditMode) return

	SideEffect {
		val window = (view.context as? Activity)?.window ?: return@SideEffect
		window.updateSystemAppearance(isDarkTheme)
	}
}


internal fun Window.updateSystemAppearance(isDarkTheme: Boolean) {
	this.statusBarColor = Color.Transparent.toArgb()
	this.navigationBarColor = Color.Transparent.toArgb()
	val useDarkIcons = systemBarIconModeForTheme(isDarkTheme) == SystemBarIconMode.DarkIcons
	if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
		val lightBars = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
			WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
		this.insetsController?.setSystemBarsAppearance(if (useDarkIcons) lightBars else 0, lightBars)
	} else {
		var flags = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
			View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
			View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
		if (useDarkIcons) {
			flags = flags or View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
			if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
				flags = flags or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
			}
		}
		this.decorView.systemUiVisibility = flags
	}
}
