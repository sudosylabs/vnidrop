#if os(macOS)
import AppKit
import UniformTypeIdentifiers

@MainActor
func makeReceiveInvitationActions() -> ReceiveInvitationActions { MacReceiveInvitationActions() }

/// macOS invitation acquisition: file picker only. QR (camera) and NFC are hidden
/// on the desktop, matching the availability model.
final class MacReceiveInvitationActions: ReceiveInvitationActions {
	var fileAvailability: ReceiveMethodAvailability { .available }
	var qrAvailability: ReceiveMethodAvailability { .hidden }
	var nfcAvailability: ReceiveMethodAvailability { .hidden }

	func pickInvitation(onResult: @escaping (Result<String, Error>) -> Void) {
		let panel = NSOpenPanel()
		panel.canChooseFiles = true
		panel.canChooseDirectories = false
		panel.allowsMultipleSelection = false
		if let vnd = UTType(filenameExtension: vniDropInvitationExtension) {
			panel.allowedContentTypes = [vnd, .data, .text]
		}
		panel.begin { response in
			guard response == .OK, let url = panel.url else {
				onResult(.failure(InvitationError.cancelled))
				return
			}
			onResult(Result {
				let data = try Data(contentsOf: url)
				return try decodeInvitationBytes(data)
			})
		}
	}

	func scanQrCode(onResult: @escaping (Result<String, Error>) -> Void) {
		onResult(.failure(InvitationError.qrUnavailable))
	}

	func readNfcInvitation(onResult: @escaping (Result<String, Error>) -> Void) {
		onResult(.failure(InvitationError.nfcUnavailable))
	}

	func cancel() {}
}
#endif
