#if os(macOS)
import AppKit
import SwiftUI

@MainActor
func makePlatformShareActions() -> TransferShareActions { MacTransferShareActions() }

/// macOS invitation delivery, mirroring the iOS actions: save panel export and
/// `NSSharingServicePicker` native share.
final class MacTransferShareActions: TransferShareActions {
	var canUseNativeShare: Bool { true }

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
		guard let view = NSApp.keyWindow?.contentView else {
			onResult(.failure(InvitationError.noWindowAvailable))
			return
		}
		do {
			let staged = try TemporaryInvitation(ticket: ticket, transferName: transferName)
			InvitationShareSession.start(staged, anchor: view, onResult: onResult)
		} catch {
			onResult(.failure(error))
		}
	}
}

@MainActor
private final class InvitationShareSession: NSObject, @preconcurrency NSSharingServicePickerDelegate, NSSharingServiceDelegate {
	// Both native delegates are weak, and the transfer view can close mid-share.
	private static var active: [URL: InvitationShareSession] = [:]
	private let file: TemporaryInvitation
	private let picker: NSSharingServicePicker
	private let onResult: (Result<Void, Error>) -> Void

	private init(_ file: TemporaryInvitation, onResult: @escaping (Result<Void, Error>) -> Void) {
		self.file = file
		self.picker = NSSharingServicePicker(items: [file.url])
		self.onResult = onResult
		super.init()
		picker.delegate = self
	}

	static func start(_ file: TemporaryInvitation, anchor: NSView, onResult: @escaping (Result<Void, Error>) -> Void) {
		let session = InvitationShareSession(file, onResult: onResult)
		active[file.url] = session
		session.picker.show(relativeTo: .zero, of: anchor, preferredEdge: .minY)
	}

	func sharingServicePicker(_ sharingServicePicker: NSSharingServicePicker, didChoose service: NSSharingService?) {
		if service == nil { finish(.failure(InvitationError.cancelled)) }
	}

	func sharingServicePicker(_ sharingServicePicker: NSSharingServicePicker, delegateFor sharingService: NSSharingService) -> (any NSSharingServiceDelegate)? {
		self
	}

	func sharingService(_ sharingService: NSSharingService, didShareItems items: [Any]) {
		finish(.success(()))
	}

	func sharingService(_ sharingService: NSSharingService, didFailToShareItems items: [Any], error: any Error) {
		finish(.failure(error))
	}

	private func finish(_ result: Result<Void, Error>) {
		guard Self.active.removeValue(forKey: file.url) != nil else { return }
		file.remove()
		onResult(result)
	}
}
#endif
