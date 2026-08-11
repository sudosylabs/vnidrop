package com.vnidrop.app.feature.saveddevices

import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.ReceiveFolder
import com.vnidrop.app.core.ReceiveFolderKind
import com.vnidrop.app.core.TargetedOfferResponseModel
import com.vnidrop.app.preferences.AppPreferences
import com.vnidrop.app.support.FakeCoreGateway
import com.vnidrop.app.support.FakeFileSystemService
import com.vnidrop.app.support.FakePreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import uniffi.vnidrop.ReceiveOutputSinkV2
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

@OptIn(ExperimentalCoroutinesApi::class)
class TargetedOfferCoordinatorTest {
	@Test
	fun acceptApprovesAndPullsByTransferId() = runTest {
		val core = initializedCore().apply {
			pendingTargetedOffers = listOf(offer("transfer-1"))
			respondTargetedResult = Result.success(TargetedOfferResponseModel.Approved("transfer-1"))
			receiveResult = Result.success(Unit)
		}
		val coordinator = TargetedOfferCoordinator(
			core,
			FakeFileSystemService(ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp")),
			preferences(enabled = true),
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		assertEquals("transfer-1", coordinator.state.value.current?.transferId)

		coordinator.accept("transfer-1")
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("transfer-1" to true), core.respondedTargetedOffers)
		assertEquals(listOf("transfer-1"), core.receivedTargetedTransferIds)
		assertEquals(listOf("transfer-1" to "/tmp"), core.receivedTargetedPathDirs)
		assertTrue(core.receivedTargetedViaSinkIds.isEmpty())
	}

	@Test
	fun acceptUsesOutputSinkWhenPlatformProvidesOne() = runTest {
		val sink = object : ReceiveOutputSinkV2 {
			override fun startFile(relativePath: String) = error("unused")
			override fun writeChunk(relativePath: String, bytes: ByteArray) = error("unused")
			override fun finishFile(relativePath: String) = error("unused")
			override fun abortFile(relativePath: String, reason: String) = error("unused")
		}
		val core = initializedCore().apply {
			pendingTargetedOffers = listOf(offer("transfer-sink"))
			respondTargetedResult = Result.success(TargetedOfferResponseModel.Approved("transfer-sink"))
			receiveResult = Result.success(Unit)
		}
		val coordinator = TargetedOfferCoordinator(
			core,
			FakeFileSystemService(
				ReceiveFolder(ReceiveFolderKind.AndroidPublicDownloads, "downloads", "Downloads"),
				receiveOutputSink = sink,
			),
			preferences(enabled = true),
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		coordinator.accept("transfer-sink")
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("transfer-sink"), core.receivedTargetedViaSinkIds)
		assertTrue(core.receivedTargetedPathDirs.isEmpty())
	}

	@Test
	fun declineDoesNotReceive() = runTest {
		val core = initializedCore().apply {
			pendingTargetedOffers = listOf(offer("transfer-2"))
			respondTargetedResult = Result.success(TargetedOfferResponseModel.Declined)
		}
		val coordinator = TargetedOfferCoordinator(
			core,
			FakeFileSystemService(ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp")),
			preferences(enabled = true),
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		coordinator.decline("transfer-2")
		runCurrent()
		advanceUntilIdle()
		assertEquals(listOf("transfer-2" to false), core.respondedTargetedOffers)
		assertTrue(core.receivedTargetedTransferIds.isEmpty())
	}

	@Test
	fun experimentalOffIgnoresPendingOffers() = runTest {
		val core = initializedCore().apply {
			pendingTargetedOffers = listOf(offer("transfer-3"))
		}
		val coordinator = TargetedOfferCoordinator(
			core,
			FakeFileSystemService(ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp")),
			preferences(enabled = false),
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		assertTrue(coordinator.state.value.pending.isEmpty())
	}

	@Test
	fun waitsForCoreInitializeBeforeListingOffers() = runTest {
		val core = FakeCoreGateway().apply {
			pendingTargetedOffers = listOf(offer("transfer-late"))
		}
		val coordinator = TargetedOfferCoordinator(
			core,
			FakeFileSystemService(ReceiveFolder(ReceiveFolderKind.FileSystemPath, "/tmp", "tmp")),
			preferences(enabled = true),
			UiMessageController(),
			backgroundScope,
		)
		runCurrent()
		advanceUntilIdle()
		assertEquals(0, core.listPendingTargetedOffersCount)
		assertTrue(coordinator.state.value.pending.isEmpty())

		core.mutableState.value = core.mutableState.value.copy(isInitialized = true)
		runCurrent()
		advanceUntilIdle()
		assertEquals(1, core.listPendingTargetedOffersCount)
		assertEquals("transfer-late", coordinator.state.value.current?.transferId)
	}

	private fun initializedCore() = FakeCoreGateway().apply {
		mutableState.value = mutableState.value.copy(isInitialized = true)
	}

	private fun offer(id: String) = PendingTargetedOfferModel(
		transferId = id,
		senderEndpointId = "sender",
		receiverEndpointId = "receiver",
		manifestId = "manifest",
		contentHash = "hash",
		transferName = "Photos",
		fileCount = 1u,
		totalSize = 10u,
		protocolVersion = 1u,
		receivedAt = 1L,
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
