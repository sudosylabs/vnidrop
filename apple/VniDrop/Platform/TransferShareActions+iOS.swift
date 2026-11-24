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
			let directory = try AppTemporaryDirectory.prepare()
				.appendingPathComponent("export-\(UUID().uuidString)", isDirectory: true)
			try FileManager.default.createDirectory(
				at: directory, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700]
			)
			do {
				let url = try writeTemporaryInvitation(ticket: ticket, transferName: transferName, directory: directory)
				let picker = InvitationExportPicker(file: url, onResult: onResult)
				try present(picker)
				picker.presentationController?.delegate = picker
			} catch {
				try? FileManager.default.removeItem(at: directory)
				throw error
			}
		} catch {
			onResult(.failure(error))
		}
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
	private let file: URL
	private var onResult: ((Result<Void, Error>) -> Void)?

	init(file: URL, onResult: @escaping (Result<Void, Error>) -> Void) {
		self.file = file
		self.onResult = onResult
		super.init(forExporting: [file], asCopy: true)
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
		try? FileManager.default.removeItem(at: file.deletingLastPathComponent())
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
