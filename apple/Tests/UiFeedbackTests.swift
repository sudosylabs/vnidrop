import XCTest
import VnidropCore
@testable import VniDrop

/// Ports `ui/feedback/UiMessageControllerTest.kt` and `UserFacingErrorTest.kt`.
@MainActor
final class UiMessageControllerTests: XCTestCase {

	func testQueuesAndAdvances() {
		let c = UiMessageController()
		c.show(UiMessage(text: .dynamic("first")))
		c.show(UiMessage(text: .dynamic("second")))
		XCTAssertEqual(c.current?.text, .dynamic("first"))

		c.advance()
		XCTAssertEqual(c.current?.text, .dynamic("second"))

		c.advance()
		XCTAssertNil(c.current)
	}

	func testErrorSuppressesUserCancellation() {
		let c = UiMessageController()
		c.error(InvitationError.message("QR scanning was cancelled"))
		XCTAssertNil(c.current) // cancellations are swallowed
	}

	func testErrorShowsNonCancellation() {
		let c = UiMessageController()
		c.error(InvitationError.message("The transfer was refused"))
		XCTAssertEqual(c.current?.tone, .error)
	}
}

@MainActor
final class UserFacingErrorTests: XCTestCase {

	func testIsUserCancellation() {
		XCTAssertTrue(InvitationError.message("NFC reading was cancelled").isUserCancellation)
		XCTAssertTrue(InvitationError.message("User canceled the picker").isUserCancellation)
		XCTAssertFalse(InvitationError.message("A database error occurred").isUserCancellation)
	}

	func testToUiTextMapsKnownReasons() {
		XCTAssertEqual(InvitationError.message("The transfer was refused").toUiText(), .resource(L10n.Error.permission))
		XCTAssertEqual(InvitationError.message("invalid ticket").toUiText(), .resource(L10n.Error.invalidTicket))
		XCTAssertEqual(InvitationError.message("Select at least one file to share").toUiText(), .resource(L10n.Error.shareEmpty))
		XCTAssertEqual(InvitationError.message("Camera access is required").toUiText(), .resource(L10n.Error.camera))
	}

	func testToUiTextFallsBackToGeneric() {
		XCTAssertEqual(InvitationError.message("something entirely unexpected").toUiText(), .resource(L10n.Error.generic))
	}

	func testToUiTextMapsTypedTransferFailures() {
		XCTAssertEqual(VnidropError.FilesystemPermission(reason: "read-only folder").toUiText(), .resource(L10n.Error.filesystem))
		XCTAssertEqual(VnidropError.DestinationExists(reason: "target exists").toUiText(), .resource(L10n.Error.destinationExists))
		XCTAssertEqual(VnidropError.StorageFull(reason: "disk full").toUiText(), .resource(L10n.Error.storageFull))
		XCTAssertEqual(VnidropError.Network(reason: "offline").toUiText(), .resource(L10n.Error.network))
		XCTAssertEqual(VnidropError.InvalidInput(reason: "bad path").toUiText(), .resource(L10n.Error.invalidInput))
		XCTAssertFalse(VnidropError.FilesystemPermission(reason: "read-only").canRetryWithoutChangingInput)
		XCTAssertFalse(VnidropError.DestinationExists(reason: "target exists").canRetryWithoutChangingInput)
		XCTAssertTrue(VnidropError.Network(reason: "offline").canRetryWithoutChangingInput)
	}
}
