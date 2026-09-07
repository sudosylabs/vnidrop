package com.vnidrop.app.ui.theme

import android.os.Build
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext

internal actual fun supportsDynamicColors(): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S

@Composable
internal actual fun platformColorScheme(dark: Boolean, dynamic: Boolean, fallback: ColorScheme): ColorScheme =
	if (dynamic && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
		val context = LocalContext.current
		if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
	} else fallback
