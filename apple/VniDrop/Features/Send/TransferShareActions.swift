import SwiftUI

enum NfcShareAvailability { case available, unavailable, hidden }

/// Invitation delivery actions, ported from `TransferShareActions` (iosMain).
/// Platform implementations perform export, native share, and NFC write.
@MainActor
protocol TransferShareActions: AnyObject {
	var canUseNativeShare: Bool { get }
	var nfcAvailability: NfcShareAvailability { get }

	func exportInvitation(ticket: String, transferName: String, onResult: @escaping (Result<Void, Error>) -> Void)
	func shareInvitation(ticket: String, transferName: String, onResult: @escaping (Result<Void, Error>) -> Void)
	func writeInvitationToNfc(ticket: String, onResult: @escaping (Result<Void, Error>) -> Void)
	func cancelNfcWrite()
}

/// The QR + delivery buttons for a transfer's share panel, ported from the button
/// stack in `TransferSharePanel` (`TransferDetails.kt`).
struct ShareActionsView: View {
	@Environment(\.vniColors) private var colors
	@ObservedObject var model: SendModel
	let transfer: Transfer
	let ticket: String

	@State private var actions: TransferShareActions = makePlatformShareActions()
	@State private var writingNfc = false

	var body: some View {
		VStack(spacing: 12) {
			if actions.nfcAvailability != .hidden {
				SecondaryButton(
					title: writingNfc ? String(localized: L10n.Transfer.nfcWaiting) : String(localized: L10n.Button.writeNfc),
					action: {
						writingNfc = true
						actions.writeInvitationToNfc(ticket: ticket) { result in
							writingNfc = false
							model.onInvitationResult(.nfc, result)
						}
					},
					enabled: actions.nfcAvailability == .available && !writingNfc
				)
				if actions.nfcAvailability == .unavailable {
					Text(String(localized: L10n.Transfer.nfcUnavailable))
						.font(VniType.bodySmall).foregroundStyle(colors.foregroundLighter)
				}
			}
			SecondaryButton(title: String(localized: L10n.Button.downloadInvitation), action: {
				actions.exportInvitation(ticket: ticket, transferName: transfer.transferName ?? "") {
					model.onInvitationResult(.export, $0)
				}
			})
			PrimaryButton(title: String(localized: L10n.Button.nativeShare), action: {
				actions.shareInvitation(ticket: ticket, transferName: transfer.transferName ?? "") {
					model.onInvitationResult(.share, $0)
				}
			}, enabled: actions.canUseNativeShare)
		}
		.onDisappear { actions.cancelNfcWrite() }
	}
}
