package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.core.TargetedTransferModel
import com.vnidrop.app.core.TargetedTransferRoleModel
import com.vnidrop.app.core.TargetedTransferStateModel
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class SavedDevicesReadModelTest {
	private val readModel = SavedDevicesReadModel()

	@Test
	fun transferActionMatrixIsDerivedBeforeRendering() {
		val activeStates = listOf(
			TargetedTransferStateModel.Preparing,
			TargetedTransferStateModel.Offering,
			TargetedTransferStateModel.AwaitingApproval,
			TargetedTransferStateModel.Approved,
			TargetedTransferStateModel.Connecting,
			TargetedTransferStateModel.Transferring,
			TargetedTransferStateModel.Interrupted,
		)
		val terminalStates = listOf(
			TargetedTransferStateModel.Completed,
			TargetedTransferStateModel.Declined,
			TargetedTransferStateModel.Cancelled,
			TargetedTransferStateModel.Failed,
		)

		activeStates.forEach { state ->
			val outgoing = item(state, TargetedTransferRoleModel.Sender)
			val incoming = item(state, TargetedTransferRoleModel.Receiver)
			assertEquals(listOf(SavedDeviceTransferAction.Cancel), outgoing.availableActions, "outgoing $state")
			assertEquals(
				when (state) {
					TargetedTransferStateModel.Approved -> listOf(
						SavedDeviceTransferAction.Receive,
						SavedDeviceTransferAction.Cancel,
					)
					TargetedTransferStateModel.Interrupted -> listOf(
						SavedDeviceTransferAction.Resume,
						SavedDeviceTransferAction.Cancel,
					)
					else -> listOf(SavedDeviceTransferAction.Cancel)
				},
				incoming.availableActions,
				"incoming $state",
			)
		}
		terminalStates.forEach { state ->
			assertEquals(
				listOf(SavedDeviceTransferAction.Delete),
				item(state, TargetedTransferRoleModel.Sender).availableActions,
				state.toString(),
			)
		}
	}

	@Test
	fun progressFactsAreReceiverOnlyAndSurviveInterruption() {
		listOf(TargetedTransferStateModel.Connecting, TargetedTransferStateModel.Transferring, TargetedTransferStateModel.Interrupted)
			.forEach { state ->
				assertEquals(0.4f, item(state, TargetedTransferRoleModel.Receiver).progressFraction, state.toString())
				assertNull(item(state, TargetedTransferRoleModel.Sender).progressFraction, state.toString())
			}
		assertNull(item(TargetedTransferStateModel.Approved, TargetedTransferRoleModel.Receiver).progressFraction)
		assertNull(item(TargetedTransferStateModel.Completed, TargetedTransferRoleModel.Receiver).progressFraction)
	}

	@Test
	fun durableReadsProduceOneSortedJoinedPlatformSnapshot() {
		val snapshot = readModel.derive(
			SavedDevicesReadInputs(
				eligibilities = listOf(eligibility("dismissed", 4), eligibility("eligible", 2)),
				relationships = listOf(
					relationship("old-outgoing", DeviceRelationshipStateModel.PendingOutgoing, 1),
					relationship("incoming", DeviceRelationshipStateModel.PendingIncoming, 3),
					relationship("saved", DeviceRelationshipStateModel.Saved, 5),
				),
				savedDevices = listOf(
					device("peer", localLabel = "Desk", remoteName = "Remote", createdAt = 2),
					device("older", localLabel = null, remoteName = "Laptop", createdAt = 1),
				),
				pendingOffers = listOf(offer("later", 9), offer("earlier", 7)),
				targetedTransfers = listOf(
					transfer("old", TargetedTransferRoleModel.Sender, TargetedTransferStateModel.Completed, updatedAt = 1),
					transfer("deleted", TargetedTransferRoleModel.Sender, TargetedTransferStateModel.Deleted, updatedAt = 3),
					transfer("new", TargetedTransferRoleModel.Receiver, TargetedTransferStateModel.Interrupted, updatedAt = 2),
				),
			),
			dismissedEligibilityIds = setOf("dismissed"),
		)

		assertEquals(listOf("dismissed", "eligible"), snapshot.eligibilities.map { it.peerEndpointId })
		assertEquals(listOf("incoming", "old-outgoing"), snapshot.pendingRelationships.map { it.remoteEndpointId })
		assertEquals(listOf("peer", "older"), snapshot.savedDevices.map { it.endpointId })
		assertEquals(listOf("earlier", "later"), snapshot.pendingOffers.map { it.transferId })
		assertEquals(listOf("new", "old"), snapshot.targetedTransfers.map { it.id })
		assertEquals("Desk", snapshot.targetedTransfers.single { it.id == "old" }.peerDisplayName)
		assertEquals(PairingPrompt.IncomingRequest("incoming", null), snapshot.nextPairingPrompt)
	}

	@Test
	fun refreshAndRestartNeedOnlyDurableReadsNotAnEventPayload() {
		val inputs = SavedDevicesReadInputs(
			savedDevices = listOf(device("peer", null, "Phone", 1)),
			targetedTransfers = listOf(
				transfer("transfer", TargetedTransferRoleModel.Sender, TargetedTransferStateModel.Failed, updatedAt = 1),
			),
		)

		assertEquals(readModel.derive(inputs), SavedDevicesReadModel().derive(inputs))
	}

	private fun item(state: TargetedTransferStateModel, role: TargetedTransferRoleModel) = readModel.derive(
		SavedDevicesReadInputs(targetedTransfers = listOf(transfer("id", role, state, updatedAt = 1))),
	).targetedTransfers.single()

	private fun eligibility(peer: String, createdAt: Long) = PairingEligibilityModel(
		peerEndpointId = peer,
		remoteDisplayName = null,
		sessionId = "session-$peer",
		protocolVersion = 1u,
		createdAt = createdAt,
		expiresAt = createdAt + 10,
	)

	private fun relationship(peer: String, state: DeviceRelationshipStateModel, updatedAt: Long) =
		DeviceRelationshipModel(peer, state, 1u, 1u, 0, updatedAt)

	private fun device(id: String, localLabel: String?, remoteName: String?, createdAt: Long) =
		SavedDeviceModel(id, localLabel, remoteName, createdAt, createdAt)

	private fun offer(id: String, receivedAt: Long) = PendingTargetedOfferModel(
		id, "peer", "local", "manifest-$id", "hash-$id", id, 1u, 100u, 1u, receivedAt,
	)

	private fun transfer(
		id: String,
		role: TargetedTransferRoleModel,
		state: TargetedTransferStateModel,
		updatedAt: Long,
	) = TargetedTransferModel(
		id = id,
		role = role,
		senderEndpointId = if (role == TargetedTransferRoleModel.Sender) "local" else "peer",
		receiverEndpointId = if (role == TargetedTransferRoleModel.Sender) "peer" else "local",
		manifestId = "manifest-$id",
		contentHash = "hash-$id",
		transferName = id,
		fileCount = 1u,
		totalSize = 100u,
		verifiedBytes = 40u,
		state = state,
		createdAt = 0,
		updatedAt = updatedAt,
	)
}
