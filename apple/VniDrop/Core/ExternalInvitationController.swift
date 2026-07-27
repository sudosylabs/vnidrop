import Foundation

let vniDropInvitationMimeType = "application/vnd.vnidrop.transfer"
let vniDropInvitationExtension = "vnd"
let maxVniDropInvitationBytes = 64 * 1024

/// Buffered ingress for invitation documents opened by the OS, ported from
/// `ExternalInvitationController.kt`. Hosts can submit before the UI is attached
/// during a cold launch; each document is consumed exactly once.
@MainActor
final class ExternalInvitationController: ObservableObject {
	/// Emits validated (or failed) invitations. The app-level receive workflow
	/// consumes each exactly once.
	private var continuation: AsyncStream<Result<String, Error>>.Continuation?
	lazy var invitations: AsyncStream<Result<String, Error>> = {
		AsyncStream { continuation in
			self.continuation = continuation
		}
	}()

	func openInvitation(raw: String) {
		continuation?.yield(validateInvitation(raw))
	}

	func reportOpenFailure(message: String) {
		continuation?.yield(.failure(InvitationError.raw(message)))
	}
}

/// Semantic, UI-agnostic invitation/transfer failures. Cases carry no display
/// text: `Error.toUiText()` (UI layer) maps each case to a localized `L10n` key,
/// so there are no free-form English strings to keep in sync or substring-match.
/// `.raw` is the escape hatch for genuinely dynamic system/core messages (e.g. a
/// `CoreNFC` `localizedDescription` or a picker's failure reason), never shown
/// verbatim — it is still routed through `reasonHints`.
enum InvitationError: LocalizedError {
	case empty
	case tooLarge
	case invalidEncoding
	case shareEmpty
	case cancelled
	case coreNotInitialized
	case unsupportedOperation
	case noWindowAvailable
	case viewControllerUnavailable
	case filesystemUnavailable
	case invalidInvitationURL
	case nfcUnavailable
	case nfcFailed
	case cameraUnavailable
	case qrUnavailable
	case bugReportingUnavailable
	case selectionFailed
	case deleteRecordsFailed
	case raw(String)

	/// Developer/log-facing only — never surfaced to users. Derived from the case
	/// so there are no hand-written English blobs; `.raw` passes its payload through.
	var errorDescription: String? {
		if case .raw(let reason) = self { return reason }
		return String(describing: self)
	}
}

func validateInvitation(_ raw: String) -> Result<String, Error> {
	if raw.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
		return .failure(InvitationError.empty)
	}
	if raw.utf8.count > maxVniDropInvitationBytes {
		return .failure(InvitationError.tooLarge)
	}
	return .success(raw)
}

/// Decode invitation document bytes as strict UTF-8, ported from
/// `decodeInvitationBytes`. Rejects payloads that are not lossless UTF-8.
func decodeInvitationBytes(_ bytes: Data) throws -> String {
	guard !bytes.isEmpty else { throw InvitationError.empty }
	guard bytes.count <= maxVniDropInvitationBytes else { throw InvitationError.tooLarge }
	guard let text = String(data: bytes, encoding: .utf8), Data(text.utf8) == bytes else {
		throw InvitationError.invalidEncoding
	}
	guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { throw InvitationError.empty }
	return text
}
