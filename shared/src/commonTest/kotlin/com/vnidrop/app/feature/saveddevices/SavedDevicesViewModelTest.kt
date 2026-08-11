package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.PickedShareFile
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.core.TargetedTransferModel
import com.vnidrop.app.core.TargetedTransferStateModel
import com.vnidrop.app.preferences.AppPreferences
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.support.FakeFileSystemService
import com.vnidrop.app.support.FakePreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.theme.ThemeMode
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
	fun labelForgetAndBlockUpdateGateway() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			savedDevices = listOf(device("peer-1", label = null))
		}
		val preferences = preferences(enabled = true)
		val viewModel = SavedDevicesViewModel(
			core,
			FakeFileSystemService(ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp")),
			preferences,
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

	@Test
	fun sendFromSavedDeviceCreatesTargetedTransfer() = runTest {
		Dispatchers.setMain(StandardTestDispatcher(testScheduler))
		val core = FakeCoreGateway().apply {
			savedDevices = listOf(device("peer-3", label = "Kitchen"))
			createTargetedResult = Result.success(
				TargetedTransferModel(
					id = "t1",
					senderEndpointId = "me",
					receiverEndpointId = "peer-3",
					manifestId = "m",
					fileCount = 1u,
					totalSize = 1u,
					verifiedBytes = 0u,
					state = TargetedTransferStateModel.Offering,
					createdAt = 1L,
					updatedAt = 1L,
				),
			)
		}
		val viewModel = SavedDevicesViewModel(
			core,
			FakeFileSystemService(ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp")),
			preferences(enabled = true),
			UiMessageController(),
		)
		runCurrent()
		advanceUntilIdle()
		viewModel.startSend("peer-3")
		viewModel.onFilesPicked(
			listOf(PickedShareFile(value = "/tmp/a.txt", displayName = "a.txt", sizeBytes = 1u)),
		)
		runCurrent()
		advanceUntilIdle()
		assertEquals(1, core.createdTargetedTransfers.size)
		assertEquals("peer-3", core.createdTargetedTransfers.single().first)
		assertNull(viewModel.state.value.sendTargetPeerId)
	}

	private fun device(id: String, label: String?) = SavedDeviceModel(
		endpointId = id,
		localLabel = label,
		remoteDisplayName = null,
		createdAt = 1L,
		lastAuthenticatedAt = null,
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
