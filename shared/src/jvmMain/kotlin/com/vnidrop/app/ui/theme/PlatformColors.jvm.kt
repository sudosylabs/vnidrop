package com.vnidrop.app.ui.theme

import androidx.compose.material3.ColorScheme
import androidx.compose.runtime.Composable

internal actual fun supportsDynamicColors(): Boolean = false

@Composable
internal actual fun platformColorScheme(dark: Boolean, dynamic: Boolean, fallback: ColorScheme): ColorScheme = fallback
