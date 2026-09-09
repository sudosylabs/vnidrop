import SwiftUI

/// Invitation delivery actions, ported from `TransferShareActions` (iosMain).
/// Platform implementations perform export and native share.
@MainActor
protocol TransferShareActions: AnyObject {
	var canUseNativeShare: Bool { get }

	func exportInvitation(ticket: String, transferName: String, onResult: @escaping (Result<Void, Error>) -> Void)
	func shareInvitation(ticket: String, transferName: String, onResult: @escaping (Result<Void, Error>) -> Void)
}

/// The QR + delivery buttons for a transfer's share panel, ported from the button
/// stack in `TransferSharePanel` (`TransferDetails.kt`).
struct ShareActionsView: View {
	@ObservedObject var model: SendModel
	let transfer: Transfer
	let ticket: String

	@State private var actions: TransferShareActions = makePlatformShareActions()

	var body: some View {
		VStack(spacing: 12) {
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
	}
}
