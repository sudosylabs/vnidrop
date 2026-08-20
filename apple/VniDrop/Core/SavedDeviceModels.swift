import Foundation

/// App-facing saved-device domain models, ported from `core/SavedDeviceModels.kt`.
/// The repository maps the generated UniFFI records into these so the UI never
/// depends on the binding surface directly.
///
/// A saved device is a remote VniDrop *app-installation identity*, not a person,
/// account, or piece of hardware. Display names and platform hints are untrusted
/// peer-supplied hints and must never be used to merge or match identities.

struct SavedDeviceModel: Equatable, Identifiable, Sendable {
	/// The remote iroh endpoint identity. Stable, cryptographic, and the only
	/// safe way to identify a peer.
	let endpointId: String
	/// User-owned local label. Takes precedence over `remoteDisplayName`.
	let localLabel: String?
	/// Untrusted display-name hint supplied by the peer.
	let remoteDisplayName: String?
	let createdAt: Int64
	let lastAuthenticatedAt: Int64?

	var id: String { endpointId }

	/// The name to show, or nil when neither side supplied one. Callers fall back
	/// to `L10n.SavedDevices.unnamed` (see `SavedDeviceTransferHistory.kt`).
	var displayNameOrNil: String? {
		if let localLabel, !localLabel.trimmed.isEmpty { return localLabel }
		if let remoteDisplayName, !remoteDisplayName.trimmed.isEmpty { return remoteDisplayName }
		return nil
	}
}

/// Durable consent lifecycle for one remote app-installation identity. A
/// relationship is usable only in `saved`; the pending states are bounded
/// operations that cannot initiate a transfer.
enum DeviceRelationshipStateModel: Equatable, Sendable {
	case pendingOutgoing
	case pendingIncoming
	case saved
	case revoked
	case blocked
}

struct DeviceRelationshipModel: Equatable, Identifiable, Sendable {
	let remoteEndpointId: String
	let state: DeviceRelationshipStateModel
	let generation: UInt64
	let minimumProtocolVersion: UInt16
	let createdAt: Int64
	let updatedAt: Int64

	var id: String { remoteEndpointId }
}

/// Single-use permission to *ask* to pair, created by a fully completed
/// authenticated transfer and expiring 24 hours later. Consumed by pairing,
/// declining, expiry, forget, block, or reset.
struct PairingEligibilityModel: Equatable, Identifiable, Sendable {
	let peerEndpointId: String
	/// Untrusted display-name hint from the qualifying transfer. Usually the only
	/// name available for a peer that is not saved yet.
	let remoteDisplayName: String?
	let sessionId: String
	let protocolVersion: UInt16
	let createdAt: Int64
	let expiresAt: Int64

	var id: String { peerEndpointId }
}

/// A pre-approval targeted offer awaiting a local approve/decline. Lives only in
/// the core's bounded live-session queue — a restart, timeout, disconnect, or
/// sender cancellation removes it, so it is never durable UI state.
struct PendingTargetedOfferModel: Equatable, Identifiable, Sendable {
	let transferId: String
	let senderEndpointId: String
	let receiverEndpointId: String
	let manifestId: String
	let contentHash: String
	/// Peer-supplied and untrusted; render it as text, never as a path.
	let transferName: String
	let fileCount: UInt64
	let totalSize: UInt64
	let protocolVersion: UInt16
	let receivedAt: Int64

	var id: String { transferId }
}

/// Durable targeted-transfer lifecycle. Rust validates every transition; the UI
/// only invokes typed operations and renders the snapshot it gets back.
enum TargetedTransferStateModel: Equatable, Sendable {
	case preparing
	case offering
	case awaitingApproval
	case approved
	case connecting
	case transferring
	case interrupted
	case completed
	case declined
	case cancelled
	case failed
	case deleted
}

/// This installation's immutable side of a targeted transfer.
///
/// Authoritative for direction. Comparing `senderEndpointId` to the local
/// endpoint is not: an identity reset retires that endpoint, after which rows
/// predating it match neither side and past sends read as incoming from the
/// device's own former identity.
enum TargetedTransferRoleModel: Equatable, Sendable {
	case sender
	case receiver
}

struct TargetedTransferModel: Equatable, Identifiable, Sendable {
	let id: String
	let role: TargetedTransferRoleModel
	let senderEndpointId: String
	let receiverEndpointId: String
	let manifestId: String
	let contentHash: String
	/// Peer-supplied and untrusted; render it as text, never as a path.
	let transferName: String
	let fileCount: UInt64
	let totalSize: UInt64
	/// Bytes verified so far; survives interruption for resume.
	let verifiedBytes: UInt64
	let state: TargetedTransferStateModel
	let createdAt: Int64
	let updatedAt: Int64
}

/// Outcome of responding to a targeted offer. `alreadySettled` is the idempotent
/// replay path — the core returns the existing result rather than creating a
/// duplicate approval.
enum TargetedOfferResponseModel: Equatable, Sendable {
	case approved(transferId: String)
	case declined
	case alreadySettled(transferId: String)
}

// MARK: - Lifecycle helpers

extension TargetedTransferStateModel {
	/// States where bytes may still move, so progress is meaningful.
	var isActive: Bool {
		switch self {
		case .connecting, .transferring: return true
		default: return false
		}
	}

	/// No further transition is possible without creating a new transfer.
	var isTerminal: Bool {
		switch self {
		case .completed, .declined, .cancelled, .failed, .deleted: return true
		default: return false
		}
	}

	/// Cancellation withdraws the offer before approval and stops authorization
	/// plus active streaming after it. Terminal transfers have nothing to stop.
	var canCancel: Bool { !isTerminal }

	/// An interrupted transfer keeps its verified progress and resumes the same
	/// immutable transfer without asking for approval again.
	var canResume: Bool { self == .interrupted }

	/// The receiver pulls content once the sender's authorization is in place.
	var canReceive: Bool { self == .approved }

	/// Deletion makes authorization unusable and removes resumable state. Offered
	/// on anything already terminal except an entry that is itself deleted.
	var canDelete: Bool { isTerminal && self != .deleted }
}

extension TargetedTransferModel {
	/// Fraction of verified payload in `0...1`, or nil when the total is unknown
	/// or the state carries no meaningful progress.
	var progressFraction: Double? {
		guard totalSize > 0, state.isActive || state == .interrupted else { return nil }
		return min(1, Double(verifiedBytes) / Double(totalSize))
	}
}

extension PairingEligibilityModel {
	func isExpired(now: Int64) -> Bool { now >= expiresAt }
}

private extension String {
	var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
}
