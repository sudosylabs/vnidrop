import XCTest
@testable import VniDrop
#if os(iOS)
import UIKit
#endif

/// Ports `feature/receive/ExternalInvitationControllerTest.kt` + the `.vnd`
/// decode/filename helpers.
@MainActor
final class InvitationTests: XCTestCase {

	func testInvitationsSurviveObserverReplacementAndDrainInOrder() async throws {
		let controller = ExternalInvitationController()
		controller.openInvitation(raw: "cold-start-ticket")
		let (first, _) = controller.observePendingInvitations()
		controller.stopObserving(first)
		controller.openInvitation(raw: "second-ticket")

		let (second, changes) = controller.observePendingInvitations()
		defer { controller.stopObserving(second) }
		var changesIterator = changes.makeAsyncIterator()
		_ = await changesIterator.next()
		let firstInvitation = try XCTUnwrap(controller.nextInvitation)
		XCTAssertEqual(try controller.takeInvitation(firstInvitation.id)?.result.get(), "cold-start-ticket")
		let secondInvitation = try XCTUnwrap(controller.nextInvitation)
		XCTAssertEqual(try controller.takeInvitation(secondInvitation.id)?.result.get(), "second-ticket")
		XCTAssertNil(controller.nextInvitation)
	}

	func testInvitationReaderEnforcesByteLimit() throws {
		let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		defer { try? FileManager.default.removeItem(at: file) }
		let contents = Data(repeating: 65, count: maxVniDropInvitationBytes)
		try contents.write(to: file)
		XCTAssertEqual(try readInvitationFile(file), String(decoding: contents, as: UTF8.self))
		try (contents + Data([65])).write(to: file)
		XCTAssertThrowsError(try readInvitationFile(file)) { error in
			guard case .tooLarge? = error as? InvitationError else { return XCTFail("expected size limit") }
		}
	}

	func testExternalInvitationKeepsDocumentAndFileErrorsTyped() async throws {
		let controller = ExternalInvitationController()
		let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
		try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
		defer { try? FileManager.default.removeItem(at: directory) }
		let malformed = directory.appendingPathComponent("malformed.VND")
		try Data([0xFF]).write(to: malformed)

		await controller.openInvitation(from: malformed)
		let first = try XCTUnwrap(controller.nextInvitation)
		XCTAssertThrowsError(try controller.takeInvitation(first.id)?.result.get()) { error in
			guard case .invalidEncoding? = error as? InvitationError else {
				return XCTFail("Expected a typed invalid encoding error, got \(error)")
			}
			XCTAssertEqual(error.toUiText(), .resource(L10n.Error.invalidTicket))
		}

		await controller.openInvitation(from: directory.appendingPathComponent("missing.vnd"))
		let second = try XCTUnwrap(controller.nextInvitation)
		XCTAssertThrowsError(try controller.takeInvitation(second.id)?.result.get()) { error in
			let native = error as NSError
			XCTAssertEqual(native.domain, NSCocoaErrorDomain)
			XCTAssertTrue([CocoaError.fileNoSuchFile, .fileReadNoSuchFile].contains(.init(rawValue: native.code)))
			XCTAssertEqual(error.toUiText(), .resource(L10n.Error.selectionFailed))
		}

		await controller.openInvitation(from: directory.appendingPathComponent("document.txt"))
		let third = try XCTUnwrap(controller.nextInvitation)
		XCTAssertThrowsError(try controller.takeInvitation(third.id)?.result.get()) { error in
			guard case .invalidInvitationURL? = error as? InvitationError else {
				return XCTFail("Expected a typed unsupported invitation URL error")
			}
		}
	}

	func testValidateInvitationAcceptsValid() {
		guard case .success(let raw) = validateInvitation("some-ticket") else { return XCTFail("expected success") }
		XCTAssertEqual(raw, "some-ticket")
	}

	func testValidateRejectsEmpty() {
		guard case .failure(let error) = validateInvitation("   \n ") else { return XCTFail("expected failure") }
		XCTAssertTrue((error as? InvitationError) != nil)
	}

	func testValidateRejectsTooLarge() {
		let big = String(repeating: "a", count: maxVniDropInvitationBytes + 1)
		guard case .failure = validateInvitation(big) else { return XCTFail("expected failure") }
	}

	func testDecodeInvitationBytesRoundTrip() throws {
		let text = "vnidrop://ticket-abc"
		let decoded = try decodeInvitationBytes(Data(text.utf8))
		XCTAssertEqual(decoded, text)
	}

	func testDecodeRejectsEmptyData() {
		XCTAssertThrowsError(try decodeInvitationBytes(Data()))
	}

	func testDecodeRejectsInvalidUtf8() {
		XCTAssertThrowsError(try decodeInvitationBytes(Data([0xFF, 0xFE, 0xFD])))
	}

	func testInvitationFileNameSanitizes() {
		XCTAssertEqual(invitationFileName("My Photos"), "My-Photos.vnd")
		XCTAssertEqual(invitationFileName("  "), "invitation.vnd")
		XCTAssertTrue(invitationFileName("a/b:c*d").hasSuffix(".vnd"))
	}

	#if os(iOS)
	func testInvitationExportCompletesOnlyAfterDocumentPickerOutcome() throws {
		var presented: UIViewController?
		let actions = IosTransferShareActions(present: { presented = $0 })
		var outcomes: [Result<Void, Error>] = []
		actions.exportInvitation(ticket: "ticket-one", transferName: "Review Export") { outcomes.append($0) }
		XCTAssertTrue(outcomes.isEmpty)
		let confirmed = try XCTUnwrap(presented as? UIDocumentPickerViewController)
		let destination = URL(fileURLWithPath: "/export/Review-Export.vnd")
		confirmed.delegate?.documentPicker?(confirmed, didPickDocumentsAt: [destination])
		confirmed.delegate?.documentPickerWasCancelled?(confirmed)
		XCTAssertEqual(outcomes.count, 1)
		XCTAssertNoThrow(try XCTUnwrap(outcomes.first).get())

		outcomes.removeAll()
		actions.exportInvitation(ticket: "ticket-two", transferName: "Review Export") { outcomes.append($0) }
		XCTAssertTrue(outcomes.isEmpty)
		let cancelled = try XCTUnwrap(presented as? UIDocumentPickerViewController)
		cancelled.delegate?.documentPickerWasCancelled?(cancelled)
		cancelled.delegate?.documentPicker?(cancelled, didPickDocumentsAt: [destination])
		XCTAssertEqual(outcomes.count, 1)
		XCTAssertThrowsError(try XCTUnwrap(outcomes.first).get()) { error in
			XCTAssertTrue(error.isUserCancellation)
		}
	}
	#endif
}
