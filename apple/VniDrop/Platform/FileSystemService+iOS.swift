#if os(iOS)
import Foundation
import UIKit
import VnidropCore

/// Native iOS file system service.
/// App-owned Documents is the fixed receive folder; custom folders are not
/// supported because raw external picker URLs do not survive relaunch.
struct IosFileSystemService: FileSystemService {
	var supportsCustomReceiveFolders: Bool { false }

	func defaultReceiveFolder() -> ReceiveFolder {
		let path = FileManager.default
			.urls(for: .documentDirectory, in: .userDomainMask)
			.first?.path ?? ""
		return ReceiveFolder(kind: .fileSystemPath, value: path, displayName: "Documents")
	}

	func validateReceiveFolder(_ folder: ReceiveFolder) async -> FolderAccessStatus {
		switch folder.kind {
		case .fileSystemPath:
			return FileManager.default.isWritableFile(atPath: folder.value) ? .writable : .unavailable
		case .iosSecurityScopedUrl:
			return validateSecurityScopedUrl(folder.value)
		}
	}

	func canRevealReceiveFolder(_ folder: ReceiveFolder) -> Bool {
		folder.kind == .fileSystemPath
			&& folder.value.trimmingTrailingSlash == defaultReceiveFolder().value.trimmingTrailingSlash
	}

	func revealReceiveFolder(_ folder: ReceiveFolder) async -> Result<Void, Error> {
		guard canRevealReceiveFolder(folder) else {
			return .failure(InvitationError.filesystemUnavailable)
		}
		guard let url = URL(string: "shareddocuments://\(folder.value)") else {
			return .failure(InvitationError.filesystemUnavailable)
		}
		let opened = await withCheckedContinuation { continuation in
			DispatchQueue.main.async {
				UIApplication.shared.open(url, options: [:]) { success in
					continuation.resume(returning: success)
				}
			}
		}
		return opened ? .success(()) : .failure(InvitationError.filesystemUnavailable)
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
		destination: ShareDestination
	) async -> Result<Share, Error> {
		guard !files.isEmpty else {
			return .failure(InvitationError.shareEmpty)
		}
		let sources = files.map { $0.toIosShareSource() }
		switch destination {
		case .invitation(let accessPolicy):
			return await repository.shareSources(
				sources, transferName: transferName, senderName: senderName, accessPolicy: accessPolicy
			)
		case .contact(let endpointId):
			return await repository.sendToContact(
				endpointId: endpointId, sources: sources,
				transferName: transferName, senderName: senderName
			)
		}
	}

	private func validateSecurityScopedUrl(_ value: String) -> FolderAccessStatus {
		let url = URL(string: value) ?? URL(fileURLWithPath: value)
		let started = url.startAccessingSecurityScopedResource()
		defer { if started { url.stopAccessingSecurityScopedResource() } }
		if FileManager.default.isWritableFile(atPath: url.path) {
			return .writable
		}
		return .permissionRequired
	}
}

extension PickedShareFile {
	/// iOS shares by filesystem path (from `asCopy` picker temp files).
	func toIosShareSource() -> ShareSource {
		ShareSource(kind: .path, value: value, displayName: displayName, isDirectory: isDirectory)
	}
}

private extension String {
	var trimmingTrailingSlash: String {
		var s = self
		while s.hasSuffix("/") { s.removeLast() }
		return s
	}
}
#endif
