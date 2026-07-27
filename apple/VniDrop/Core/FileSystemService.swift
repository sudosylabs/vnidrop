import Foundation
import VnidropCore

/// A file/folder selected for sharing, ported from `PickedShareFile` in
/// `core/FilePicker.kt`.
struct PickedShareFile: Equatable, Identifiable, Sendable {
	let value: String
	let displayName: String
	var sizeBytes: UInt64? = nil
	var thumbnailData: Data? = nil
	/// App-owned picker copy that may be deleted after import or abandonment.
	var isTemporaryCopy: Bool = false
	/// When true, `value` is a directory (path or security-scoped folder URL).
	var isDirectory: Bool = false
	/// macOS sandbox: a security-scoped bookmark captured at pick time so access to
	/// `value` can be re-acquired when the core imports the file (the picker's own
	/// scope ends immediately). Nil on iOS (which copies into the container instead).
	var securityScopeBookmark: Data? = nil

	var id: String { value }
}

/// Receive-destination and share-source platform bridge, ported from
/// `core/FileSystemService.kt` and its iOS/desktop actuals.
@MainActor
protocol FileSystemService {
	var supportsCustomReceiveFolders: Bool { get }

	func defaultReceiveFolder() -> ReceiveFolder
	func effectiveReceiveFolder(_ configured: ReceiveFolder) -> ReceiveFolder
	func validateReceiveFolder(_ folder: ReceiveFolder) async -> FolderAccessStatus
	func canRevealReceiveFolder(_ folder: ReceiveFolder) -> Bool
	func revealReceiveFolder(_ folder: ReceiveFolder) async -> Result<Void, Error>
	/// Releases only app-owned picker copies; never deletes original user sources.
	func discardPickedFiles(_ files: [PickedShareFile]) async
	func sharePickedFiles(
		repository: CoreGateway,
		files: [PickedShareFile],
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy
	) async -> Result<Share, Error>
}

extension FileSystemService {
	var supportsCustomReceiveFolders: Bool { true }

	func effectiveReceiveFolder(_ configured: ReceiveFolder) -> ReceiveFolder {
		supportsCustomReceiveFolders ? configured : defaultReceiveFolder()
	}

	func canRevealReceiveFolder(_ folder: ReceiveFolder) -> Bool { false }

	func revealReceiveFolder(_ folder: ReceiveFolder) async -> Result<Void, Error> {
		.failure(InvitationError.message("Revealing the receive folder is not supported"))
	}

	func discardPickedFiles(_ files: [PickedShareFile]) async {}
}

extension ReceiveFolder {
	var isFileSystemPath: Bool { kind == .fileSystemPath }
}
