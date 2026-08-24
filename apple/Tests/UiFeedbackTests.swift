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
		c.error(InvitationError.cancelled)
		XCTAssertNil(c.current) // cancellations are swallowed
	}

	func testErrorShowsNonCancellation() {
		let c = UiMessageController()
		c.error(InvitationError.raw("The transfer was refused"))
		XCTAssertEqual(c.current?.tone, .error)
	}
}

@MainActor
final class UserFacingErrorTests: XCTestCase {

	func testIsUserCancellation() {
		XCTAssertTrue(InvitationError.cancelled.isUserCancellation)
		XCTAssertTrue(InvitationError.raw("User canceled the picker").isUserCancellation)
		XCTAssertFalse(InvitationError.raw("A database error occurred").isUserCancellation)
	}

	func testToUiTextMapsKnownReasons() {
		// Typed cases map directly at the UI boundary.
		XCTAssertEqual(InvitationError.shareEmpty.toUiText(), .resource(L10n.Error.shareEmpty))
		XCTAssertEqual(InvitationError.cameraUnavailable.toUiText(), .resource(L10n.Error.camera))
		XCTAssertEqual(InvitationError.nfcFailed.toUiText(), .resource(L10n.Error.nfc))
		// Dynamic `.raw` payloads still fall through the substring hints.
		XCTAssertEqual(InvitationError.raw("The transfer was refused").toUiText(), .resource(L10n.Error.permission))
		XCTAssertEqual(InvitationError.raw("invalid ticket").toUiText(), .resource(L10n.Error.invalidTicket))
	}

	func testToUiTextFallsBackToGeneric() {
		XCTAssertEqual(InvitationError.raw("something entirely unexpected").toUiText(), .resource(L10n.Error.generic))
	}

	func testToUiTextMapsTypedTransferFailures() {
		XCTAssertEqual(VnidropError.FilesystemPermission(reason: "read-only folder").toUiText(), .resource(L10n.Error.filesystem))
		XCTAssertEqual(VnidropError.DestinationExists(reason: "target exists").toUiText(), .resource(L10n.Error.destinationExists))
		XCTAssertEqual(VnidropError.StorageFull(reason: "disk full").toUiText(), .resource(L10n.Error.storageFull))
		XCTAssertEqual(VnidropError.Network(reason: "offline").toUiText(), .resource(L10n.Error.network))
		XCTAssertEqual(VnidropError.InvalidInput(reason: "bad path").toUiText(), .resource(L10n.Error.invalidInput))
		XCTAssertFalse(VnidropError.FilesystemPermission(reason: "read-only").canRetryWithoutChangingInput)
		XCTAssertFalse(VnidropError.DestinationExists(reason: "target exists").canRetryWithoutChangingInput)
		XCTAssertFalse(VnidropError.SecureStorageMissing(reason: "credential is missing").canRetryWithoutChangingInput)
		XCTAssertFalse(VnidropError.SecureStorageCorrupted(reason: "credential is corrupted").canRetryWithoutChangingInput)
		XCTAssertTrue(VnidropError.Network(reason: "offline").canRetryWithoutChangingInput)
		XCTAssertTrue(VnidropError.SecureStorageLocked(reason: "credential store is locked").canRetryWithoutChangingInput)
		// Unreachable, not busy: retrying re-enters the call that just failed.
		XCTAssertFalse(
			VnidropError.SecureStorageUnavailable(reason: "credential store is unavailable")
				.canRetryWithoutChangingInput
		)
	}
}
