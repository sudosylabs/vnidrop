package com.vnidrop.app.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.ui.platform.LocalUiPlatform
import kotlin.math.max
import kotlin.math.min

enum class ThemeMode {
	System,
	Light,
	Dark,
}

fun resolveDarkTheme(mode: ThemeMode, systemDark: Boolean): Boolean =
	when (mode) {
		ThemeMode.System -> systemDark
		ThemeMode.Light -> false
		ThemeMode.Dark -> true
	}

@Composable
fun rememberResolvedDarkTheme(mode: ThemeMode): Boolean =
	resolveDarkTheme(mode, isSystemInDarkTheme())

@Immutable
data class VniDropColors(
	val backgroundDefault: Color,
	val backgroundDashCanvas: Color,
	val backgroundDashSidebar: Color,
	val backgroundSurface75: Color,
	val backgroundSurface100: Color,
	val backgroundSurface200: Color,
	val backgroundSurface300: Color,
	val backgroundSurface400: Color,
	val backgroundMuted: Color,
	val backgroundControl: Color,
	val backgroundSelection: Color,
	val backgroundButton: Color,
	val backgroundOverlayHover: Color,
	val backgroundDialog: Color,
	val borderDefault: Color,
	val borderStrong: Color,
	val borderStronger: Color,
	val borderMuted: Color,
	val borderControl: Color,
	val foregroundDefault: Color,
	val foregroundLight: Color,
	val foregroundLighter: Color,
	val foregroundMuted: Color,
	val foregroundContrast: Color,
	val brandLink: Color,
	val brandButton: Color,
	val brandDefault: Color,
	val brand600: Color,
	val brand500: Color,
	val brand400: Color,
	val brand300: Color,
	val brand200: Color,
	val warningDefault: Color,
	val warning200: Color,
	val warning300: Color,
	val warning400: Color,
	val warning500: Color,
	val warning600: Color,
	val destructiveDefault: Color,
	val destructive200: Color,
	val destructive300: Color,
	val destructive400: Color,
	val destructive500: Color,
	val destructive600: Color,
)

val LocalVniDropColors = staticCompositionLocalOf { VniDropThemeTokens.light }

object VniDropThemeTokens {
	val cameraOverlay = Color.White

	// These values are a direct Compose port of the legacy Tauri theme tokens.
	// The app uses these semantic tokens directly because Material3's ColorScheme
	// cannot represent the full surface, border, and foreground stack.
	val light = VniDropColors(
		backgroundDefault = hsl(0f, 0f, 98.8f),
		backgroundDashCanvas = hsl(0f, 0f, 97.3f),
		backgroundDashSidebar = hsl(0f, 0f, 98.8f),
		backgroundSurface75 = hsl(0f, 0f, 100f),
		backgroundSurface100 = hsl(0f, 0f, 98.8f),
		backgroundSurface200 = hsl(0f, 0f, 95.3f),
		backgroundSurface300 = hsl(0f, 0f, 92.9f),
		backgroundSurface400 = hsl(0f, 0f, 89.8f),
		backgroundMuted = hsl(0f, 0f, 96.9f),
		backgroundControl = hsl(0f, 0f, 95.3f),
		backgroundSelection = hsl(0f, 0f, 92.9f),
		backgroundButton = hsl(0f, 0f, 91f),
		backgroundOverlayHover = hsl(0f, 0f, 95.3f),
		backgroundDialog = hsl(0f, 0f, 100f),
		borderDefault = hsl(0f, 0f, 87.5f),
		borderStrong = hsl(0f, 0f, 83.1f),
		borderStronger = hsl(0f, 0f, 56.1f),
		borderMuted = hsl(0f, 0f, 92.9f),
		borderControl = hsl(0f, 0f, 78f),
		foregroundDefault = hsl(0f, 0f, 9f),
		foregroundLight = hsl(0f, 0f, 32.2f),
		foregroundLighter = hsl(0f, 0f, 43.9f),
		foregroundMuted = hsl(0f, 0f, 69.8f),
		foregroundContrast = hsl(0f, 0f, 98.4f),
		brandLink = hsl(271f, 91f, 65f),
		brandButton = hsl(270f, 95f, 75f),
		brandDefault = hsl(271f, 91f, 65f),
		brand600 = hsl(271f, 81f, 56f),
		brand500 = hsl(271f, 91f, 65f),
		brand400 = hsl(270f, 95f, 75f),
		brand300 = hsl(269f, 97f, 85f),
		brand200 = hsl(269f, 100f, 92f),
		warningDefault = hsl(38.9f, 100f, 57.1f),
		warning600 = hsl(30.3f, 80.3f, 47.8f),
		warning500 = hsl(36.3f, 85.7f, 67.1f),
		warning400 = hsl(41.9f, 100f, 81.8f),
		warning300 = hsl(44.3f, 100f, 91.8f),
		warning200 = hsl(40f, 81.8f, 97.8f),
		destructiveDefault = hsl(10.2f, 77.9f, 53.9f),
		destructive600 = hsl(9.9f, 82f, 43.5f),
		destructive500 = hsl(10.4f, 77.1f, 79.4f),
		destructive400 = hsl(7.1f, 91.3f, 91f),
		destructive300 = hsl(7.1f, 100f, 96.7f),
		destructive200 = hsl(0f, 100f, 99.4f),
	)

	val dark = VniDropColors(
		backgroundDefault = hsl(0f, 0f, 7.1f),
		backgroundDashCanvas = hsl(0f, 0f, 7.1f),
		backgroundDashSidebar = hsl(0f, 0f, 9f),
		backgroundSurface75 = hsl(0f, 0f, 9f),
		backgroundSurface100 = hsl(0f, 0f, 12.2f),
		backgroundSurface200 = hsl(0f, 0f, 12.9f),
		backgroundSurface300 = hsl(0f, 0f, 16.1f),
		backgroundSurface400 = hsl(0f, 0f, 16.1f),
		backgroundMuted = hsl(0f, 0f, 14.1f),
		backgroundControl = hsl(0f, 0f, 14.1f),
		backgroundSelection = hsl(0f, 0f, 19.2f),
		backgroundButton = hsl(0f, 0f, 18f),
		backgroundOverlayHover = hsl(0f, 0f, 18f),
		backgroundDialog = hsl(0f, 0f, 7.1f),
		borderDefault = hsl(0f, 0f, 18f),
		borderStrong = hsl(0f, 0f, 21.2f),
		borderStronger = hsl(0f, 0f, 27.1f),
		borderMuted = hsl(0f, 0f, 14.1f),
		borderControl = hsl(0f, 0f, 22.4f),
		foregroundDefault = hsl(0f, 0f, 98f),
		foregroundLight = hsl(0f, 0f, 70.6f),
		foregroundLighter = hsl(0f, 0f, 53.7f),
		foregroundMuted = hsl(0f, 0f, 30.2f),
		foregroundContrast = hsl(0f, 0f, 8.6f),
		brandLink = hsl(270f, 95f, 75f),
		brandButton = hsl(271f, 81f, 56f),
		brandDefault = hsl(270f, 95f, 75f),
		brand600 = hsl(271f, 91f, 65f),
		brand500 = hsl(271f, 81f, 56f),
		brand400 = hsl(273f, 67f, 39f),
		brand300 = hsl(274f, 66f, 32f),
		brand200 = hsl(274f, 87f, 21f),
		warningDefault = hsl(38.9f, 100f, 42.9f),
		warning600 = hsl(38.9f, 100f, 42.9f),
		warning500 = hsl(34.8f, 90.9f, 21.6f),
		warning400 = hsl(33.2f, 100f, 14.5f),
		warning300 = hsl(32.3f, 100f, 10.2f),
		warning200 = hsl(36.6f, 100f, 8f),
		destructiveDefault = hsl(10.2f, 77.9f, 53.9f),
		destructive600 = hsl(9.7f, 85.2f, 62.9f),
		destructive500 = hsl(7.9f, 71.6f, 29f),
		destructive400 = hsl(6.7f, 60f, 20.6f),
		destructive300 = hsl(7.5f, 51.3f, 15.3f),
		destructive200 = hsl(10.9f, 23.4f, 9.2f),
	)
}

@Composable
fun VniDropTheme(
	mode: ThemeMode,
	useDynamicColors: Boolean = false,
	content: @Composable () -> Unit,
) {
	VniDropTheme(isDarkTheme = rememberResolvedDarkTheme(mode), useDynamicColors = useDynamicColors, content = content)
}

@Composable
fun VniDropTheme(
	isDarkTheme: Boolean,
	useDynamicColors: Boolean = false,
	content: @Composable () -> Unit,
) {
	val base = if (isDarkTheme) VniDropThemeTokens.dark else VniDropThemeTokens.light
	val scheme = if (LocalUiPlatform.current == UiPlatform.Android) {
		platformColorScheme(isDarkTheme, useDynamicColors, androidColorScheme(isDarkTheme))
	} else {
		base.toMaterialColorScheme(isDarkTheme)
	}
	val tokens = if (LocalUiPlatform.current == UiPlatform.Android) base.withMaterialColors(scheme) else base
	androidx.compose.runtime.CompositionLocalProvider(LocalVniDropColors provides tokens) {
		MaterialTheme(
			colorScheme = scheme,
			content = content,
		)
	}
}

private fun VniDropColors.toMaterialColorScheme(isDark: Boolean): ColorScheme {
	val base = if (isDark) darkColorScheme() else lightColorScheme()
	return base.copy(
		primary = brandDefault,
		onPrimary = if (isDark) Color.Black else Color.White,
		secondary = brandLink,
		background = backgroundDashCanvas,
		onBackground = foregroundDefault,
		surface = backgroundSurface75,
		onSurface = foregroundDefault,
		surfaceVariant = backgroundSurface200,
		onSurfaceVariant = foregroundLight,
		outline = borderDefault,
		outlineVariant = borderMuted,
		error = destructiveDefault,
		errorContainer = destructive200,
		onErrorContainer = if (isDark) destructive600 else destructiveDefault,
	)
}

fun hslColorForTest(hue: Float, saturation: Float, lightness: Float): Color =
	hsl(hue, saturation, lightness)

private fun hsl(hue: Float, saturation: Float, lightness: Float): Color {
	val h = ((hue % 360f) + 360f) % 360f / 360f
	val s = saturation.coerceIn(0f, 100f) / 100f
	val l = lightness.coerceIn(0f, 100f) / 100f
	if (s == 0f) return Color(l, l, l)
	val q = if (l < 0.5f) l * (1 + s) else l + s - l * s
	val p = 2 * l - q
	return Color(
		red = hueToRgb(p, q, h + 1f / 3f),
		green = hueToRgb(p, q, h),
		blue = hueToRgb(p, q, h - 1f / 3f),
	)
}

private fun hueToRgb(p: Float, q: Float, input: Float): Float {
	var t = input
	if (t < 0f) t += 1f
	if (t > 1f) t -= 1f
	return when {
		t < 1f / 6f -> p + (q - p) * 6f * t
		t < 1f / 2f -> q
		t < 2f / 3f -> p + (q - p) * (2f / 3f - t) * 6f
		else -> p
	}.let { min(1f, max(0f, it)) }
}
