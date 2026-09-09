#if os(iOS)
import UIKit
@preconcurrency import AVFoundation
import UniformTypeIdentifiers

@MainActor
func makeReceiveInvitationActions() -> ReceiveInvitationActions { IosReceiveInvitationActions() }

/// iOS invitation acquisition:
/// document picker and camera QR scanner.
final class IosReceiveInvitationActions: NSObject, ReceiveInvitationActions, UIDocumentPickerDelegate {
	private var documentResult: ((Result<String, Error>) -> Void)?
	private var qrController: QrScannerViewController?

	var fileAvailability: ReceiveMethodAvailability { .available }
	var qrAvailability: ReceiveMethodAvailability {
		AVCaptureDevice.default(for: .video) != nil ? .available : .unavailable
	}

	func pickInvitation(onResult: @escaping (Result<String, Error>) -> Void) {
		cancel()
		documentResult = onResult
		let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.data], asCopy: true)
		picker.delegate = self
		picker.modalPresentationStyle = .formSheet
		guard let presenter = topPresenter() else {
			return onResult(.failure(InvitationError.viewControllerUnavailable))
		}
		presenter.present(picker, animated: true)
	}

	func scanQrCode(onResult: @escaping (Result<String, Error>) -> Void) {
		cancel()
		guard let presenter = topPresenter() else {
			return onResult(.failure(InvitationError.viewControllerUnavailable))
		}
		ensureCameraAccess { [weak self] granted in
			guard let self else { return }
			guard granted else {
				return onResult(.failure(InvitationError.cameraUnavailable))
			}
			let scanner = QrScannerViewController { result in
				self.qrController = nil
				onResult(result)
			}
			self.qrController = scanner
			scanner.modalPresentationStyle = .fullScreen
			presenter.present(scanner, animated: true)
		}
	}

	func cancel() {
		qrController?.cancelScan()
		qrController = nil
		documentResult = nil
	}

	// UIDocumentPickerDelegate
	func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
		let result = documentResult
		documentResult = nil
		result?(Result {
			guard let url = urls.first else { throw InvitationError.invalidInvitationURL }
			let started = url.startAccessingSecurityScopedResource()
			defer { if started { url.stopAccessingSecurityScopedResource() } }
			let data = try Data(contentsOf: url)
			guard data.count <= maxVniDropInvitationBytes else { throw InvitationError.tooLarge }
			return try decodeInvitationBytes(data)
		})
	}

	func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
		documentResult = nil
	}

	private func ensureCameraAccess(_ completion: @escaping (Bool) -> Void) {
		switch AVCaptureDevice.authorizationStatus(for: .video) {
		case .authorized:
			completion(true)
		case .notDetermined:
			// The permission callback is delivered back on the main queue.
			nonisolated(unsafe) let completion = completion
			AVCaptureDevice.requestAccess(for: .video) { granted in
				DispatchQueue.main.async { completion(granted) }
			}
		default:
			completion(false)
		}
	}
}

/// Full-screen camera QR scanner, ported from `QrScannerViewController`.
final class QrScannerViewController: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
	private let onResult: (Result<String, Error>) -> Void
	private let session = AVCaptureSession()
	private var previewLayer: AVCaptureVideoPreviewLayer?
	private var finished = false

	init(onResult: @escaping (Result<String, Error>) -> Void) {
		self.onResult = onResult
		super.init(nibName: nil, bundle: nil)
	}

	@available(*, unavailable)
	required init?(coder: NSCoder) { fatalError() }

	override func viewDidLoad() {
		super.viewDidLoad()
		view.backgroundColor = .black

		let hint = UILabel(frame: view.bounds)
		hint.text = "Point the camera at a VniDrop QR code"
		hint.textColor = .white
		hint.textAlignment = .center
		hint.numberOfLines = 0
		hint.autoresizingMask = [.flexibleWidth, .flexibleHeight]
		view.addSubview(hint)

		let close = UIButton(type: .system)
		close.setTitle("Cancel", for: .normal)
		close.setTitleColor(.white, for: .normal)
		close.frame = CGRect(x: 16, y: 52, width: 88, height: 36)
		close.addAction(UIAction { [weak self] _ in self?.cancelScan() }, for: .touchUpInside)
		view.addSubview(close)

		configureSession()
	}

	override func viewDidLayoutSubviews() {
		super.viewDidLayoutSubviews()
		previewLayer?.frame = view.bounds
	}

	override func viewWillDisappear(_ animated: Bool) {
		super.viewWillDisappear(animated)
		if session.isRunning { session.stopRunning() }
	}

	func cancelScan() {
		finish(.failure(InvitationError.cancelled))
	}

	private func configureSession() {
		guard let device = AVCaptureDevice.default(for: .video),
			  let input = try? AVCaptureDeviceInput(device: device),
			  session.canAddInput(input) else {
			return finish(.failure(InvitationError.cameraUnavailable))
		}
		session.addInput(input)
		let output = AVCaptureMetadataOutput()
		guard session.canAddOutput(output) else {
			return finish(.failure(InvitationError.cameraUnavailable))
		}
		session.addOutput(output)
		output.setMetadataObjectsDelegate(self, queue: .main)
		output.metadataObjectTypes = [.qr]

		let layer = AVCaptureVideoPreviewLayer(session: session)
		layer.videoGravity = .resizeAspectFill
		layer.frame = view.bounds
		view.layer.insertSublayer(layer, at: 0)
		previewLayer = layer
		session.sessionPreset = .high

		DispatchQueue.global(qos: .userInitiated).async { [session] in session.startRunning() }
	}

	// The metadata output delegate queue is `.main`, so hop back onto the main
	// actor to touch view-controller state.
	nonisolated func metadataOutput(_ output: AVCaptureMetadataOutput, didOutput metadataObjects: [AVMetadataObject], from connection: AVCaptureConnection) {
		let value = metadataObjects
			.compactMap { $0 as? AVMetadataMachineReadableCodeObject }
			.first { $0.type == .qr }?.stringValue?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
		guard !value.isEmpty else { return }
		MainActor.assumeIsolated { finish(.success(value)) }
	}

	private func finish(_ result: Result<String, Error>) {
		if finished { return }
		finished = true
		if session.isRunning { session.stopRunning() }
		if presentingViewController != nil {
			dismiss(animated: true) { self.onResult(result) }
		} else {
			onResult(result)
		}
	}
}

#endif
