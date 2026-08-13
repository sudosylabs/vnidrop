import SwiftUI
import SFSafeSymbols

/// Saved devices screen. A native `List` of saved devices and outstanding consent
/// requests; selecting one opens the per-device details surface. The global
/// targeted-transfer history is deliberately absent — transfers belong to a
/// device, and are reachable only through it.
struct SavedDevicesScreen: View {
	@ObservedObject var model: SavedDevicesModel
	let windowClass: WindowClass

	/// Endpoint ID of the device whose details are open.
	@State private var selectedPeerId: String?

	private var state: SavedDevicesState { model.state }

	var body: some View {
		NavigationStack {
			Group {
				if state.isLoading && state.isEmpty {
					loadingState
				} else if state.loadFailed && state.isEmpty {
					failedState
				} else if state.isEmpty {
					emptyState
				} else {
					deviceList
				}
			}
			// Title-only header once populated: explanatory copy belongs in the
			// first-use empty state, not above a list the user already understands.
			.navigationTitle(Text(String(localized: L10n.Saved.devicesListTitle)))
		}
		// The consent prompts are hosted at the app root, not here: they must be
		// answerable from any tab, not only while this screen is showing.
		.savedDeviceDetails(model: model, windowClass: windowClass, selectedPeerId: $selectedPeerId)
	}

	// MARK: - List

	private var deviceList: some View {
		List {
			if !state.eligibilities.isEmpty || !state.pendingRelationships.isEmpty {
				Section {
					ForEach(state.pendingRelationships) { relationship in
						PendingRelationshipRow(
							relationship: relationship,
							busy: state.busyPeerIds.contains(relationship.remoteEndpointId),
							onAccept: { model.acceptIncoming(relationship.remoteEndpointId) },
							onDecline: { model.declineIncoming(relationship.remoteEndpointId) }
						)
					}
					ForEach(pendingEligibilities) { eligibility in
						EligibilityRow(
							eligibility: eligibility,
							busy: state.busyPeerIds.contains(eligibility.peerEndpointId),
							onRemember: { model.rememberEligible(eligibility.peerEndpointId) },
							onDecline: { model.declineEligible(eligibility.peerEndpointId) }
						)
					}
				} header: {
					Text(String(localized: L10n.Saved.devicesAttentionTitle))
				}
			}

			if !state.savedDevices.isEmpty {
				Section {
					ForEach(state.savedDevices) { device in
						Button { selectedPeerId = device.endpointId } label: {
							SavedDeviceRow(
								device: device,
								transferCount: state.transfers(for: device.endpointId).count,
								busy: state.busyPeerIds.contains(device.endpointId)
							)
						}
						.buttonStyle(.plain)
						.contextMenu { deviceMenu(device) }
					}
				} header: {
					Text(String(localized: L10n.Saved.devicesPendingTitle))
				}
			}
		}
	}

	/// Eligibilities whose peer already has a pending relationship are represented
	/// by that relationship row instead, so the same device never appears twice.
	private var pendingEligibilities: [PairingEligibilityModel] {
		let pendingIds = Set(state.pendingRelationships.map(\.remoteEndpointId))
		return state.eligibilities.filter { !pendingIds.contains($0.peerEndpointId) }
	}

	@ViewBuilder
	private func deviceMenu(_ device: SavedDeviceModel) -> some View {
		Button {
			selectedPeerId = device.endpointId
		} label: {
			Label(String(localized: L10n.Saved.devicesSendAction), systemSymbol: .paperplane)
		}
		Button {
			selectedPeerId = device.endpointId
			model.openLabelEditor(device.endpointId)
		} label: {
			Label(String(localized: L10n.Saved.devicesLabelAction), systemSymbol: .pencil)
		}
	}

	// MARK: - Placeholder states

	private var loadingState: some View {
		VStack(spacing: 12) {
			ProgressView()
			Text(String(localized: L10n.Saved.devicesLoading))
				.font(.subheadline)
				.foregroundStyle(.secondary)
		}
		.frame(maxWidth: .infinity, maxHeight: .infinity)
	}

	private var failedState: some View {
		ContentUnavailableView {
			Label(String(localized: L10n.Saved.devicesLoadFailed), systemSymbol: .exclamationmarkTriangleFill)
		} actions: {
			Button(String(localized: L10n.Button.retry), action: model.retry)
				.buttonStyle(.borderedProminent)
		}
	}

	/// First-use state: the only place that explains what a saved device is.
	private var emptyState: some View {
		ContentUnavailableView {
			Label(String(localized: L10n.Saved.devicesEmptyTitle), systemSymbol: .laptopcomputerAndIphone)
		} description: {
			Text(String(localized: L10n.Saved.devicesEmpty))
		}
	}
}

// MARK: - Rows

private struct SavedDeviceRow: View {
	let device: SavedDeviceModel
	let transferCount: Int
	let busy: Bool

	var body: some View {
		HStack(spacing: 12) {
			DeviceAvatar()
			VStack(alignment: .leading, spacing: 3) {
				Text(device.displayName)
					.font(.body)
					.lineLimit(1)
					.truncationMode(.tail)
				EndpointIdLabel(endpointId: device.endpointId)
			}
			Spacer(minLength: 8)
			if busy {
				ProgressView().controlSize(.small)
			} else if transferCount > 0 {
				Text(verbatim: "\(transferCount)")
					.font(.caption)
					.foregroundStyle(.secondary)
					.monospacedDigit()
			}
			Image(systemSymbol: .chevronRight)
				.font(.caption)
				.foregroundStyle(.tertiary)
		}
		.contentShape(Rectangle())
	}
}

/// A relationship awaiting consent. Incoming requests get accept/decline; an
/// outgoing request is informational until the peer answers.
private struct PendingRelationshipRow: View {
	let relationship: DeviceRelationshipModel
	let busy: Bool
	let onAccept: () -> Void
	let onDecline: () -> Void

	private var isIncoming: Bool { relationship.state == .pendingIncoming }

	var body: some View {
		VStack(alignment: .leading, spacing: 10) {
			HStack(spacing: 12) {
				DeviceAvatar(symbol: .personBadgeClock, tint: .orange)
				VStack(alignment: .leading, spacing: 3) {
					Text(String(localized: isIncoming
						? L10n.Saved.devicesPendingIncoming
						: L10n.Saved.devicesPendingOutgoing))
						.font(.body)
						.lineLimit(2)
					EndpointIdLabel(endpointId: relationship.remoteEndpointId)
				}
				Spacer(minLength: 0)
				if busy { ProgressView().controlSize(.small) }
			}
			if isIncoming {
				HStack(spacing: 10) {
					Button(String(localized: L10n.Saved.devicesAcceptPairingAction), action: onAccept)
						.buttonStyle(.borderedProminent)
					Button(String(localized: L10n.Saved.devicesDeclineAction), role: .destructive, action: onDecline)
						.buttonStyle(.bordered)
				}
				.controlSize(.small)
				.disabled(busy)
			}
		}
		.padding(.vertical, 4)
	}
}

/// A peer we completed a transfer with and may ask to pair. Naming it uses the
/// peer's untrusted hint — the only name available before the device is saved.
private struct EligibilityRow: View {
	let eligibility: PairingEligibilityModel
	let busy: Bool
	let onRemember: () -> Void
	let onDecline: () -> Void

	var body: some View {
		VStack(alignment: .leading, spacing: 10) {
			HStack(spacing: 12) {
				DeviceAvatar(symbol: .checkmarkSealFill, tint: VniDropColors.brandPurple)
				VStack(alignment: .leading, spacing: 3) {
					Text(eligibility.remoteDisplayName ?? String(localized: L10n.Saved.devicesUnnamed))
						.font(.body)
						.lineLimit(1)
					Text(String(localized: L10n.Saved.devicesEligibilityTitle))
						.font(.caption)
						.foregroundStyle(.secondary)
					EndpointIdLabel(endpointId: eligibility.peerEndpointId)
				}
				Spacer(minLength: 0)
				if busy { ProgressView().controlSize(.small) }
			}
			HStack(spacing: 10) {
				Button(String(localized: L10n.Saved.devicesRememberAction), action: onRemember)
					.buttonStyle(.borderedProminent)
				Button(String(localized: L10n.Saved.devicesDeclineAction), role: .destructive, action: onDecline)
					.buttonStyle(.bordered)
			}
			.controlSize(.small)
			.disabled(busy)
		}
		.padding(.vertical, 4)
	}
}
