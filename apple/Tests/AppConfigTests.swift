import XCTest
@testable import VniDrop

/// Verifies the build-time `AppConfig` (generated from the shared `app.properties`)
/// exposes the expected, well-formed values to the app.
final class AppConfigTests: XCTestCase {
	func testPrivacyPolicyURLIsTheExpectedHTTPSEndpoint() {
		let url = AppConfig.privacyPolicyURL
		XCTAssertEqual(url.scheme, "https", "Privacy policy URL must be https")
		XCTAssertEqual(url.absoluteString, "https://vnidrop.sudosy.fr/privacy/")
	}

	func testPrivacyPolicyURLMatchesTheSharedConfigFile() throws {
		// Cross-check the generated constant against the single source of truth so a
		// broken generator (or drift) is caught, not just a hardcoded copy.
		let expected = try Self.privacyURLFromAppProperties()
		XCTAssertEqual(AppConfig.privacyPolicyURL.absoluteString, expected)
	}

	/// Reads `PRIVACY_POLICY_URL` from the repo's `app.properties` by walking up
	/// from this source file's location to the repository root.
	private static func privacyURLFromAppProperties() throws -> String {
		var dir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
		for _ in 0..<8 {
			let candidate = dir.appendingPathComponent("app.properties")
			if FileManager.default.fileExists(atPath: candidate.path) {
				let contents = try String(contentsOf: candidate, encoding: .utf8)
				for line in contents.split(whereSeparator: \.isNewline) {
					if line.hasPrefix("PRIVACY_POLICY_URL=") {
						return String(line.dropFirst("PRIVACY_POLICY_URL=".count))
					}
				}
				throw XCTSkip("PRIVACY_POLICY_URL missing in \(candidate.path)")
			}
			dir.deleteLastPathComponent()
		}
		throw XCTSkip("app.properties not found from \(#filePath)")
	}
}
