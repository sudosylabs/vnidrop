package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.ui.feedback.UiMessageController
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class SavedDevicesViewModelTest {
	@AfterTest
	fun tearDown() {
		Dispatchers.resetMain()
	}

	@Test
	fun loadsSavedDevicesWithoutAnExperimentalPreferenceGate() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			savedDevices = listOf(device("peer-always-visible", label = null))
		}
		val viewModel = SavedDevicesViewModel(core, UiMessageController())

		runCurrent()
		advanceUntilIdle()

		assertEquals("peer-always-visible", viewModel.state.value.savedDevices.single().endpointId)
		assertEquals(false, viewModel.state.value.isLoading)
		assertEquals(false, viewModel.state.value.loadFailed)
	}

	@Test
	fun labelForgetAndBlockUpdateGateway() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			mutableState.value = mutableState.value.copy(isInitialized = true)
			savedDevices = listOf(device("peer-1", label = null))
		}
		val viewModel = SavedDevicesViewModel(
			core,
			UiMessageController(),
		)
		runCurrent()
		advanceUntilIdle()
		assertEquals(1, viewModel.state.value.savedDevices.size)

		viewModel.openLabelEditor("peer-1")
		viewModel.setLabelDraft("Kitchen")
		viewModel.saveLabel()
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf<Pair<String, String?>>("peer-1" to "Kitchen"), core.labeledDevices.toList())
		assertEquals("Kitchen", viewModel.state.value.savedDevices.single().localLabel)

		viewModel.openLabelEditor("peer-1")
		viewModel.clearLabel()
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf<Pair<String, String?>>("peer-1" to "Kitchen", "peer-1" to null), core.labeledDevices.toList())
		assertNull(viewModel.state.value.savedDevices.single().localLabel)

		viewModel.forget("peer-1")
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("peer-1"), core.forgottenDevices.toList())
		assertTrue(viewModel.state.value.savedDevices.isEmpty())

		core.savedDevices = listOf(device("peer-2", label = "Desk"))
		viewModel.block("peer-2")
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("peer-2"), core.blockedPeers.toList())
	}

	private fun device(id: String, label: String?) = SavedDeviceModel(
		endpointId = id,
		localLabel = label,
		remoteDisplayName = null,
		createdAt = 1L,
		lastAuthenticatedAt = null,
	)
}
