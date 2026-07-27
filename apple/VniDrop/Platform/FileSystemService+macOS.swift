#if os(macOS)
import Foundation
import AppKit
import VnidropCore

/// macOS file system service. Mirrors the desktop JVM behavior: default Downloads
/// receive folder, custom folders enabled via security-scoped bookmarks, reveal in
/// Finder. The Rust core streams bytes; Swift passes filesystem paths.
struct MacFileSystemService: FileSystemService {
	var supportsCustomReceiveFolders: Bool { true }

	func defaultReceiveFolder() -> ReceiveFolder {
		let url = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
			?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Downloads")
		return ReceiveFolder(kind: .fileSystemPath, value: url.path, displayName: url.lastPathComponent)
	}

	func validateReceiveFolder(_ folder: ReceiveFolder) async -> FolderAccessStatus {
		FileManager.default.isWritableFile(atPath: folder.value) ? .writable : .unavailable
	}

	func canRevealReceiveFolder(_ folder: ReceiveFolder) -> Bool { true }

	func revealReceiveFolder(_ folder: ReceiveFolder) async -> Result<Void, Error> {
		let url = URL(fileURLWithPath: folder.value, isDirectory: true)
		NSWorkspace.shared.activateFileViewerSelecting([url])
		return .success(())
	}

	func discardPickedFiles(_ files: [PickedShareFile]) async {
		let paths = Set(files.filter { $0.isTemporaryCopy }.map { $0.value })
		for path in paths {
			try? FileManager.default.removeItem(atPath: path)
		}
	}

	func sharePickedFiles(
		repository: CoreGateway,
		files: [PickedShareFile],
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy
	) async -> Result<Share, Error> {
		guard !files.isEmpty else {
			return .failure(InvitationError.message("Select at least one file to share"))
		}
		// Re-acquire security-scoped access to every picked source (from the bookmark
		// captured at pick time) and hold it across the whole share call. The core
		// imports the bytes during shareFiles(), so access only needs to survive that
		// call; without this, the import fails with EPERM under the App Store sandbox.
		var scopedURLs: [URL] = []
		for file in files {
			guard let bookmark = file.securityScopeBookmark else { continue }
			var stale = false
			guard let url = try? URL(
				resolvingBookmarkData: bookmark, options: .withSecurityScope,
				relativeTo: nil, bookmarkDataIsStale: &stale
			), url.startAccessingSecurityScopedResource() else { continue }
			scopedURLs.append(url)
		}
		defer { scopedURLs.forEach { $0.stopAccessingSecurityScopedResource() } }

		let sources = files.map {
			ShareSource(kind: .path, value: $0.value, displayName: $0.displayName, isDirectory: $0.isDirectory)
		}
		return await repository.shareSources(
			sources, transferName: transferName, senderName: senderName, accessPolicy: accessPolicy
		)
	}
}
#endif
