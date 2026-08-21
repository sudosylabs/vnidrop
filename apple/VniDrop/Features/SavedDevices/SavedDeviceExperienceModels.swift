import Foundation

/// Presentation-level saved-device models, ported from
/// `feature/saveddevices/SavedDeviceExperienceModels.kt`.

/// The one consent question currently worth asking. Only one is shown at a time:
/// an incoming request always outranks an eligibility we could act on ourselves.
enum PairingPrompt: Equatable, Identifiable {
	/// We completed a qualifying transfer and may ask this peer to pair.
	case eligibility(peerEndpointId: String, remoteDisplayName: String?)
	/// This peer asked us; we approve or decline.
	case incomingRequest(peerEndpointId: String, remoteDisplayName: String?)

	var peerEndpointId: String {
		switch self {
		case .eligibility(let id, _), .incomingRequest(let id, _): return id
		}
	}

	var remoteDisplayName: String? {
		switch self {
		case .eligibility(_, let name), .incomingRequest(_, let name): return name
		}
	}

	var id: String {
		switch self {
		case .eligibility(let id, _): return "eligibility-\(id)"
		case .incomingRequest(let id, _): return "incoming-\(id)"
		}
	}
}

struct PairingPromptState: Equatable {
	var prompt: PairingPrompt?
	var busy = false
}

struct TargetedOfferState: Equatable {
	var pending: [PendingTargetedOfferModel] = []
	/// Display names for senders we already have saved. A sender we have not
	/// saved has no trustworthy name, so the UI falls back to a generic label
	/// rather than rendering a peer-supplied string as if it were verified.
	var senderDisplayNames: [String: String] = [:]
	var respondingIds: Set<String> = []

	var current: PendingTargetedOfferModel? { pending.first }

	var currentSenderDisplayName: String? {
		guard let current else { return nil }
		return senderDisplayNames[current.senderEndpointId]
	}
}

enum SavedDeviceTransferDirection: Equatable, Sendable {
	case outgoing
	case incoming
}

enum SavedDeviceTransferAction: Equatable, Sendable {
	case receive
	case resume
	case cancel
	case delete
}

/// One targeted transfer as the details surface renders it: resolved against the
/// local endpoint so "peer" means the *other* device, whichever side we are on.
struct SavedDeviceTransferItem: Equatable, Identifiable, Sendable {
	let id: String
	let peerEndpointId: String
	let peerDisplayName: String?
	let direction: SavedDeviceTransferDirection
	let transferName: String
	let fileCount: UInt64
	let totalSize: UInt64
	let verifiedBytes: UInt64
	let state: TargetedTransferStateModel
	let createdAt: Int64
	let updatedAt: Int64
	let availableActions: [SavedDeviceTransferAction]
	let progressFraction: Double?

	init(
		id: String,
		peerEndpointId: String,
		peerDisplayName: String?,
		direction: SavedDeviceTransferDirection,
		transferName: String,
		fileCount: UInt64,
		totalSize: UInt64,
		verifiedBytes: UInt64,
		state: TargetedTransferStateModel,
		createdAt: Int64,
		updatedAt: Int64
	) {
		self.id = id
		self.peerEndpointId = peerEndpointId
		self.peerDisplayName = peerDisplayName
		self.direction = direction
		self.transferName = transferName
		self.fileCount = fileCount
		self.totalSize = totalSize
		self.verifiedBytes = verifiedBytes
		self.state = state
		self.createdAt = createdAt
		self.updatedAt = updatedAt
		self.availableActions = Self.actions(direction: direction, state: state)
		self.progressFraction = Self.progress(
			direction: direction,
			state: state,
			verifiedBytes: verifiedBytes,
			totalSize: totalSize
		)
	}

	private static func actions(
		direction: SavedDeviceTransferDirection,
		state: TargetedTransferStateModel
	) -> [SavedDeviceTransferAction] {
		var actions: [SavedDeviceTransferAction] = []
		if direction == .incoming {
			switch state {
			case .approved: actions.append(.receive)
			case .interrupted: actions.append(.resume)
			default: break
			}
		}
		switch state {
		case .preparing, .offering, .awaitingApproval, .approved,
				.connecting, .transferring, .interrupted:
			actions.append(.cancel)
		case .completed, .declined, .cancelled, .failed:
			actions.append(.delete)
		case .deleted:
			break
		}
		return actions
	}

	private static func progress(
		direction: SavedDeviceTransferDirection,
		state: TargetedTransferStateModel,
		verifiedBytes: UInt64,
		totalSize: UInt64
	) -> Double? {
		guard direction == .incoming, totalSize > 0 else { return nil }
		guard state == .connecting || state == .transferring || state == .interrupted else {
			return nil
		}
		return min(1, Double(verifiedBytes) / Double(totalSize))
	}
}
