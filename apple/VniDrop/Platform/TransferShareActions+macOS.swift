#if os(macOS)
import AppKit
import SwiftUI

@MainActor
func makePlatformShareActions() -> TransferShareActions { MacTransferShareActions() }

/// macOS invitation delivery, mirroring the iOS actions: save panel export and
/// `NSSharingServicePicker` native share. NFC is unavailable on macOS.
final class MacTransferShareActions: TransferShareActions {
	var canUseNativeShare: Bool { true }
	var nfcAvailability: NfcShareAvailability { .hidden }

	func exportInvitation(ticket: String, transferName: String, onResult: @escaping (Result<Void, Error>) -> Void) {
		let panel = NSSavePanel()
		panel.nameFieldStringValue = invitationFileName(transferName)
		panel.allowedContentTypes = []
		panel.begin { response in
			guard response == .OK, let url = panel.url else {
				onResult(.failure(InvitationError.cancelled))
				return
			}
			onResult(Result { try ticket.write(to: url, atomically: true, encoding: .utf8) })
		}
	}

	func shareInvitation(ticket: String, transferName: String, onResult: @escaping (Result<Void, Error>) -> Void) {
		do {
			let url = try writeTemporaryInvitation(ticket: ticket, transferName: transferName)
			guard let view = NSApp.keyWindow?.contentView else {
				onResult(.failure(InvitationError.noWindowAvailable))
				return
			}
			let picker = NSSharingServicePicker(items: [url])
			picker.show(relativeTo: .zero, of: view, preferredEdge: .minY)
			onResult(.success(()))
		} catch {
			onResult(.failure(error))
		}
	}

	func writeInvitationToNfc(ticket: String, onResult: @escaping (Result<Void, Error>) -> Void) {
		onResult(.failure(InvitationError.nfcUnavailable))
	}

	func cancelNfcWrite() {}
}
#endif
