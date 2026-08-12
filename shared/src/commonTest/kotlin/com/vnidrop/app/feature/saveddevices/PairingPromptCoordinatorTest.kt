package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.ui.feedback.UiMessageController
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class PairingPromptCoordinatorTest {
	@Test
	fun waitsForCoreInitializeBeforeRefreshing() = runTest {
		// The coordinator must not query domain state before AppViewModel initializes the core.
		val core = FakeCoreGateway().apply {
			pairingEligibilities = listOf(eligibility("peer-a"))
		}
		val messages = UiMessageController()
		val seen = mutableListOf<String>()
		backgroundScope.launch {
			messages.messages.collect { seen += it.text.toString() }
		}
		val coordinator = PairingPromptCoordinator(
			core,
			messages,
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		assertEquals(0, core.listDeviceRelationshipsCount)
		assertNull(coordinator.state.value.prompt)
		assertTrue(seen.isEmpty())

		core.mutableState.value = core.mutableState.value.copy(isInitialized = true)
		runCurrent()
		advanceUntilIdle()
		assertEquals(1, core.listDeviceRelationshipsCount)
		assertEquals(PairingPrompt.Eligibility("peer-a", "Remote device"), coordinator.state.value.prompt)
		assertTrue(seen.isEmpty())
	}

	@Test
	fun acceptEligibilityRequestsPairing() = runTest {
		val core = initializedCore().apply {
			pairingEligibilities = listOf(eligibility("peer-a"))
		}
		val coordinator = PairingPromptCoordinator(
			core,
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		assertEquals(PairingPrompt.Eligibility("peer-a", "Remote device"), coordinator.state.value.prompt)

		coordinator.accept()
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("peer-a"), core.requestedPairings)
	}

	@Test
	fun declineEligibilityConsumesWithoutRequest() = runTest {
		val core = initializedCore().apply {
			pairingEligibilities = listOf(eligibility("peer-a"))
		}
		val coordinator = PairingPromptCoordinator(
			core,
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		coordinator.decline()
		runCurrent()
		advanceUntilIdle()
		assertTrue(core.requestedPairings.isEmpty())
		assertTrue(core.pairingEligibilities.none { it.peerEndpointId == "peer-a" })
		assertNull(coordinator.state.value.prompt)
	}

	@Test
	fun dismissKeepsEligibilityForSavedDevicesArea() = runTest {
		val core = initializedCore().apply {
			pairingEligibilities = listOf(eligibility("peer-a"))
		}
		val coordinator = PairingPromptCoordinator(
			core,
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		coordinator.dismiss()
		runCurrent()
		advanceUntilIdle()
		assertNull(coordinator.state.value.prompt)
		assertEquals(1, core.pairingEligibilities.size)
	}

	@Test
	fun incomingPairingRequestAcceptsViaRespond() = runTest {
		val core = initializedCore().apply {
			deviceRelationships = listOf(incoming("peer-b"))
		}
		val coordinator = PairingPromptCoordinator(
			core,
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		assertIs<PairingPrompt.IncomingRequest>(coordinator.state.value.prompt)

		coordinator.accept()
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("peer-b" to true), core.pairingResponses)
	}

	private fun initializedCore() = FakeCoreGateway().apply {
		mutableState.value = mutableState.value.copy(isInitialized = true)
	}

	private fun eligibility(peer: String) = PairingEligibilityModel(
		peerEndpointId = peer,
		remoteDisplayName = "Remote device",
		sessionId = "session",
		protocolVersion = 1u,
		createdAt = 1L,
		expiresAt = 2L,
	)

	private fun incoming(peer: String) = DeviceRelationshipModel(
		remoteEndpointId = peer,
		state = DeviceRelationshipStateModel.PendingIncoming,
		generation = 1u,
		minimumProtocolVersion = 1u,
		createdAt = 1L,
		updatedAt = 1L,
	)
}
