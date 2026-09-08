import Foundation

struct BugReportConfiguration: Sendable {
	let submissionURL: URL
	let ingestKey: String

	init(endpoint: String, ingestKey: String) throws {
		let endpoint = endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
		guard let url = URLComponents(string: endpoint),
			let host = url.host, !host.isEmpty,
			url.scheme == "https",
			url.user == nil, url.password == nil, url.query == nil, url.fragment == nil,
			!endpoint.unicodeScalars.contains(where: { CharacterSet.whitespacesAndNewlines.contains($0) }),
			!ingestKey.isEmpty, ingestKey.utf8.allSatisfy({ (33...126).contains($0) }),
			let baseURL = url.url
		else { throw BugReportError.notConfigured }
		submissionURL = baseURL.appendingPathComponent("v1/bugs")
		self.ingestKey = ingestKey
	}
}
