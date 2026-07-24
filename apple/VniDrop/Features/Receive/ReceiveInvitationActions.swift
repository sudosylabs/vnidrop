import SFSafeSymbols
import SwiftUI

enum ReceiveMethodAvailability { case available, unavailable, hidden }

/// Invitation acquisition actions, ported from `ReceiveInvitationActions` (iosMain).
@MainActor
protocol ReceiveInvitationActions: AnyObject {
	var fileAvailability: ReceiveMethodAvailability { get }
	var qrAvailability: ReceiveMethodAvailability { get }
	var nfcAvailability: ReceiveMethodAvailability { get }

	func pickInvitation(onResult: @escaping (Result<String, Error>) -> Void)
	func scanQrCode(onResult: @escaping (Result<String, Error>) -> Void)
	func readNfcInvitation(onResult: @escaping (Result<String, Error>) -> Void)
	func cancel()
}

/// Method chooser panel, ported from `ReceiveMethodPanel` in `ReceiveScreen.kt`.
struct ReceiveMethodPanel: View {
	@Environment(\.vniColors) private var colors
	@ObservedObject var model: ReceiveModel
	@State private var actions: ReceiveInvitationActions = makeReceiveInvitationActions()

	var body: some View {
		VStack(alignment: .leading, spacing: 12) {
			Text(String(localized: L10n.Receive.chooseMethodTitle)).font(VniType.titleLarge)
			Text(String(localized: L10n.Receive.chooseMethodBody)).foregroundStyle(colors.foregroundLighter)

			MethodRow(
				icon: .doc, titleKey: L10n.Receive.methodFile, descKey: L10n.Receive.methodFileDescription,
				availability: actions.fileAvailability
			) { actions.pickInvitation { model.onInvitationResult(.invitationFile, $0) } }

			if actions.qrAvailability != .hidden {
				MethodRow(
					icon: .qrcodeViewfinder, titleKey: L10n.Receive.methodScan, descKey: L10n.Receive.methodScanDescription,
					availability: actions.qrAvailability
				) { actions.scanQrCode { model.onInvitationResult(.qrCode, $0) } }
			}
			if actions.nfcAvailability != .hidden {
				MethodRow(
					icon: .wave3Right,
					titleOverride: model.state.isWaitingForNfc ? String(localized: L10n.Receive.nfcWaiting) : nil,
					titleKey: L10n.Receive.methodNfc, descKey: L10n.Receive.methodNfcDescription,
					availability: model.state.isWaitingForNfc ? .unavailable : actions.nfcAvailability
				) {
					model.setWaitingForNfc(true)
					actions.readNfcInvitation { model.onInvitationResult(.nfc, $0) }
				}
			}
		}
		.padding(.horizontal, 20).padding(.vertical, 14)
		.frame(maxWidth: .infinity, alignment: .leading)
	}
}

private struct MethodRow: View {
	@Environment(\.vniColors) private var colors
	let icon: SFSymbol
	var titleOverride: String? = nil
	let titleKey: String.LocalizationValue
	let descKey: String.LocalizationValue
	let availability: ReceiveMethodAvailability
	let onTap: () -> Void

	var body: some View {
		let enabled = availability == .available
		Button(action: onTap) {
			HStack(spacing: 14) {
				Image(systemSymbol: icon).font(.system(size: 22))
					.foregroundStyle(enabled ? colors.brandLink : colors.foregroundLighter)
					.frame(width: 24)
				VStack(alignment: .leading, spacing: 3) {
					if let titleOverride {
						Text(titleOverride).font(VniType.bodyLarge)
					} else {
						Text(String(localized: titleKey)).font(VniType.bodyLarge)
					}
					Text(String(localized: descKey)).font(VniType.bodySmall).foregroundStyle(colors.foregroundLighter)
				}
				Spacer()
				if availability == .unavailable {
					Text(String(localized: L10n.Value.unavailable)).font(VniType.labelSmall).foregroundStyle(colors.foregroundLighter)
				}
			}
			.padding(16)
			.frame(maxWidth: .infinity)
			.background(colors.backgroundSurface200, in: RoundedRectangle(cornerRadius: 14))
			.contentShape(Rectangle())
		}
		.buttonStyle(.plain)
		.disabled(!enabled)
	}
}

/// Invitation review panel, ported from `InvitationReviewPanel` in `ReceiveScreen.kt`.
struct InvitationReviewPanel: View {
	@Environment(\.vniColors) private var colors
	@ObservedObject var model: ReceiveModel

	private var state: ReceiveState { model.state }

	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			Text(String(localized: L10n.Receive.reviewTitle)).font(VniType.titleLarge)
			if state.isInspecting {
				ProgressView().frame(maxWidth: .infinity).padding(40)
			}
			if let inspection = state.inspection {
				let metadata = inspection.metadata
				VStack(alignment: .leading, spacing: 8) {
					Text(metadata.transferName).font(VniType.bodyLarge).lineLimit(2)
					Text(L10n.Format.separatedTriple(
						first: "\(metadata.fileCount)",
						second: String(localized: L10n.Metadata.files).lowercased(),
						third: formatBytes(metadata.totalSize)))
						.foregroundStyle(colors.foregroundLighter)
				}
				.padding(16)
				.frame(maxWidth: .infinity, alignment: .leading)
				.background(colors.backgroundSurface200, in: RoundedRectangle(cornerRadius: 14))

				Field(label: String(localized: L10n.Field.receiverName),
					  value: Binding(get: { state.receiverName }, set: { model.setReceiverName($0) }))
				Text(state.receiveFolder?.displayName ?? String(localized: L10n.Value.unavailable))
					.font(VniType.bodySmall)
					.foregroundStyle(state.folderAccessStatus == .writable ? colors.foregroundLight : colors.destructiveDefault)

				if state.isReceiving {
					let progressId = state.activeReceiveTransferId
						?? model.coreState.events.first { $0.eventDirection == .receive && $0.transferId != nil }?.transferId
					let progress = progressId.flatMap { progressForTransfer(events: model.coreState.events, transferId: $0) }
					ProgressRow(labelKey: progress?.labelKey ?? L10n.Progress.receiving, progress: progress?.progress, detail: progress?.detail)
					SecondaryButton(title: String(localized: L10n.Button.cancelReceive), action: model.cancelActiveReceive)
				} else {
					PrimaryButton(
						title: String(localized: L10n.Button.receive), action: model.receive,
						enabled: state.canReceive(coreInitialized: model.coreState.isInitialized)
					)
				}
				if let error = state.lastReceiveError {
					Text(error.resolved()).font(VniType.bodySmall).foregroundStyle(colors.destructiveDefault)
				}
			}
		}
		.padding(.horizontal, 20).padding(.vertical, 14)
		.frame(maxWidth: .infinity, alignment: .leading)
	}
}
