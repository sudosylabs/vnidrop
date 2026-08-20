package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.core.TargetedTransferModel
import com.vnidrop.app.core.TargetedTransferRoleModel
import com.vnidrop.app.core.TargetedTransferStateModel

internal data class SavedDevicesReadInputs(
	val eligibilities: List<PairingEligibilityModel> = emptyList(),
	val relationships: List<DeviceRelationshipModel> = emptyList(),
	val savedDevices: List<SavedDeviceModel> = emptyList(),
	val pendingOffers: List<PendingTargetedOfferModel> = emptyList(),
	val targetedTransfers: List<TargetedTransferModel> = emptyList(),
)

internal data class SavedDevicesReadSnapshot(
	val eligibilities: List<PairingEligibilityModel>,
	val pendingRelationships: List<DeviceRelationshipModel>,
	val savedDevices: List<SavedDeviceModel>,
	val pendingOffers: List<PendingTargetedOfferModel>,
	val targetedTransfers: List<SavedDeviceTransferItem>,
	val senderDisplayNames: Map<String, String>,
	val nextPairingPrompt: PairingPrompt?,
)

/**
 * Derives stable Saved Devices presentation facts from authoritative core reads.
 * Core events only trigger another read; they are deliberately absent here.
 */
internal class SavedDevicesReadModel {
	fun derive(
		inputs: SavedDevicesReadInputs,
		dismissedEligibilityIds: Set<String> = emptySet(),
	): SavedDevicesReadSnapshot {
		val savedNames = buildMap {
			inputs.savedDevices.forEach { device ->
				device.displayNameOrNull()?.let { name -> put(device.endpointId, name) }
			}
		}
		val pendingRelationships = inputs.relationships
			.filter { relationship ->
				relationship.state == DeviceRelationshipStateModel.PendingIncoming ||
					relationship.state == DeviceRelationshipStateModel.PendingOutgoing
			}
			.sortedByDescending(DeviceRelationshipModel::updatedAt)
		val eligibilities = inputs.eligibilities.sortedByDescending(PairingEligibilityModel::createdAt)

		return SavedDevicesReadSnapshot(
			eligibilities = eligibilities,
			pendingRelationships = pendingRelationships,
			savedDevices = inputs.savedDevices.sortedByDescending(SavedDeviceModel::createdAt),
			pendingOffers = inputs.pendingOffers.sortedBy(PendingTargetedOfferModel::receivedAt),
			targetedTransfers = inputs.targetedTransfers
				.filter { transfer -> transfer.state != TargetedTransferStateModel.Deleted }
				.sortedByDescending(TargetedTransferModel::updatedAt)
				.map { transfer -> transfer.toExperienceItem(savedNames) },
			senderDisplayNames = savedNames,
			nextPairingPrompt = nextPairingPrompt(
				pendingRelationships,
				eligibilities,
				savedNames,
				dismissedEligibilityIds,
			),
		)
	}

	private fun nextPairingPrompt(
		relationships: List<DeviceRelationshipModel>,
		eligibilities: List<PairingEligibilityModel>,
		savedNames: Map<String, String>,
		dismissedEligibilityIds: Set<String>,
	): PairingPrompt? {
		val incoming = relationships.firstOrNull { it.state == DeviceRelationshipStateModel.PendingIncoming }
		if (incoming != null) {
			val name = eligibilities.firstOrNull { it.peerEndpointId == incoming.remoteEndpointId }?.remoteDisplayName
				?: savedNames[incoming.remoteEndpointId]
			return PairingPrompt.IncomingRequest(incoming.remoteEndpointId, name)
		}
		return eligibilities.firstOrNull { it.peerEndpointId !in dismissedEligibilityIds }?.let {
			PairingPrompt.Eligibility(it.peerEndpointId, it.remoteDisplayName)
		}
	}

	private fun TargetedTransferModel.toExperienceItem(
		savedNames: Map<String, String>,
	): SavedDeviceTransferItem {
		val outgoing = role == TargetedTransferRoleModel.Sender
		val peerEndpointId = if (outgoing) receiverEndpointId else senderEndpointId
		val direction = if (outgoing) SavedDeviceTransferDirection.Outgoing else SavedDeviceTransferDirection.Incoming
		return SavedDeviceTransferItem(
			id = id,
			peerEndpointId = peerEndpointId,
			peerDisplayName = savedNames[peerEndpointId],
			direction = direction,
			transferName = transferName,
			fileCount = fileCount,
			totalSize = totalSize,
			verifiedBytes = verifiedBytes,
			state = state,
			createdAt = createdAt,
			updatedAt = updatedAt,
			availableActions = savedDeviceTransferActions(direction, state),
			progressFraction = savedDeviceTransferProgress(direction, state, verifiedBytes, totalSize),
		)
	}
}

private fun SavedDeviceModel.displayNameOrNull(): String? =
	localLabel?.takeIf(String::isNotBlank) ?: remoteDisplayName?.takeIf(String::isNotBlank)
