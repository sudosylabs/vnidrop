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

/// Writes a temporary `.vnd` file for sharing/exporting.
func writeTemporaryInvitation(ticket: String, transferName: String) throws -> URL {
	let dir = try AppTemporaryDirectory.prepare()
	let url = dir.appendingPathComponent(invitationFileName(transferName))
	try ticket.write(to: url, atomically: true, encoding: .utf8)
	return url
}
