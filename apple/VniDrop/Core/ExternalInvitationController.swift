import Foundation

let vniDropInvitationMimeType = "application/vnd.vnidrop.transfer"
let vniDropInvitationExtension = "vnd"
let maxVniDropInvitationBytes = 64 * 1024

/// Buffered ingress for invitation documents opened by the OS, ported from
/// `ExternalInvitationController.kt`. Hosts can submit before the UI is attached
/// during a cold launch; each document is consumed exactly once.
@MainActor
final class ExternalInvitationController: ObservableObject {
	struct PendingInvitation {
		let id = UUID()
		let result: Result<String, Error>
	}

	private var pending: [PendingInvitation] = []
	private var observers: [UUID: AsyncStream<Void>.Continuation] = [:]

	var nextInvitation: PendingInvitation? { pending.first }

	func observePendingInvitations() -> (UUID, AsyncStream<Void>) {
		let id = UUID()
		let (changes, continuation) = AsyncStream<Void>.makeStream(bufferingPolicy: .bufferingNewest(1))
		observers[id] = continuation
		continuation.yield()
		return (id, changes)
	}

	func stopObserving(_ id: UUID) {
		observers.removeValue(forKey: id)?.finish()
	}

	func takeInvitation(_ id: UUID) -> PendingInvitation? {
		guard pending.first?.id == id else { return nil }
		return pending.removeFirst()
	}

	func openInvitation(raw: String) {
		enqueue(validateInvitation(raw))
	}

	func reportOpenFailure(message: String) {
		enqueue(.failure(InvitationError.raw(message)))
	}

	private func enqueue(_ result: Result<String, Error>) {
		pending.append(PendingInvitation(result: result))
		for observer in observers.values { observer.yield() }
	}
}

/// Semantic, UI-agnostic invitation/transfer failures. Cases carry no display
/// text: `Error.toUiText()` (UI layer) maps each case to a localized `L10n` key,
/// so there are no free-form English strings to keep in sync or substring-match.
/// `.raw` carries dynamic system/core messages, such as a picker's failure reason.
/// They are routed through `reasonHints` rather than shown verbatim.
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

func readInvitationFile(_ url: URL) throws -> String {
	let started = url.startAccessingSecurityScopedResource()
	defer { if started { url.stopAccessingSecurityScopedResource() } }
	let file = try FileHandle(forReadingFrom: url)
	defer { try? file.close() }
	var bytes = Data()
	while bytes.count <= maxVniDropInvitationBytes,
	      let chunk = try file.read(upToCount: maxVniDropInvitationBytes + 1 - bytes.count),
	      !chunk.isEmpty {
		bytes.append(chunk)
	}
	return try decodeInvitationBytes(bytes)
}
