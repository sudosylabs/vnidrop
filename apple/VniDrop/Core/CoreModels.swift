import Foundation

/// App-facing domain models, ported from `core/CoreModels.kt`. The repository maps
/// the generated UniFFI records/enums into these so the UI never depends on the
/// binding surface directly.

struct CoreStatus: Equatable, Sendable {
	let endpointId: String
	let activeTransfers: UInt64
	let activeShares: UInt64
}

struct CoreEventModel: Equatable, Identifiable, Sendable {
	let id: String
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
///
/// A contact destination deliberately carries no access policy: the core forces
/// approval-required for offers, so exposing the choice here would imply a
/// setting that does not exist.
enum ShareDestination: Equatable, Sendable {
	case invitation(accessPolicy: ShareAccessPolicy)
	case contact(endpointId: String)
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
	/// Device history changed: a contact was added, forgotten, or blocked.
	case contactsChanged
	/// An incoming offer arrived or was answered.
	case offersChanged
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

// MARK: - Device history

/// A device the user has chosen to remember.
///
/// `localLabel` is the user's own name for the device and is authoritative for
/// display; `remoteDisplayName` is whatever the device last called itself and is
/// untrusted. The endpoint id is the only real identity.
struct DeviceContact: Equatable, Identifiable, Sendable {
	let endpointId: String
	let localLabel: String?
	let remoteDisplayName: String?
	let lastTransferAt: Int64?
	let createdAt: Int64
	/// Whether a live grant is held. False once the peer revoked, the grant
	/// lapsed, or the peer reinstalled and lost its identity.
	let canSend: Bool

	var id: String { endpointId }

	/// Name to show, preferring the local label the peer cannot influence.
	var displayName: String {
		if let localLabel, !localLabel.isEmpty { return localLabel }
		if let remoteDisplayName, !remoteDisplayName.isEmpty { return remoteDisplayName }
		return String(localized: L10n.Approval.nearbyDevice)
	}

	/// Short prefix of the endpoint id, for telling apart devices claiming the
	/// same name.
	var shortFingerprint: String { String(endpointId.prefix(8)) }
}

/// A device offering to be remembered, awaiting this user's decision.
struct PendingPairingModel: Equatable, Identifiable, Sendable {
	let endpointId: String
	let displayName: String?
	let receivedAt: Int64

	var id: String { endpointId }

	var resolvedName: String {
		guard let displayName, !displayName.isEmpty else {
			return String(localized: L10n.Approval.nearbyDevice)
		}
		return displayName
	}
}

/// A transfer a remembered device is offering. Carries no ticket: that is a
/// capability and the core releases it only once the user accepts.
struct IncomingOfferModel: Equatable, Identifiable, Sendable {
	let offerId: String
	let fromEndpointId: String
	let senderDisplayName: String?
	let transferName: String
	let fileCount: UInt64
	let totalBytes: UInt64
	let receivedAt: Int64

	var id: String { offerId }

	var resolvedSenderName: String {
		guard let senderDisplayName, !senderDisplayName.isEmpty else {
			return String(localized: L10n.Approval.nearbyDevice)
		}
		return senderDisplayName
	}
}

/// How long a remembered device stays reachable while unused. The countdown
/// restarts on every transfer.
enum GrantLifetimeOption: String, CaseIterable, Identifiable, Sendable {
	case days30
	case days90
	case days365
	case never

	var id: String { rawValue }

	var days: Int? {
		switch self {
		case .days30: return 30
		case .days90: return 90
		case .days365: return 365
		case .never: return nil
		}
	}
}
