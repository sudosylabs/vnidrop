#if os(iOS)
import UIKit
@preconcurrency import CoreNFC

@MainActor
func makePlatformShareActions() -> TransferShareActions { IosTransferShareActions() }

/// iOS invitation delivery: export via
/// document picker, native share via `UIActivityViewController`, and NFC write.
final class IosTransferShareActions: NSObject, TransferShareActions {
	private var nfcWriter: InvitationNfcWriter?

	var canUseNativeShare: Bool { true }
	var nfcAvailability: NfcShareAvailability {
		NFCNDEFReaderSession.readingAvailable ? .available : .unavailable
	}

	func exportInvitation(ticket: String, transferName: String, onResult: @escaping (Result<Void, Error>) -> Void) {
		onResult(Result {
			let url = try writeTemporaryInvitation(ticket: ticket, transferName: transferName)
			let picker = UIDocumentPickerViewController(forExporting: [url], asCopy: true)
			picker.modalPresentationStyle = .formSheet
			try present(picker)
		})
	}

	func shareInvitation(ticket: String, transferName: String, onResult: @escaping (Result<Void, Error>) -> Void) {
		onResult(Result {
			let url = try writeTemporaryInvitation(ticket: ticket, transferName: transferName)
			let controller = UIActivityViewController(activityItems: [url], applicationActivities: nil)
			controller.modalPresentationStyle = .formSheet
			try present(controller)
		})
	}

	func writeInvitationToNfc(ticket: String, onResult: @escaping (Result<Void, Error>) -> Void) {
		cancelNfcWrite()
		guard NFCNDEFReaderSession.readingAvailable else {
			onResult(.failure(InvitationError.nfcUnavailable))
			return
		}
		let writer = InvitationNfcWriter(ticket: ticket) { [weak self] result in
			self?.nfcWriter = nil
			onResult(result)
		}
		nfcWriter = writer
		writer.start()
	}

	func cancelNfcWrite() {
		nfcWriter?.cancel()
		nfcWriter = nil
	}

	@MainActor
	private func present(_ controller: UIViewController) throws {
		guard let presenter = topPresenter() else {
			throw InvitationError.viewControllerUnavailable
		}
		presenter.present(controller, animated: true)
	}
}

/// Writes a VniDrop invitation to a writable NDEF tag.
// Runs entirely on the NFC session's `.main` delegate queue.
final class InvitationNfcWriter: NSObject, NFCNDEFReaderSessionDelegate, @unchecked Sendable {
	private let ticket: String
	private let onResult: (Result<Void, Error>) -> Void
	private var session: NFCNDEFReaderSession?
	private var finished = false

	init(ticket: String, onResult: @escaping (Result<Void, Error>) -> Void) {
		self.ticket = ticket
		self.onResult = onResult
	}

	func start() {
		let reader = NFCNDEFReaderSession(delegate: self, queue: .main, invalidateAfterFirstRead: false)
		reader.alertMessage = String(localized: L10n.Transfer.nfcWaiting)
		session = reader
		reader.begin()
	}

	func cancel() {
		session?.invalidate()
		session = nil
	}

	func readerSession(_ session: NFCNDEFReaderSession, didInvalidateWithError error: Error) {
		if finished { return }
		let cancelled = (error as NSError).code == 200 // readerSessionInvalidationErrorUserCanceled
		finish(.failure(cancelled ? InvitationError.cancelled : InvitationError.raw(error.localizedDescription)))
	}

	func readerSession(_ session: NFCNDEFReaderSession, didDetectNDEFs messages: [NFCNDEFMessage]) {}

	func readerSession(_ session: NFCNDEFReaderSession, didDetect tags: [NFCNDEFTag]) {
		guard let firstTag = tags.first else {
			return finish(.failure(InvitationError.nfcFailed))
		}
		// CoreNFC completion handlers run on the session's `.main` queue; these
		// framework values are safe to use there.
		nonisolated(unsafe) let session = session
		nonisolated(unsafe) let tag = firstTag
		session.connect(to: tag) { [weak self] connectError in
			guard let self else { return }
			if let connectError { return self.finish(.failure(connectError)) }
			tag.queryNDEFStatus { status, _, queryError in
				if let queryError { return self.finish(.failure(queryError)) }
				switch status {
				case .notSupported:
					self.finish(.failure(InvitationError.nfcFailed))
				case .readOnly:
					self.finish(.failure(InvitationError.nfcFailed))
				default:
					guard let message = self.invitationMessage() else {
						return self.finish(.failure(InvitationError.nfcFailed))
					}
					tag.writeNDEF(message) { writeError in
						if let writeError {
							self.finish(.failure(writeError))
						} else {
							session.alertMessage = String(localized: L10n.Transfer.nfcWritten)
							session.invalidate()
							self.finish(.success(()))
						}
					}
				}
			}
		}
	}

	private func invitationMessage() -> NFCNDEFMessage? {
		guard let type = vniDropInvitationMimeType.data(using: .utf8),
			  let payload = ticket.data(using: .utf8) else { return nil }
		let record = NFCNDEFPayload(format: .media, type: type, identifier: Data(), payload: payload)
		return NFCNDEFMessage(records: [record])
	}

	private func finish(_ result: Result<Void, Error>) {
		if finished { return }
		finished = true
		session = nil
		DispatchQueue.main.async { self.onResult(result) }
	}
}

@MainActor
func topPresenter() -> UIViewController? {
	let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
	let keyWindow = scenes.flatMap { $0.windows }.first { $0.isKeyWindow }
	var controller = keyWindow?.rootViewController
	while let presented = controller?.presentedViewController {
		controller = presented
	}
	return controller
}
#endif
