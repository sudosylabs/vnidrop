#if os(iOS)
import UIKit

@MainActor
func makePlatformShareActions() -> TransferShareActions { IosTransferShareActions() }

/// iOS invitation delivery: export via
/// document picker and native share via `UIActivityViewController`.
final class IosTransferShareActions: NSObject, TransferShareActions {
	private let present: @MainActor (UIViewController) throws -> Void

	init(present: @escaping @MainActor (UIViewController) throws -> Void = IosTransferShareActions.presentController) {
		self.present = present
	}

	var canUseNativeShare: Bool { true }

	func exportInvitation(ticket: String, transferName: String, onResult: @escaping (Result<Void, Error>) -> Void) {
		do {
			let file = try TemporaryInvitation(ticket: ticket, transferName: transferName)
			let picker = InvitationExportPicker(file: file, onResult: onResult)
			do {
				try present(picker)
				picker.presentationController?.delegate = picker
			} catch {
				file.remove()
				throw error
			}
		} catch {
			onResult(.failure(error))
		}
	}

	func shareInvitation(ticket: String, transferName: String, onResult: @escaping (Result<Void, Error>) -> Void) {
		do {
			let file = try TemporaryInvitation(ticket: ticket, transferName: transferName)
			let controller = UIActivityViewController(activityItems: [file.url], applicationActivities: nil)
			controller.modalPresentationStyle = .formSheet
			controller.completionWithItemsHandler = { _, completed, _, error in
				file.remove()
				if let error { onResult(.failure(error)) }
				else if completed { onResult(.success(())) }
				else { onResult(.failure(InvitationError.cancelled)) }
			}
			do {
				try present(controller)
			} catch {
				file.remove()
				throw error
			}
		} catch {
			onResult(.failure(error))
		}
	}

	@MainActor
	private static func presentController(_ controller: UIViewController) throws {
		guard let presenter = topPresenter() else {
			throw InvitationError.viewControllerUnavailable
		}
		presenter.present(controller, animated: true)
	}
}

// UIKit retains the presented controller while its delegates are weak.
private final class InvitationExportPicker: UIDocumentPickerViewController,
	UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate {
	private let file: TemporaryInvitation
	private var onResult: ((Result<Void, Error>) -> Void)?

	init(file: TemporaryInvitation, onResult: @escaping (Result<Void, Error>) -> Void) {
		self.file = file
		self.onResult = onResult
		super.init(forExporting: [file.url], asCopy: true)
		modalPresentationStyle = .formSheet
		delegate = self
	}

	required init?(coder: NSCoder) { nil }

	func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
		finish(urls.isEmpty ? .failure(InvitationError.filesystemUnavailable) : .success(()))
	}

	func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
		finish(.failure(InvitationError.cancelled))
	}

	func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
		finish(.failure(InvitationError.cancelled))
	}

	private func finish(_ result: Result<Void, Error>) {
		guard let onResult else { return }
		self.onResult = nil
		file.remove()
		onResult(result)
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
