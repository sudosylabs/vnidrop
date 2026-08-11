package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.preferences.AppPreferences
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.support.FakePreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.ExperimentalCoroutinesApi
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
	fun experimentalOffDoesNotPromptOnEligibility() = runTest {
		val core = FakeCoreGateway().apply {
			pairingEligibilities = listOf(eligibility("peer-a"))
		}
		val coordinator = PairingPromptCoordinator(
			core,
			preferences(enabled = false),
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		assertNull(coordinator.state.value.prompt)
	}

	@Test
	fun acceptEligibilityRequestsPairing() = runTest {
		val core = FakeCoreGateway().apply {
			pairingEligibilities = listOf(eligibility("peer-a"))
		}
		val coordinator = PairingPromptCoordinator(
			core,
			preferences(enabled = true),
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		assertEquals(PairingPrompt.Eligibility("peer-a"), coordinator.state.value.prompt)

		coordinator.accept()
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("peer-a"), core.requestedPairings)
	}

	@Test
	fun declineEligibilityConsumesWithoutRequest() = runTest {
		val core = FakeCoreGateway().apply {
			pairingEligibilities = listOf(eligibility("peer-a"))
		}
		val coordinator = PairingPromptCoordinator(
			core,
			preferences(enabled = true),
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
		val core = FakeCoreGateway().apply {
			pairingEligibilities = listOf(eligibility("peer-a"))
		}
		val coordinator = PairingPromptCoordinator(
			core,
			preferences(enabled = true),
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
		val core = FakeCoreGateway().apply {
			deviceRelationships = listOf(incoming("peer-b"))
		}
		val coordinator = PairingPromptCoordinator(
			core,
			preferences(enabled = true),
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

	private fun eligibility(peer: String) = PairingEligibilityModel(
		peerEndpointId = peer,
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

	private fun preferences(enabled: Boolean) = FakePreferencesRepository(
		AppPreferences(
			username = "User",
			receiveFolder = ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp"),
			themeMode = ThemeMode.System,
			notificationsEnabled = false,
			experimentalSavedDevicesEnabled = enabled,
		),
	)
}
