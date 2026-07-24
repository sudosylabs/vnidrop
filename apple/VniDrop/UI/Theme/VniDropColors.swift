import SwiftUI

/// Direct Compose port of the VniDrop semantic color tokens
/// (`shared/.../ui/theme/VniDropTheme.kt`). The app uses these semantic tokens
/// directly because a single SwiftUI/Material color scheme cannot represent the
/// full surface, border, and foreground stack.
struct VniDropColors {
	let backgroundDefault: Color
	let backgroundDashCanvas: Color
	let backgroundDashSidebar: Color
	let backgroundSurface75: Color
	let backgroundSurface100: Color
	let backgroundSurface200: Color
	let backgroundSurface300: Color
	let backgroundSurface400: Color
	let backgroundMuted: Color
	let backgroundControl: Color
	let backgroundSelection: Color
	let backgroundButton: Color
	let backgroundOverlayHover: Color
	let backgroundDialog: Color
	let borderDefault: Color
	let borderStrong: Color
	let borderStronger: Color
	let borderMuted: Color
	let borderControl: Color
	let foregroundDefault: Color
	let foregroundLight: Color
	let foregroundLighter: Color
	let foregroundMuted: Color
	let foregroundContrast: Color
	let brandLink: Color
	let brandButton: Color
	let brandDefault: Color
	let brand600: Color
	let brand500: Color
	let brand400: Color
	let brand300: Color
	let brand200: Color
	let warningDefault: Color
	let warning200: Color
	let warning300: Color
	let warning400: Color
	let warning500: Color
	let warning600: Color
	let destructiveDefault: Color
	let destructive200: Color
	let destructive300: Color
	let destructive400: Color
	let destructive500: Color
	let destructive600: Color
}

extension VniDropColors {
	/// The single brand accent used app-wide as the SwiftUI tint. Mirrored by the
	/// `AccentColor` asset (the OS-level global accent for the macOS sidebar etc.);
	/// keep the two in sync.
	static let brandPurple = Color.hsl(271, 91, 65)

	static let light = VniDropColors(
		backgroundDefault: .hsl(0, 0, 98.8),
		backgroundDashCanvas: .hsl(0, 0, 97.3),
		backgroundDashSidebar: .hsl(0, 0, 98.8),
		backgroundSurface75: .hsl(0, 0, 100),
		backgroundSurface100: .hsl(0, 0, 98.8),
		backgroundSurface200: .hsl(0, 0, 95.3),
		backgroundSurface300: .hsl(0, 0, 92.9),
		backgroundSurface400: .hsl(0, 0, 89.8),
		backgroundMuted: .hsl(0, 0, 96.9),
		backgroundControl: .hsl(0, 0, 95.3),
		backgroundSelection: .hsl(0, 0, 92.9),
		backgroundButton: .hsl(0, 0, 91),
		backgroundOverlayHover: .hsl(0, 0, 95.3),
		backgroundDialog: .hsl(0, 0, 100),
		borderDefault: .hsl(0, 0, 87.5),
		borderStrong: .hsl(0, 0, 83.1),
		borderStronger: .hsl(0, 0, 56.1),
		borderMuted: .hsl(0, 0, 92.9),
		borderControl: .hsl(0, 0, 78),
		foregroundDefault: .hsl(0, 0, 9),
		foregroundLight: .hsl(0, 0, 32.2),
		foregroundLighter: .hsl(0, 0, 43.9),
		foregroundMuted: .hsl(0, 0, 69.8),
		foregroundContrast: .hsl(0, 0, 98.4),
		brandLink: .hsl(271, 91, 65),
		brandButton: .hsl(270, 95, 75),
		brandDefault: .hsl(271, 91, 65),
		brand600: .hsl(271, 81, 56),
		brand500: .hsl(271, 91, 65),
		brand400: .hsl(270, 95, 75),
		brand300: .hsl(269, 97, 85),
		brand200: .hsl(269, 100, 92),
		warningDefault: .hsl(38.9, 100, 57.1),
		warning200: .hsl(40, 81.8, 97.8),
		warning300: .hsl(44.3, 100, 91.8),
		warning400: .hsl(41.9, 100, 81.8),
		warning500: .hsl(36.3, 85.7, 67.1),
		warning600: .hsl(30.3, 80.3, 47.8),
		destructiveDefault: .hsl(10.2, 77.9, 53.9),
		destructive200: .hsl(0, 100, 99.4),
		destructive300: .hsl(7.1, 100, 96.7),
		destructive400: .hsl(7.1, 91.3, 91),
		destructive500: .hsl(10.4, 77.1, 79.4),
		destructive600: .hsl(9.9, 82, 43.5)
	)

	static let dark = VniDropColors(
		backgroundDefault: .hsl(0, 0, 7.1),
		backgroundDashCanvas: .hsl(0, 0, 7.1),
		backgroundDashSidebar: .hsl(0, 0, 9),
		backgroundSurface75: .hsl(0, 0, 9),
		backgroundSurface100: .hsl(0, 0, 12.2),
		backgroundSurface200: .hsl(0, 0, 12.9),
		backgroundSurface300: .hsl(0, 0, 16.1),
		backgroundSurface400: .hsl(0, 0, 16.1),
		backgroundMuted: .hsl(0, 0, 14.1),
		backgroundControl: .hsl(0, 0, 14.1),
		backgroundSelection: .hsl(0, 0, 19.2),
		backgroundButton: .hsl(0, 0, 18),
		backgroundOverlayHover: .hsl(0, 0, 18),
		backgroundDialog: .hsl(0, 0, 7.1),
		borderDefault: .hsl(0, 0, 18),
		borderStrong: .hsl(0, 0, 21.2),
		borderStronger: .hsl(0, 0, 27.1),
		borderMuted: .hsl(0, 0, 14.1),
		borderControl: .hsl(0, 0, 22.4),
		foregroundDefault: .hsl(0, 0, 98),
		foregroundLight: .hsl(0, 0, 70.6),
		foregroundLighter: .hsl(0, 0, 53.7),
		foregroundMuted: .hsl(0, 0, 30.2),
		foregroundContrast: .hsl(0, 0, 8.6),
		brandLink: .hsl(270, 95, 75),
		brandButton: .hsl(271, 81, 56),
		brandDefault: .hsl(270, 95, 75),
		brand600: .hsl(271, 91, 65),
		brand500: .hsl(271, 81, 56),
		brand400: .hsl(273, 67, 39),
		brand300: .hsl(274, 66, 32),
		brand200: .hsl(274, 87, 21),
		warningDefault: .hsl(38.9, 100, 42.9),
		warning200: .hsl(36.6, 100, 8),
		warning300: .hsl(32.3, 100, 10.2),
		warning400: .hsl(33.2, 100, 14.5),
		warning500: .hsl(34.8, 90.9, 21.6),
		warning600: .hsl(38.9, 100, 42.9),
		destructiveDefault: .hsl(10.2, 77.9, 53.9),
		destructive200: .hsl(10.9, 23.4, 9.2),
		destructive300: .hsl(7.5, 51.3, 15.3),
		destructive400: .hsl(6.7, 60, 20.6),
		destructive500: .hsl(7.9, 71.6, 29),
		destructive600: .hsl(9.7, 85.2, 62.9)
	)
}

extension Color {
	/// HSL constructor matching the Compose `hsl()` helper (hue in degrees,
	/// saturation and lightness in percent).
	static func hsl(_ hue: Double, _ saturation: Double, _ lightness: Double) -> Color {
		let h = (hue.truncatingRemainder(dividingBy: 360) + 360)
			.truncatingRemainder(dividingBy: 360) / 360
		let s = min(max(saturation, 0), 100) / 100
		let l = min(max(lightness, 0), 100) / 100
		if s == 0 {
			return Color(red: l, green: l, blue: l)
		}
		let q = l < 0.5 ? l * (1 + s) : l + s - l * s
		let p = 2 * l - q
		return Color(
			red: hueToRgb(p, q, h + 1.0 / 3.0),
			green: hueToRgb(p, q, h),
			blue: hueToRgb(p, q, h - 1.0 / 3.0)
		)
	}
}

private func hueToRgb(_ p: Double, _ q: Double, _ input: Double) -> Double {
	var t = input
	if t < 0 { t += 1 }
	if t > 1 { t -= 1 }
	let value: Double
	if t < 1.0 / 6.0 {
		value = p + (q - p) * 6 * t
	} else if t < 1.0 / 2.0 {
		value = q
	} else if t < 2.0 / 3.0 {
		value = p + (q - p) * (2.0 / 3.0 - t) * 6
	} else {
		value = p
	}
	return min(1, max(0, value))
}
