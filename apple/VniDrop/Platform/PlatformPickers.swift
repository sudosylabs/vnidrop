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
					do {
						settingsModel.onReceiveFolderPicked(try PickerSupport.receiveFolder(from: url))
					} catch {
						settingsModel.onReceiveFolderPickFailed(error.technicalDetail)
					}
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
	@State private var selectionTask: Task<Void, Never>?

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
			.onDisappear { selectionTask?.cancel() }
	}

	private func handleShareSelection(_ result: Result<[URL], Error>, isDirectory: Bool) {
		switch result {
		case .success(let urls):
			selectionTask?.cancel()
			selectionTask = PickerSupport.loadShareSelection(urls, isDirectory: isDirectory) { files in
				guard model.state.isComposerOpen, !model.state.isSharing else { return false }
				if files.isEmpty {
					model.onFilePickFailed("The selected document could not be opened")
				} else {
					model.onFilesPicked(files)
				}
				return true
			}
		case .failure(let error):
			if !error.isUserCancellation { model.onFilePickFailed(error.technicalDetail) }
		}
	}
}

/// File picker for "send to this device", reusing the share picker's selection
/// handling so security-scoped bookmarks are captured the same way.

enum PickerSupport {
	static func acquireShareAccess(_ url: URL) -> (@Sendable () -> Void)? {
		guard url.startAccessingSecurityScopedResource() else { return nil }
		return { url.stopAccessingSecurityScopedResource() }
	}

	@MainActor
	static func loadShareSelection(
		_ urls: [URL], isDirectory: Bool, onResult: @escaping ([PickedShareFile]) -> Bool
	) -> Task<Void, Never> {
		let worker = stageShareSelection(urls, isDirectory: isDirectory)
		return Task {
			let files = await withTaskCancellationHandler {
				await worker.value
			} onCancel: {
				worker.cancel()
			}
			if Task.isCancelled || !onResult(files) {
				#if os(iOS)
				await IosFileSystemService().discardPickedFiles(files)
				#endif
			}
		}
	}

	@MainActor
	static func stageShareSelection(
		_ urls: [URL],
		isDirectory: Bool,
		acquireAccess: (URL) -> (@Sendable () -> Void)? = acquireShareAccess,
		copyFile: @escaping @Sendable (URL, URL) throws -> Void = { try FileManager.default.copyItem(at: $0, to: $1) }
	) -> Task<[PickedShareFile], Never> {
		#if os(iOS)
		// Acquire access in the picker callback before sending URLs to the worker.
		let selections = urls.map { url in
			(url, acquireAccess(url).map { SecurityScopedFolderAccess(release: $0) })
		}
		return Task.detached(priority: .userInitiated) {
			var files: [PickedShareFile] = []
			for (url, access) in selections {
				if Task.isCancelled { break }
				if let file = stageIosFile(from: url, isDirectory: isDirectory, access: access, copyFile: copyFile) {
					files.append(file)
				}
			}
			return files
		}
		#else
		let files = urls.compactMap { pickedFile(from: $0, isDirectory: isDirectory, acquireAccess: acquireAccess) }
		return Task { files.count == urls.count ? files : [] }
		#endif
	}

	static func receiveFolder(from url: URL) throws -> ReceiveFolder {
		#if os(iOS)
		// External receive folders on iOS use security-scoped URLs; the core holds
		// access while streaming. Store the URL string.
		return ReceiveFolder(kind: .iosSecurityScopedUrl, value: url.absoluteString, displayName: url.lastPathComponent)
		#elseif DIRECT_DISTRIBUTION
		return ReceiveFolder(kind: .fileSystemPath, value: url.path, displayName: url.lastPathComponent)
		#else
		let accessing = url.startAccessingSecurityScopedResource()
		defer { if accessing { url.stopAccessingSecurityScopedResource() } }
		let bookmark = try url.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil)
		return ReceiveFolder(
			kind: .fileSystemPath, value: url.path, displayName: url.lastPathComponent,
			securityScopeBookmark: bookmark
		)
		#endif
	}

	#if os(macOS)
	private static func pickedFile(
		from url: URL,
		isDirectory: Bool,
		acquireAccess: (URL) -> (@Sendable () -> Void)? = acquireShareAccess
	) -> PickedShareFile? {
		let releaseAccess = acquireAccess(url)
		defer { releaseAccess?() }
		let size = isDirectory ? nil : (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize.map { UInt64($0) }
		// The App Store sandbox needs the bookmark when Rust opens the selected path.
		guard let bookmark = try? url.bookmarkData(
			options: .withSecurityScope, includingResourceValuesForKeys: [.fileResourceIdentifierKey], relativeTo: nil
		) else { return nil }
		return PickedShareFile(
			value: url.path, displayName: url.lastPathComponent, sizeBytes: size,
			isTemporaryCopy: false, isDirectory: isDirectory, securityScopeBookmark: bookmark
		)
	}
	#else
	private static func stageIosFile(
		from url: URL, isDirectory: Bool, access: SecurityScopedFolderAccess?,
		copyFile: (URL, URL) throws -> Void
	) -> PickedShareFile? {
		defer { withExtendedLifetime(access) {} }
		if isDirectory || (try? url.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true {
			return PickedShareFile(
				value: url.path,
				displayName: url.lastPathComponent,
				isDirectory: true,
				folderAccess: access
			)
		}
		// A staged file remains available after the picker scope closes.
		guard let appTemp = try? AppTemporaryDirectory.prepare() else { return nil }
		let tempDir = appTemp
			.appendingPathComponent("share-\(UUID().uuidString)", isDirectory: true)
		try? FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
		let dest = tempDir.appendingPathComponent(url.lastPathComponent)
		do {
			try copyFile(url, dest)
		} catch {
			try? FileManager.default.removeItem(at: tempDir)
			return nil
		}
		let size = (try? dest.resourceValues(forKeys: [.fileSizeKey]))?.fileSize.map { UInt64($0) }
		return PickedShareFile(
			value: dest.path, displayName: url.lastPathComponent, sizeBytes: size,
			isTemporaryCopy: true, isDirectory: false
		)
	}
	#endif
}

extension View {
	func platformPickers(settingsModel: SettingsModel) -> some View {
		modifier(PlatformPickers(settingsModel: settingsModel))
	}

	func sendPickers(model: SendModel) -> some View {
		modifier(SendPickers(model: model))
	}

}
