import Foundation

struct BugReportPayload: Encodable, Sendable {
	static let maximumLogBytes = 192 * 1024
	static let maximumBodyBytes = 256 * 1024

	let schemaVersion = 1
	let id: String
	let timestampMillis: Int64
	let installId: String
	let appVersion: String
	let platform: String
	let whatHappened: String
	let expected: String
	let steps: String
	let contact: String
	let includeLogs: Bool
	var logs: String
	let device: Device

	struct Device: Encodable, Sendable {
		let deviceName: String
		let deviceModel: String
		let operatingSystem: String
		let network: String
		let batteryLevel: String
	}

	init(
		draft: BugReportDraft, installId: String, environment: PlatformEnvironment,
		deviceInfo: DeviceInfo?, logs: String, id: UUID = UUID(), date: Date = Date()
	) {
		self.id = id.uuidString.lowercased()
		timestampMillis = Int64(date.timeIntervalSince1970 * 1000)
		self.installId = installId
		appVersion = Self.prefix(environment.appVersion, maximumBytes: 40)
		platform = Self.prefix(environment.name, maximumBytes: 40)
		whatHappened = draft.whatHappened.trimmingCharacters(in: .whitespacesAndNewlines)
		expected = draft.expected.trimmingCharacters(in: .whitespacesAndNewlines)
		steps = draft.steps.trimmingCharacters(in: .whitespacesAndNewlines)
		contact = draft.contact.trimmingCharacters(in: .whitespacesAndNewlines)
		includeLogs = draft.includeLogs
		self.logs = draft.includeLogs ? Self.tail(logs, maximumBytes: Self.maximumLogBytes) : ""
		device = Device(
			deviceName: Self.prefix(deviceInfo?.deviceName ?? "", maximumBytes: 128),
			deviceModel: Self.prefix(deviceInfo?.deviceModel ?? "", maximumBytes: 128),
			operatingSystem: Self.prefix(deviceInfo?.operatingSystem ?? "", maximumBytes: 192),
			network: Self.prefix(deviceInfo?.network ?? "", maximumBytes: 96),
			batteryLevel: Self.prefix(deviceInfo?.batteryLevel ?? "", maximumBytes: 64)
		)
	}

	static func validate(_ draft: BugReportDraft) throws {
		guard !draft.whatHappened.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
			throw BugReportError.missingWhat
		}
		guard !draft.expected.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
			throw BugReportError.missingExpected
		}
		guard [draft.whatHappened, draft.expected, draft.steps].allSatisfy({ $0.utf8.count <= 4000 }),
			draft.contact.utf8.count <= 320 else { throw BugReportError.textTooLong }
	}

	func encoded() throws -> Data {
		var report = self
		let encoder = JSONEncoder()
		var body = try encoder.encode(report)
		// JSON escaping can expand logs well beyond their UTF-8 size.
		while body.count > Self.maximumBodyBytes && !report.logs.isEmpty {
			report.logs = Self.tail(report.logs, maximumBytes: report.logs.utf8.count / 2)
			body = try encoder.encode(report)
		}
		guard body.count <= Self.maximumBodyBytes else { throw BugReportError.invalidPayload }
		return body
	}

	static func prefix(_ value: String, maximumBytes: Int) -> String {
		let bytes = Array(value.utf8.prefix(maximumBytes))
		var end = bytes.count
		while end > 0 {
			if let result = String(bytes: bytes[..<end], encoding: .utf8) { return result }
			end -= 1
		}
		return ""
	}

	static func tail(_ value: String, maximumBytes: Int) -> String {
		let bytes = value.utf8.suffix(maximumBytes).drop(while: { $0 & 0xC0 == 0x80 })
		return String(decoding: bytes, as: UTF8.self)
	}
}
