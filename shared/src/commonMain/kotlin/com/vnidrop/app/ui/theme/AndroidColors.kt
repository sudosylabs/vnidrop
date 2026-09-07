package com.vnidrop.app.ui.theme

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.ui.graphics.Color

internal fun androidColorScheme(dark: Boolean): ColorScheme = if (dark) {
	darkColorScheme(
		primary = Color(0xFFDAB9FF), onPrimary = Color(0xFF430078),
		primaryContainer = Color(0xFF6100A7), onPrimaryContainer = Color(0xFFF0DBFF),
		secondary = Color(0xFFD0C1DA), onSecondary = Color(0xFF362C3E),
		secondaryContainer = Color(0xFF4D4256), onSecondaryContainer = Color(0xFFECDDF6),
		tertiary = Color(0xFFF2B7C5), onTertiary = Color(0xFF4A2530),
		tertiaryContainer = Color(0xFF643B46), onTertiaryContainer = Color(0xFFFFD9E2),
		background = Color(0xFF111315), onBackground = Color(0xFFE3E3E6),
		surface = Color(0xFF111315), onSurface = Color(0xFFE3E3E6),
		surfaceVariant = Color(0xFF44474B), onSurfaceVariant = Color(0xFFC5C6CA),
		surfaceDim = Color(0xFF111315), surfaceBright = Color(0xFF37393C),
		surfaceContainerLowest = Color(0xFF0C0E10), surfaceContainerLow = Color(0xFF1B1C1F),
		surfaceContainer = Color(0xFF1F2123), surfaceContainerHigh = Color(0xFF292B2E),
		surfaceContainerHighest = Color(0xFF343639),
		outline = Color(0xFF8F9195), outlineVariant = Color(0xFF44474B),
		error = Color(0xFFFFB4AB), onError = Color(0xFF690005),
		errorContainer = Color(0xFF93000A), onErrorContainer = Color(0xFFFFDAD6),
		surfaceTint = Color(0xFFDAB9FF), inverseSurface = Color(0xFFE3E3E6),
		inverseOnSurface = Color(0xFF303235), inversePrimary = Color(0xFF8035B5),
	)
} else {
	lightColorScheme(
		primary = Color(0xFF8035B5), onPrimary = Color.White,
		primaryContainer = Color(0xFFF0DBFF), onPrimaryContainer = Color(0xFF2B0052),
		secondary = Color(0xFF65586F), onSecondary = Color.White,
		secondaryContainer = Color(0xFFECDDF6), onSecondaryContainer = Color(0xFF21182A),
		tertiary = Color(0xFF7F525E), onTertiary = Color.White,
		tertiaryContainer = Color(0xFFFFD9E2), onTertiaryContainer = Color(0xFF32101B),
		background = Color(0xFFFCFCFF), onBackground = Color(0xFF1B1C1F),
		surface = Color(0xFFFCFCFF), onSurface = Color(0xFF1B1C1F),
		surfaceVariant = Color(0xFFE2E3E7), onSurfaceVariant = Color(0xFF44474B),
		surfaceDim = Color(0xFFDADADD), surfaceBright = Color(0xFFFCFCFF),
		surfaceContainerLowest = Color.White, surfaceContainerLow = Color(0xFFF5F5F8),
		surfaceContainer = Color(0xFFF0F0F3), surfaceContainerHigh = Color(0xFFEAEAED),
		surfaceContainerHighest = Color(0xFFE3E3E6),
		outline = Color(0xFF74777B), outlineVariant = Color(0xFFC5C6CA),
		error = Color(0xFFBA1A1A), onError = Color.White,
		errorContainer = Color(0xFFFFDAD6), onErrorContainer = Color(0xFF410002),
		surfaceTint = Color(0xFF8035B5), inverseSurface = Color(0xFF303235),
		inverseOnSurface = Color(0xFFF2F2F5), inversePrimary = Color(0xFFDAB9FF),
	)
}

internal fun VniDropColors.withMaterialColors(scheme: ColorScheme): VniDropColors = copy(
	backgroundDefault = scheme.background,
	backgroundDashCanvas = scheme.surface,
	backgroundDashSidebar = scheme.surfaceContainer,
	backgroundSurface75 = scheme.surface,
	backgroundSurface100 = scheme.surfaceContainerLowest,
	backgroundSurface200 = scheme.surfaceContainerLow,
	backgroundSurface300 = scheme.surfaceContainer,
	backgroundSurface400 = scheme.surfaceContainerHigh,
	backgroundMuted = scheme.surfaceContainerLow,
	backgroundControl = scheme.surfaceContainerHighest,
	backgroundSelection = scheme.secondaryContainer,
	backgroundButton = scheme.secondaryContainer,
	backgroundOverlayHover = scheme.surfaceContainerHigh,
	backgroundDialog = scheme.surfaceContainerHigh,
	borderDefault = scheme.outlineVariant,
	borderStrong = scheme.outline,
	borderStronger = scheme.outline,
	borderMuted = scheme.outlineVariant,
	borderControl = scheme.outline,
	foregroundDefault = scheme.onSurface,
	foregroundLight = scheme.onSurfaceVariant,
	foregroundLighter = scheme.onSurfaceVariant,
	foregroundMuted = scheme.onSurface.copy(alpha = 0.38f),
	foregroundContrast = scheme.onPrimary,
	brandDefault = scheme.primary,
	brand200 = scheme.primaryContainer,
	brand600 = scheme.onPrimaryContainer,
	brandLink = scheme.primary,
	brandButton = scheme.primary,
	destructiveDefault = scheme.error,
	destructive200 = scheme.errorContainer,
	destructive600 = scheme.onErrorContainer,
)
