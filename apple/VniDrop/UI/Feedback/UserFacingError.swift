import Foundation
import VnidropCore

/// Maps technical failures to stable, user-facing catalog keys. Ported from
/// `ui/feedback/UserFacingError.kt`. Never exposes raw `reason=` blobs.
extension Error {
	func toUiText() -> UiText {
		if let invitation = self as? InvitationError {
			return invitation.uiText
		}
		if let vni = self as? VnidropError {
			switch vni {
			case .Ticket:
				return .resource(L10n.Error.invalidTicket)
			case .Permission:
				return .resource(L10n.Error.permission)
			case .Filesystem:
				return .resource(L10n.Error.filesystem)
			case .FilesystemPermission:
				return .resource(L10n.Error.filesystem)
			case .DestinationExists:
				return .resource(L10n.Error.destinationExists)
			case .StorageFull:
				return .resource(L10n.Error.storageFull)
			case .Network:
				return .resource(L10n.Error.network)
			case .Transfer(let reason):
				return transferUiText(reason)
			case .Repository:
				return .resource(L10n.Error.repository)
			case .Cancelled:
				return .resource(L10n.Error.generic)
			case .InvalidInput:
				return .resource(L10n.Error.invalidInput)
			case .Initialization(let reason):
				return initializationUiText(reason)
			case .Internal(let reason):
				return reasonHints(reason) ?? .resource(L10n.Error.generic)
			}
		}
		return reasonHints(technicalDetail) ?? .resource(L10n.Error.generic)
	}

	/// True when the user intentionally backed out of a flow.
	var isUserCancellation: Bool {
		if let invitation = self as? InvitationError, case .cancelled = invitation { return true }
		if let vni = self as? VnidropError, case .Cancelled = vni { return true }
		let haystack = technicalDetail.lowercased()
		if haystack.isEmpty {
			// URLError / CocoaError cancellation without a message.
			if let urlError = self as? URLError, urlError.code == .cancelled { return true }
			return (self as NSError).code == NSUserCancelledError
		}
		return haystack.contains("cancelled")
			|| haystack.contains("canceled")
			|| haystack.contains("user cancelled")
			|| haystack.contains("user canceled")
	}

	/// Prefers a `VnidropError` reason; else the localized description.
	var technicalDetail: String {
		if let vni = self as? VnidropError {
			switch vni {
			case .Initialization(let r), .Ticket(let r), .Filesystem(let r), .FilesystemPermission(let r),
				 .DestinationExists(let r), .StorageFull(let r), .Network(let r),
				 .Transfer(let r), .Permission(let r), .Repository(let r), .Cancelled(let r),
				 .InvalidInput(let r), .Internal(let r):
				return r
			}
		}
		return (self as? LocalizedError)?.errorDescription ?? (self as NSError).localizedDescription
	}

	var canRetryWithoutChangingInput: Bool {
		guard let vni = self as? VnidropError else { return true }
		switch vni {
		case .FilesystemPermission, .DestinationExists, .InvalidInput: return false
		default: return true
		}
	}
}

/// Maps each semantic `InvitationError` case to a localized user-facing message.
/// This is the sole `InvitationError` → `L10n` boundary: no substring guessing,
/// except for `.raw`, whose dynamic payload still falls through `reasonHints`.
extension InvitationError {
	var uiText: UiText {
		switch self {
		case .empty:
			return .resource(L10n.Error.invitationEmpty)
		case .tooLarge, .unsupportedOperation, .noWindowAvailable,
			 .viewControllerUnavailable, .qrUnavailable, .bugReportingUnavailable, .cancelled:
			return .resource(L10n.Error.generic)
		case .invalidEncoding, .invalidInvitationURL:
			return .resource(L10n.Error.invalidTicket)
		case .shareEmpty:
			return .resource(L10n.Error.shareEmpty)
		case .coreNotInitialized:
			return .resource(L10n.Error.startingUp)
		case .filesystemUnavailable:
			return .resource(L10n.Error.filesystem)
		case .nfcUnavailable, .nfcFailed:
			return .resource(L10n.Error.nfc)
		case .cameraUnavailable:
			return .resource(L10n.Error.camera)
		case .selectionFailed:
			return .resource(L10n.Error.selectionFailed)
		case .deleteRecordsFailed:
			return .resource(L10n.Error.repository)
		case .raw(let reason):
			return reasonHints(reason) ?? .resource(L10n.Error.generic)
		}
	}
}

/// Maps a receiver delivery/refusal reason code to a user-facing message, never
/// surfacing the raw core code (e.g. `destination_exists`). Unknown codes fall back
/// to the substring hints, then a generic message.
func receiverReasonUiText(_ reason: String) -> UiText {
	switch reason {
	case "destination_exists":
		return .resource(L10n.Error.destinationExists)
	case "filesystem", "filesystem_permission_denied":
		return .resource(L10n.Error.filesystem)
	case "permission_denied", "approval-required", "approval-expired",
		 "unknown-transfer", "missing-endpoint-id", "invalid-receipt":
		return .resource(L10n.Error.permission)
	case "storage_full":
		return .resource(L10n.Error.storageFull)
	case "network":
		return .resource(L10n.Error.network)
	case "invalid_ticket":
		return .resource(L10n.Error.invalidTicket)
	case "transfer":
		return .resource(L10n.Error.transfer)
	case "repository", "repository-error":
		return .resource(L10n.Error.repository)
	case "invalid_input":
		return .resource(L10n.Error.invalidInput)
	case "initialization":
		return .resource(L10n.Error.initialization)
	case "cancelled", "internal":
		return .resource(L10n.Error.generic)
	default:
		return reasonHints(reason) ?? .resource(L10n.Error.generic)
	}
}

private func transferUiText(_ reason: String) -> UiText {
	let detail = reason.lowercased()
	if detail.contains("refused") || detail.contains("denied") || detail.contains("not approved") {
		return .resource(L10n.Error.permission)
	}
	return .resource(L10n.Error.transfer)
}

private func initializationUiText(_ reason: String) -> UiText {
	let detail = reason.lowercased()
	if detail.contains("native") && detail.contains("library") {
		return .resource(L10n.Error.missingNativeLibrary)
	}
	if detail.contains("socket") || detail.contains("bind") {
		return .resource(L10n.Error.socketBind)
	}
	return .resource(L10n.Error.initialization)
}

private func reasonHints(_ detailRaw: String) -> UiText? {
	let detail = detailRaw.lowercased()
	if detail.isEmpty { return nil }

	if detail.contains("still starting") || detail.contains("starting up") {
		return .resource(L10n.Error.startingUp)
	}
	if detail.contains("empty") && (detail.contains("invitation") || detail.contains("ticket") || detail.contains("qr")) {
		return .resource(L10n.Error.invitationEmpty)
	}
	if detail.contains("select at least one") || detail.contains("no files found") {
		return .resource(L10n.Error.shareEmpty)
	}
	if detail.contains("camera") {
		return .resource(L10n.Error.camera)
	}
	if detail.contains("nfc") || detail.contains("ndef")
		|| (detail.contains("read-only") && detail.contains("tag"))
		|| detail.contains("tag is too small") || detail.contains("no nfc tag") {
		return .resource(L10n.Error.nfc)
	}
	if detail.contains("native") && detail.contains("library") {
		return .resource(L10n.Error.missingNativeLibrary)
	}
	if detail.contains("socket") || detail.contains("bind") {
		return .resource(L10n.Error.socketBind)
	}
	if detail.contains("device information") || detail.contains("device info") {
		return .resource(L10n.Error.deviceInfo)
	}
	if detail.contains("refused") || detail.contains("denied") || detail.contains("permission")
		|| detail.contains("not approved") || detail.contains("waiting for approval") {
		return .resource(L10n.Error.permission)
	}
	if detail.contains("invalid ticket") || detail.contains("ticket error")
		|| detail.contains("could not be read") || detail.contains("malformed")
		|| detail.contains("invitation could not be opened") {
		return .resource(L10n.Error.invalidTicket)
	}
	if detail.contains("selected")
		&& (detail.contains("file") || detail.contains("folder") || detail.contains("document") || detail.contains("open")) {
		return .resource(L10n.Error.selectionFailed)
	}
	if detail.contains("could not open the selected") || detail.contains("could not open selected") {
		return .resource(L10n.Error.selectionFailed)
	}
	if detail.contains("document picker") || detail.contains("folder picker") || detail.contains("file descriptor")
		|| detail.contains("view controller") {
		return .resource(L10n.Error.selectionFailed)
	}
	return nil
}
