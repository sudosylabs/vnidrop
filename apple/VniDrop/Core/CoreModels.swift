import Foundation

/// App-facing domain models, ported from `core/CoreModels.kt`. The repository maps
/// the generated UniFFI records/enums into these so the UI never depends on the
/// binding surface directly.

struct CoreStatus: Equatable, Sendable {
	let endpointId: String
	let activeTransfers: UInt64
	let activeShares: UInt64
}

struct RuntimeObligationFactsModel: Equatable, Sendable {
	let activeInvitationTransfers: UInt64
	let invitationProviderAvailability: UInt64
	let targetedPreparations: UInt64
	let activeTargetedTransfers: UInt64
	let targetedProviderAvailability: UInt64

	var requiresRuntime: Bool {
		activeInvitationTransfers > 0
			|| invitationProviderAvailability > 0
			|| targetedPreparations > 0
			|| activeTargetedTransfers > 0
			|| targetedProviderAvailability > 0
	}
}

struct CoreEventModel: Equatable, Identifiable, Sendable {
	let id: String
	let revision: UInt64
	let timestamp: Int64
	let scope: String
	let transferId: UInt64?
	/// Raw wire values as emitted by the core. Interpret them through the typed
	/// `eventDirection` / `eventPhase` / `eventKind` accessors below — logic code
	/// should never compare these strings directly.
	let direction: String?
	let phase: String
	let kind: String
	let dataJson: String

	var eventDirection: EventDirection? { direction.flatMap(EventDirection.init(rawValue:)) }
	var eventPhase: EventPhase? { EventPhase(rawValue: phase) }
	var eventKind: EventKind? { EventKind(rawValue: kind) }

}

/// Direction of a core event, matching the wire strings the core emits.
enum EventDirection: String, Equatable, Sendable {
	case send
	case receive
}

/// Phase of a core progress event (the `phase` wire field).
enum EventPhase: String, Equatable, Sendable {
	case importing = "import"
	case ticket
	case access
	case transfer
	case download
	case export
	case lifecycle
	case network
	case handshake
	case error
	/// Saved-device consent lifecycle (eligibility, relationships, grants).
	case pairing
	/// Targeted-transfer offer and lifecycle. Its events identify the transfer by
	/// a string `targeted_transfer_id`, not the numeric `transferId` used by
	/// invitation shares.
	case targetedTransfer = "targeted_transfer"
	/// Neutral wake-up indicating that lifecycle retention facts changed.
	case runtimeObligation = "runtime_obligation"
}

/// Kind of a core progress event (the `kind` wire field).
enum EventKind: String, Equatable, Sendable {
	case started
	case copyProgress = "copy-progress"
	case copyDone = "copy-done"
	case outboardProgress = "outboard-progress"
	case done
	case created
	case progress
	case completed
	case aborted
	case failed
	case connecting
	case connected
	case foundCollection = "found-collection"
	case cancelled
	case shareStopped = "share-stopped"
}

enum ShareAccessPolicy: Equatable, Sendable {
	case requireApproval
	case anyoneWithTransfer
}

/// Where a picked selection is going.
enum ShareDestination: Equatable, Sendable {
	case invitation(accessPolicy: ShareAccessPolicy)
}

enum TransferDirection: Equatable, Sendable {
	case send
	case receive
}

enum TransferStatus: Equatable, Sendable {
	case importing
	case sharing
	case receiving
	case done
	case failed
	case cancelled
	case stopped
}

struct Transfer: Equatable, Identifiable, Sendable {
	let localId: String
	let transferId: UInt64
	let direction: TransferDirection
	let status: TransferStatus
	let peerId: String?
	let transferName: String?
	let contentHash: String?
	let fileCount: UInt64
	let totalSize: UInt64
	let ticket: String?
	let accessPolicy: ShareAccessPolicy
	let createdAt: Int64
	let updatedAt: Int64

	var id: String { localId }
}

struct Share: Equatable, Sendable {
	let transferId: UInt64
	let ticket: String
	let transferName: String
	let contentHash: String
	let fileCount: UInt64
	let totalSize: UInt64
}

struct TransferMetadataModel: Equatable, Sendable {
	let transferId: UInt64
	let transferName: String
	let senderName: String?
	let contentHash: String
	let fileCount: UInt64
	let totalSize: UInt64
}

struct TicketInspectionModel: Equatable, Sendable {
	let kind: String
	let metadata: TransferMetadataModel
}

enum ReceiverDeliveryStatus: Equatable, Sendable {
	case requested
	case accepted
	case refused
	case expired
	case completed
	case failed
	case unknown
}

struct ReceiverRequestModel: Equatable, Identifiable, Sendable {
	let id: String
	let transferId: UInt64
	let remoteEndpointId: String
	let transferName: String
	let receiverName: String?
	let receiverDeviceName: String?
	let appVersion: String
	let status: ReceiverDeliveryStatus
	let reason: String?
	let requestedAt: Int64
	let respondedAt: Int64?
	let completedAt: Int64?
}

struct CoreState: Equatable, Sendable {
	var isInitialized: Bool = false
	var status: CoreStatus?
	var events: [CoreEventModel] = []
	var transfers: [Transfer] = []
	var lastShare: Share?
	var lastInspection: TicketInspectionModel?
}

/// Coalesced change hints emitted from the event sink, ported from `CoreSignal`.
enum CoreSignal: Equatable, Sendable {
	case approvalChanged(transferId: UInt64)
	case receiverHistoryChanged(transferId: UInt64)
	/// Transfer status/history changed enough to re-read the durable snapshot.
	case transfersChanged(transferId: UInt64)
	/// Pairing / saved-device state changed; refresh eligibility, relationships,
	/// and the saved list. Carries no payload: core events are wake-ups, not
	/// authoritative state, so consumers re-query rather than apply a delta.
	case pairingChanged
	/// Targeted-transfer offer or lifecycle changed; refresh pending offers and
	/// transfers. Payload-free for the same reason as `pairingChanged`.
	case targetedTransferChanged
	/// Runtime-retention facts changed; consumers re-read the neutral snapshot.
	case runtimeObligationChanged
}

// MARK: - Transfer helpers (ported from AppUiModels.kt)

extension TransferStatus {
	var isActiveTransfer: Bool {
		self == .importing || self == .sharing || self == .receiving
	}

	var canCancelTransfer: Bool {
		self == .importing || self == .sharing || self == .receiving
	}

	/// Terminal receive-history states eligible for deletion.
	var isTerminalReceiveHistory: Bool {
		self == .done || self == .failed || self == .cancelled
	}
}
