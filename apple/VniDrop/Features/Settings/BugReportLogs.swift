import Darwin
import Foundation

protocol BugReportLogReading: Sendable {
	func recentLogs() async throws -> String
}

actor CoreBugReportLogs: BugReportLogReading {
	private let directory: URL

	init(dataDirectory: String) {
		directory = URL(fileURLWithPath: dataDirectory).appendingPathComponent("logs", isDirectory: true)
	}

	func recentLogs() throws -> String {
		let directoryFD = open(directory.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
		guard directoryFD >= 0 else { return "" }
		defer { close(directoryFD) }
		var remaining = BugReportPayload.maximumLogBytes * 4
		var chunks: [String] = []
		for index in 0...5 where remaining > 0 {
			try Task.checkCancellation()
			let name = index == 0 ? "vnidrop.log" : "vnidrop.\(index).log"
			// Open relative to the held directory and reject links, including a rotation race.
			let descriptor = openat(directoryFD, name, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
			guard descriptor >= 0 else { continue }
			let file = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
			defer { try? file.close() }
			var metadata = stat()
			guard fstat(descriptor, &metadata) == 0,
				metadata.st_mode & S_IFMT == S_IFREG, metadata.st_size > 0 else { continue }
			let count = min(remaining, Int(metadata.st_size))
			let offset = UInt64(metadata.st_size) - UInt64(count)
			do {
				try file.seek(toOffset: offset)
				guard var data = try file.read(upToCount: count), !data.isEmpty else { continue }
				remaining -= data.count
				if offset > 0 {
					// A partial first line might begin inside a ticket, hiding its redaction prefix.
					guard let newline = data.firstIndex(of: 10) else { continue }
					data = data.suffix(from: data.index(after: newline))
				}
				chunks.append(String(decoding: data, as: UTF8.self))
			} catch {
				// Logs are optional; a file may be rotated away while a report is prepared.
				continue
			}
		}
		return BugReportPayload.tail(
			Self.redact(chunks.reversed().joined(separator: "\n")),
			maximumBytes: BugReportPayload.maximumLogBytes
		)
	}

	static func redact(_ input: String) -> String {
		let rules: [(String, String)] = [
			(#"\bvnd1:[^\s\"'<>]+"#, "[redacted-ticket]"),
			(#"\bvndaddr1:[^\s\"'<>]+"#, "[redacted-endpoint]"),
			(#"\b(?:blob[_-]?ticket|ticket|invitation)\b[\"']?\s*[:=]\s*[\"']?[^\s,\"'<>}\]]{24,}"#, "[redacted-ticket]"),
			(#"\b(?:(?:peer|sender|receiver|from|remote|local)[_-]?)?(?:endpoint|device|node)[_-]?id\b[\"']?\s*[:=]\s*[\"']?[A-Za-z0-9+/=_-]{16,}"#, "[redacted-endpoint]"),
			(#"\b[0-9a-f]{32,}\b"#, "[redacted-hex]"),
			(#"\b(?:file[_-]?(?:name|path)|relative[_-]?path|path)\b[\"']?\s*[:=]\s*(?:[\"'][^\"'\r\n]*[\"']|.*?(?=\s+\b[A-Za-z_][A-Za-z0-9_-]*\b\s*=|[,}\]\r\n]|$))"#, "[redacted-path]"),
			(#"\b[a-z][a-z0-9+.-]{1,31}://[^\s<>\"']+"#, "[redacted-uri]"),
			(#"(?<![A-Za-z0-9_])(?:[A-Za-z]:\\|\\\\)[^\r\n\t\"'<>]+"#, "[redacted-path]"),
			(#"(?<![A-Za-z0-9_])/(?!/)(?:[^\s/:\"'<>]+/)*[^\s:\"'<>]+"#, "[redacted-path]"),
			(#"\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b"#, "[redacted-email]"),
			(#"\b(?:\d{1,3}\.){3}\d{1,3}\b"#, "[redacted-address]"),
			(#"(?<![A-Za-z0-9])(?=[0-9a-f:]*::|(?:[0-9a-f]{1,4}:){7})(?:[0-9a-f]{0,4}:){2,}[0-9a-f:.]*(?:%[A-Za-z0-9]+)?"#, "[redacted-address]"),
		]
		return rules.reduce(input) { text, rule in
			text.replacingOccurrences(of: rule.0, with: rule.1, options: [.regularExpression, .caseInsensitive])
		}
	}
}
