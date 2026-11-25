import Foundation

enum AppTemporaryDirectory {
	private nonisolated static func location(under systemTemporaryDirectory: URL) -> URL {
		systemTemporaryDirectory.appendingPathComponent("com.vnidrop.app", isDirectory: true)
	}

	nonisolated static func existing(under systemTemporaryDirectory: URL = FileManager.default.temporaryDirectory) -> URL? {
		let url = location(under: systemTemporaryDirectory)
		guard let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]),
			values.isDirectory == true, values.isSymbolicLink == false else { return nil }
		return url
	}

	nonisolated static func prepare() throws -> URL {
		let systemTemporaryDirectory = FileManager.default.temporaryDirectory
		let url = location(under: systemTemporaryDirectory)
		try FileManager.default.createDirectory(
			at: url,
			withIntermediateDirectories: true,
			attributes: [.posixPermissions: 0o700]
		)
		guard let directory = existing(under: systemTemporaryDirectory) else {
			throw CocoaError(.fileWriteNoPermission)
		}
		return directory
	}
}
