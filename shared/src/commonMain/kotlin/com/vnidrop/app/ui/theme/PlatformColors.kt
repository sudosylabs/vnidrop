package com.vnidrop.app.ui.theme

import androidx.compose.material3.ColorScheme
import androidx.compose.runtime.Composable

internal expect fun supportsDynamicColors(): Boolean

@Composable
internal expect fun platformColorScheme(dark: Boolean, dynamic: Boolean, fallback: ColorScheme): ColorScheme
