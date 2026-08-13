import SwiftUI
import SFSafeSymbols

/// Shared presentation helpers for the saved-device surfaces.

extension SavedDeviceModel {
	/// `localLabel`, else the peer's untrusted display-name hint, else a generic
	/// placeholder. Never falls back to the endpoint ID, which stays diagnostic.
	var displayName: String {
		displayNameOrNil ?? String(localized: L10n.Saved.devicesUnnamed)
	}
}

extension TargetedTransferStateModel {
	var label: String {
		switch self {
		case .preparing: return String(localized: L10n.Status.preparing)
		case .offering: return String(localized: L10n.Status.offering)
		case .awaitingApproval: return String(localized: L10n.Status.awaitingApproval)
		case .approved: return String(localized: L10n.Status.approved)
		case .connecting: return String(localized: L10n.Status.connecting)
		case .transferring: return String(localized: L10n.Status.transferring)
		case .interrupted: return String(localized: L10n.Status.interrupted)
		case .completed: return String(localized: L10n.Status.completed)
		case .declined: return String(localized: L10n.Status.declined)
		case .cancelled: return String(localized: L10n.Status.cancelled)
		case .failed: return String(localized: L10n.Status.failed)
		// Deleted transfers are filtered out of the snapshot; label it defensively
		// rather than crashing if one ever reaches the UI.
		case .deleted: return String(localized: L10n.Status.cancelled)
		}
	}

	var tone: PillTone {
		switch self {
		case .completed: return .success
		case .failed, .declined: return .destructive
		case .interrupted: return .warning
		case .transferring, .connecting, .approved: return .brand
		default: return .neutral
		}
	}
}

/// A device identity avatar. Deliberately generic: the core exposes no trustworthy
/// hardware type, so showing a specific device silhouette would imply knowledge
/// VniDrop does not have.
struct DeviceAvatar: View {
	var symbol: SFSymbol = .laptopcomputerAndIphone
	var tint: Color = .secondary
	var size: CGFloat = 40

	var body: some View {
		Image(systemSymbol: symbol)
			.font(.system(size: size * 0.45))
			.foregroundStyle(tint)
			.frame(width: size, height: size)
			.background(.quaternary, in: RoundedRectangle(cornerRadius: size * 0.225))
	}
}

/// The endpoint ID, shown as secondary diagnostic detail. Truncated in the middle
/// so both ends stay recognizable, and never used as a device's name.
struct EndpointIdLabel: View {
	let endpointId: String

	var body: some View {
		Text(L10n.Saved.devicesEndpoint(deviceId: endpointId))
			.font(.caption2)
			.foregroundStyle(.tertiary)
			.lineLimit(1)
			.truncationMode(.middle)
			.textSelection(.enabled)
	}
}
