import Foundation
import SFSafeSymbols

/// Top-level destinations, ported from `ui/navigation/AppDestination.kt`.
enum AppDestination: String, CaseIterable, Identifiable {
	case send
	case receive
	case settings

	var id: String { rawValue }

	var labelKey: String.LocalizationValue {
		switch self {
		case .send: return L10n.Nav.send
		case .receive: return L10n.Nav.receive
		case .settings: return L10n.Nav.settings
		}
	}

	/// SF Symbol approximating the Compose line icon.
	var systemSymbol: SFSymbol {
		switch self {
		case .send: return .paperplane
		case .receive: return .trayAndArrowDown
		case .settings: return .gearshape
		}
	}
}
