import Foundation

/// Builds a `.vnd` invitation filename from a transfer name.
func invitationFileName(_ transferName: String) -> String {
	let trimmed = transferName.trimmingCharacters(in: .whitespacesAndNewlines)
	let base = trimmed.isEmpty ? "invitation" : trimmed
	let safe = base.components(separatedBy: CharacterSet.alphanumerics.inverted.subtracting(CharacterSet(charactersIn: "-_ ")))
		.joined()
		.replacingOccurrences(of: " ", with: "-")
	let name = safe.isEmpty ? "invitation" : safe
	return "\(name).\(vniDropInvitationExtension)"
}

/// Files live in a private directory for the lifetime of one native operation.
struct TemporaryInvitation {
	let url: URL

	init(ticket: String, transferName: String, under parent: URL? = nil) throws {
		let parent = try parent ?? AppTemporaryDirectory.prepare()
		let directory = parent.appendingPathComponent("invitation-\(UUID().uuidString)", isDirectory: true)
		try FileManager.default.createDirectory(
			at: directory, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700]
		)
		url = directory.appendingPathComponent(invitationFileName(transferName))
		do {
			try ticket.write(to: url, atomically: true, encoding: .utf8)
			try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
		} catch {
			remove()
			throw error
		}
	}

	func remove() {
		try? FileManager.default.removeItem(at: url.deletingLastPathComponent())
	}

	static func pruneStaleFiles(
		under directory: URL? = AppTemporaryDirectory.existing(),
		before cutoff: Date = .now.addingTimeInterval(-24 * 60 * 60)
	) {
		let keys: Set<URLResourceKey> = [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .contentModificationDateKey]
		guard let directory, let children = try? FileManager.default.contentsOfDirectory(
			at: directory, includingPropertiesForKeys: Array(keys)
		) else { return }

		for child in children {
			guard let values = try? child.resourceValues(forKeys: keys),
				values.isSymbolicLink == false, let modified = values.contentModificationDate, modified < cutoff else { continue }
			let name = child.lastPathComponent
			let managedDirectory = values.isDirectory == true && ["invitation-", "export-"].contains { prefix in
				name.hasPrefix(prefix) && UUID(uuidString: String(name.dropFirst(prefix.count))) != nil
			}
			let legacyFile = values.isRegularFile == true && child.pathExtension == vniDropInvitationExtension
			if managedDirectory || legacyFile { try? FileManager.default.removeItem(at: child) }
		}
	}
}
