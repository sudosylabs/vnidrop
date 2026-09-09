#if os(iOS)
import UIKit

@MainActor
func makePlatformShareActions() -> TransferShareActions { IosTransferShareActions() }

/// iOS invitation delivery: export via
/// document picker and native share via `UIActivityViewController`.
final class IosTransferShareActions: NSObject, TransferShareActions {
	var canUseNativeShare: Bool { true }

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

	@MainActor
	private func present(_ controller: UIViewController) throws {
		guard let presenter = topPresenter() else {
			throw InvitationError.viewControllerUnavailable
		}
		presenter.present(controller, animated: true)
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
