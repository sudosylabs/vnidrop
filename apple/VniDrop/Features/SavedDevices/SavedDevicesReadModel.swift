import Foundation

struct SavedDevicesReadInputs: Equatable {
	let eligibilities: [PairingEligibilityModel]
	let relationships: [DeviceRelationshipModel]
	let savedDevices: [SavedDeviceModel]
	let pendingOffers: [PendingTargetedOfferModel]
	let targetedTransfers: [TargetedTransferModel]

	init(
		eligibilities: [PairingEligibilityModel] = [],
		relationships: [DeviceRelationshipModel] = [],
		savedDevices: [SavedDeviceModel] = [],
		pendingOffers: [PendingTargetedOfferModel] = [],
		targetedTransfers: [TargetedTransferModel] = []
	) {
		self.eligibilities = eligibilities
		self.relationships = relationships
		self.savedDevices = savedDevices
		self.pendingOffers = pendingOffers
		self.targetedTransfers = targetedTransfers
	}
}

struct SavedDevicesReadSnapshot: Equatable {
	let eligibilities: [PairingEligibilityModel]
	let pendingRelationships: [DeviceRelationshipModel]
	let savedDevices: [SavedDeviceModel]
	let pendingOffers: [PendingTargetedOfferModel]
	let targetedTransfers: [SavedDeviceTransferItem]
	let senderDisplayNames: [String: String]
	let nextPairingPrompt: PairingPrompt?
}

/// Derives stable Saved Devices presentation facts from authoritative core
/// reads. Core events only trigger another read; they are deliberately absent.
struct SavedDevicesReadModel {
	func derive(
		_ inputs: SavedDevicesReadInputs,
		dismissedEligibilityIds: Set<String> = [],
		hiddenTransferIds: Set<String> = []
	) -> SavedDevicesReadSnapshot {
		let savedNames = inputs.savedDevices.reduce(into: [String: String]()) { names, device in
			if let name = device.displayNameOrNil { names[device.endpointId] = name }
		}
		let pendingRelationships = inputs.relationships
			.filter { $0.state == .pendingIncoming || $0.state == .pendingOutgoing }
			.sorted { $0.updatedAt > $1.updatedAt }
		let eligibilities = inputs.eligibilities.sorted { $0.createdAt > $1.createdAt }

		return SavedDevicesReadSnapshot(
			eligibilities: eligibilities,
			pendingRelationships: pendingRelationships,
			savedDevices: inputs.savedDevices.sorted { $0.createdAt > $1.createdAt },
			pendingOffers: inputs.pendingOffers.sorted { $0.receivedAt < $1.receivedAt },
			targetedTransfers: inputs.targetedTransfers
				.filter { $0.state != .deleted && !hiddenTransferIds.contains($0.id) }
				.sorted { $0.updatedAt > $1.updatedAt }
				.map { transferItem($0, savedNames: savedNames) },
			senderDisplayNames: savedNames,
			nextPairingPrompt: nextPairingPrompt(
				pendingRelationships,
				eligibilities: eligibilities,
				savedNames: savedNames,
				dismissedEligibilityIds: dismissedEligibilityIds
			)
		)
	}

	private func nextPairingPrompt(
		_ relationships: [DeviceRelationshipModel],
		eligibilities: [PairingEligibilityModel],
		savedNames: [String: String],
		dismissedEligibilityIds: Set<String>
	) -> PairingPrompt? {
		if let incoming = relationships.first(where: { $0.state == .pendingIncoming }) {
			let name = eligibilities
				.first { $0.peerEndpointId == incoming.remoteEndpointId }?
				.remoteDisplayName
				?? savedNames[incoming.remoteEndpointId]
			return .incomingRequest(
				peerEndpointId: incoming.remoteEndpointId,
				remoteDisplayName: name
			)
		}
		guard let eligibility = eligibilities.first(where: {
			!dismissedEligibilityIds.contains($0.peerEndpointId)
		}) else { return nil }
		return .eligibility(
			peerEndpointId: eligibility.peerEndpointId,
			remoteDisplayName: eligibility.remoteDisplayName
		)
	}

	private func transferItem(
		_ transfer: TargetedTransferModel,
		savedNames: [String: String]
	) -> SavedDeviceTransferItem {
		let outgoing = transfer.role == .sender
		let peerEndpointId = outgoing ? transfer.receiverEndpointId : transfer.senderEndpointId
		return SavedDeviceTransferItem(
			id: transfer.id,
			peerEndpointId: peerEndpointId,
			peerDisplayName: savedNames[peerEndpointId],
			direction: outgoing ? .outgoing : .incoming,
			transferName: transfer.transferName,
			fileCount: transfer.fileCount,
			totalSize: transfer.totalSize,
			verifiedBytes: transfer.verifiedBytes,
			state: transfer.state,
			createdAt: transfer.createdAt,
			updatedAt: transfer.updatedAt
		)
	}
}
