import XCTest
@testable import VniDrop

final class RelayConfigurationTests: XCTestCase {
	func testCustomFallbackValidatesAndPreservesItsMode() throws {
		let result = try RelayConfigurationValidator.validate(
			mode: .customWithDirectFallback,
			relayURLs: ["https://relay.example/"]
		)

		XCTAssertEqual(
			result,
			RelayConfiguration(
				mode: .customWithDirectFallback,
				relayURLs: ["https://relay.example"]
			)
		)
	}

	func testLocalOnlyRetainsPreviouslySavedRelayURLs() throws {
		let retained = ["https://relay.example"]
		let result = try RelayConfigurationValidator.validate(
			mode: .localOnly,
			relayURLs: ["not a URL"],
			retainedRelayURLs: retained
		)

		XCTAssertEqual(result, RelayConfiguration(mode: .localOnly, relayURLs: retained))
	}

	func testAutomaticModeIgnoresRelayDrafts() throws {
		let result = try RelayConfigurationValidator.validate(
			mode: .automatic,
			relayURLs: ["not a URL"]
		)
		XCTAssertEqual(result, .automatic)
	}

	func testAutomaticModeRetainsPreviouslySavedRelayURLs() throws {
		let result = try RelayConfigurationValidator.validate(
			mode: .automatic,
			relayURLs: ["not a URL"],
			retainedRelayURLs: ["https://relay.example"]
		)
		XCTAssertEqual(result, RelayConfiguration(
			mode: .automatic,
			relayURLs: ["https://relay.example"]
		))
	}

	func testCustomModeTrimsValidHTTPSRelayURLs() throws {
		let result = try RelayConfigurationValidator.validate(
			mode: .strictCustom,
			relayURLs: ["  https://relay.example/  ", "https://backup.example:443"]
		)
		XCTAssertEqual(result, RelayConfiguration(
			mode: .strictCustom,
			relayURLs: ["https://relay.example", "https://backup.example"]
		))
	}

	func testCustomModeIgnoresEmptyURLRows() throws {
		let result = try RelayConfigurationValidator.validate(
			mode: .strictCustom,
			relayURLs: ["", "  ", "https://relay.example"]
		)
		XCTAssertEqual(result.relayURLs, ["https://relay.example"])
	}

	func testCustomModeRequiresAtLeastOneRelay() {
		XCTAssertThrowsError(try RelayConfigurationValidator.validate(mode: .strictCustom, relayURLs: [])) { error in
			XCTAssertEqual(error as? RelayConfigurationValidationError, .missingURL)
		}
	}

	func testCustomModeRequiresHTTPS() {
		XCTAssertThrowsError(try RelayConfigurationValidator.validate(
			mode: .strictCustom,
			relayURLs: ["http://relay.example"]
		)) { error in
			XCTAssertEqual(error as? RelayConfigurationValidationError, .httpsRequired(index: 0))
		}
	}

	func testCustomModeRejectsCredentialsQueryFragmentAndPath() {
		let invalidURLs = [
			"https://user:password@relay.example",
			"https://relay.example?token=secret",
			"https://relay.example#fragment",
			"https://relay.example/custom/path",
			"https://relay.example:0",
			"https://relay.example:99999",
		]
		for relayURL in invalidURLs {
			XCTAssertThrowsError(
				try RelayConfigurationValidator.validate(mode: .strictCustom, relayURLs: [relayURL]),
				"Expected \(relayURL) to be rejected"
			) { error in
				XCTAssertEqual(error as? RelayConfigurationValidationError, .invalidURL(index: 0))
			}
		}
	}

	func testCustomModeRejectsNormalizedDuplicate() {
		XCTAssertThrowsError(try RelayConfigurationValidator.validate(
			mode: .strictCustom,
			relayURLs: ["https://relay.example", "https://RELAY.example:443/"]
		)) { error in
			XCTAssertEqual(error as? RelayConfigurationValidationError, .duplicateURL(index: 1))
		}
	}

	func testCustomModeRejectsMoreThanEightRelays() {
		let relayURLs = (0...RelayConfigurationValidator.maximumRelayCount).map {
			"https://relay-\($0).example"
		}
		XCTAssertThrowsError(try RelayConfigurationValidator.validate(mode: .strictCustom, relayURLs: relayURLs)) { error in
			XCTAssertEqual(error as? RelayConfigurationValidationError, .tooManyURLs)
		}
	}
}
