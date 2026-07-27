import SwiftUI
import UniformTypeIdentifiers

/// Root-level pickers that are NOT triggered from inside a sheet. Currently just
/// the receive-folder picker (Settings is presented in the tab's navigation
/// stack, not a sheet, so presenting from the root works).
struct PlatformPickers: ViewModifier {
	@ObservedObject var settingsModel: SettingsModel

	func body(content: Content) -> some View {
		content
			.fileImporter(
				isPresented: Binding(get: { settingsModel.pendingReceiveFolderPick }, set: { settingsModel.pendingReceiveFolderPick = $0 }),
				allowedContentTypes: [.folder],
				allowsMultipleSelection: false
			) { result in
				switch result {
				case .success(let urls):
					guard let url = urls.first else { return }
					settingsModel.onReceiveFolderPicked(PickerSupport.receiveFolder(from: url))
				case .failure(let error):
					if !error.isUserCancellation { settingsModel.onReceiveFolderPickFailed(error.technicalDetail) }
				}
			}
	}
}

/// Send file/folder pickers. Must be attached to the composer view so the picker
/// presents from the composer's sheet, not the already-presenting root controller.
struct SendPickers: ViewModifier {
	@ObservedObject var model: SendModel

	func body(content: Content) -> some View {
		// A single .fileImporter, switched between files and folders. Stacking two
		// .fileImporter modifiers on one view silently breaks on iOS/macOS < 27:
		// the second shadows the first, so "Choose files" never presents.
		content
			.fileImporter(
				isPresented: Binding(
					get: { model.pendingFilePick || model.pendingFolderPick },
					set: { presented in
						if !presented {
							model.pendingFilePick = false
							model.pendingFolderPick = false
						}
					}
				),
				allowedContentTypes: model.pendingFolderPick ? [.folder] : [.item],
				allowsMultipleSelection: !model.pendingFolderPick
			) { result in
				let isDirectory = model.pendingFolderPick
				model.pendingFilePick = false
				model.pendingFolderPick = false
				handleShareSelection(result, isDirectory: isDirectory)
			}
	}

	private func handleShareSelection(_ result: Result<[URL], Error>, isDirectory: Bool) {
		switch result {
		case .success(let urls):
			let files = urls.compactMap { PickerSupport.pickedFile(from: $0, isDirectory: isDirectory) }
			if files.isEmpty {
				model.onFilePickFailed("The selected document could not be opened")
			} else {
				model.onFilesPicked(files)
			}
		case .failure(let error):
			if !error.isUserCancellation { model.onFilePickFailed(error.technicalDetail) }
		}
	}
}

enum PickerSupport {
	static func receiveFolder(from url: URL) -> ReceiveFolder {
		#if os(iOS)
		// External receive folders on iOS use security-scoped URLs; the core holds
		// access while streaming. Store the URL string.
		return ReceiveFolder(kind: .iosSecurityScopedUrl, value: url.absoluteString, displayName: url.lastPathComponent)
		#else
		return ReceiveFolder(kind: .fileSystemPath, value: url.path, displayName: url.lastPathComponent)
		#endif
	}

	static func pickedFile(from url: URL, isDirectory: Bool) -> PickedShareFile? {
		let started = url.startAccessingSecurityScopedResource()
		defer { if started { url.stopAccessingSecurityScopedResource() } }

		#if os(iOS)
		// Copy into a temporary sandbox location so the core can read the file
		// after the picker/security scope ends. Folders are passed by path.
		if isDirectory {
			return PickedShareFile(value: url.path, displayName: url.lastPathComponent, isDirectory: true)
		}
		let tempDir = FileManager.default.temporaryDirectory
			.appendingPathComponent("share-\(UUID().uuidString)", isDirectory: true)
		try? FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
		let dest = tempDir.appendingPathComponent(url.lastPathComponent)
		do {
			try FileManager.default.copyItem(at: url, to: dest)
		} catch {
			return nil
		}
		let size = (try? dest.resourceValues(forKeys: [.fileSizeKey]))?.fileSize.map { UInt64($0) }
		return PickedShareFile(
			value: dest.path, displayName: url.lastPathComponent, sizeBytes: size,
			isTemporaryCopy: true, isDirectory: false
		)
		#else
		let size = isDirectory ? nil : (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize.map { UInt64($0) }
		// Capture a security-scoped bookmark while the picker's scope is still held,
		// so the core can re-acquire access to open the file at import time (under
		// the App Store sandbox). Non-sandboxed builds don't need it but it's harmless.
		let bookmark = try? url.bookmarkData(
			options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil
		)
		return PickedShareFile(
			value: url.path, displayName: url.lastPathComponent, sizeBytes: size,
			isTemporaryCopy: false, isDirectory: isDirectory, securityScopeBookmark: bookmark
		)
		#endif
	}
}

extension View {
	func platformPickers(settingsModel: SettingsModel) -> some View {
		modifier(PlatformPickers(settingsModel: settingsModel))
	}

	func sendPickers(model: SendModel) -> some View {
		modifier(SendPickers(model: model))
	}
}
