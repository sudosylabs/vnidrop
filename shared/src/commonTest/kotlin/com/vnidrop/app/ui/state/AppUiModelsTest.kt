package com.vnidrop.app.ui.state

import com.vnidrop.app.feature.receive.ReceiveState
import com.vnidrop.app.feature.send.SendState
import com.vnidrop.app.core.CoreEventModel
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.PickedShareFile
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.ui.theme.ThemeMode
import com.vnidrop.app.ui.theme.resolveDarkTheme
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.progress_completed
import vnidrop.shared.generated.resources.progress_interrupted
import vnidrop.shared.generated.resources.progress_saving
import vnidrop.shared.generated.resources.progress_sending
import vnidrop.shared.generated.resources.progress_working
import vnidrop.shared.generated.resources.transfer_file_count

class AppUiModelsTest {
	@Test
	fun windowClassUsesPhoneTabletDesktopBreakpoints() {
		assertEquals(WindowClass.Phone, windowClassFor(390f))
		assertEquals(WindowClass.Tablet, windowClassFor(700f))
		assertEquals(WindowClass.Desktop, windowClassFor(1200f))
	}

	@Test
	fun bottomNavigationIsReservedForPhoneWidth() {
		assertTrue(useBottomNavigation(WindowClass.Phone))
		assertFalse(useBottomNavigation(WindowClass.Tablet))
		assertFalse(useBottomNavigation(WindowClass.Desktop))
	}

	@Test
	fun themeModeResolvesAgainstSystemOnlyWhenRequested() {
		assertTrue(resolveDarkTheme(ThemeMode.System, systemDark = true))
		assertFalse(resolveDarkTheme(ThemeMode.System, systemDark = false))
		assertFalse(resolveDarkTheme(ThemeMode.Light, systemDark = true))
		assertTrue(resolveDarkTheme(ThemeMode.Dark, systemDark = false))
	}

	@Test
	fun byteFormattingKeepsTransferCardsReadable() {
		assertEquals("58 B", formatBytes(58UL))
		assertEquals("1.5 KB", formatBytes(1536UL))
	}

	@Test
	fun sendStateExposesShareEligibility() {
		val ready = SendState(
			selectedFiles = listOf(PickedShareFile("/tmp/payload.txt", "payload.txt", 128UL)),
			transferName = "payload.txt",
		)

		assertTrue(ready.canCreateShare(coreInitialized = true))
		assertFalse(ready.canCreateShare(coreInitialized = false))
		assertFalse(SendState().canCreateShare(coreInitialized = true))
		assertFalse(ready.copy(isSharing = true).canCreateShare(coreInitialized = true))
	}

	@Test
	fun receiveStateExposesInspectAndReceiveEligibility() {
		val ready = ReceiveState(
			ticket = "ticket",
			inspection = com.vnidrop.app.core.TicketInspectionModel(
				kind = "vnidrop",
				metadata = com.vnidrop.app.core.TransferMetadataModel(1UL, "Photo", null, "hash", 1UL, 42UL),
			),
			folderAccessStatus = com.vnidrop.app.core.FolderAccessStatus.Writable,
		)

		assertTrue(ready.canReceive(coreInitialized = true))
		assertFalse(ready.canReceive(coreInitialized = false))
		assertFalse(ready.copy(ticket = "").canReceive(coreInitialized = true))
		assertFalse(ready.copy(inspection = null).canReceive(coreInitialized = true))
		assertFalse(ready.copy(isReceiving = true).canReceive(coreInitialized = true))
	}

	@Test
	fun transferActivityOnlyIncludesRunningStatuses() {
		assertTrue(storedTransfer(status = TransferStatus.Importing).isActiveTransfer())
		assertTrue(storedTransfer(status = TransferStatus.Sharing).isActiveTransfer())
		assertTrue(storedTransfer(status = TransferStatus.Receiving).isActiveTransfer())
		assertFalse(storedTransfer(status = TransferStatus.Done).isActiveTransfer())
		assertFalse(storedTransfer(status = TransferStatus.Failed).isActiveTransfer())
		assertFalse(storedTransfer(status = TransferStatus.Cancelled).isActiveTransfer())
	}

	@Test
	fun parseProgressUnderstandsCorePayloadKeys() {
		assertEquals(0.5f, parseProgress("""{"offset":50,"size":100}"""))
		assertEquals(0.25f, parseProgress("""{"exported":25,"file_size":100}"""))
		assertEquals(0.8f, parseProgress("""{"downloaded":80,"total_size":100}"""))
		assertEquals(0.1f, parseProgress("""{"end_offset":10}""", sizeHint = 100.0))
		assertEquals(null, parseProgress("""{"end_offset":10}"""))
	}

	@Test
	fun progressForTransferUsesNewestMatchingEvent() {
		val events = listOf(
			event(id = "new", phase = "export", kind = "progress", data = """{"exported":75,"file_size":100,"file_name":"a.bin"}"""),
			event(id = "old", phase = "export", kind = "progress", data = """{"exported":10,"file_size":100,"file_name":"a.bin"}"""),
		)
		val progress = progressForTransfer(events, 7UL)
		assertEquals(0.75f, progress?.progress)
		assertEquals(Res.string.progress_saving, progress?.label)
		assertEquals("a.bin", progress?.detail)
	}

	@Test
	fun progressForReceiverAggregatesMultiBlobSendByEndpoint() {
		val events = listOf(
			// newest first
			event(
				id = "p2",
				phase = "transfer",
				kind = "progress",
				data = """{"connection_id":9,"request_id":2,"endpoint_id":"peer-a","end_offset":40}""",
				direction = "send",
			),
			event(
				id = "s2",
				phase = "transfer",
				kind = "started",
				data = """{"connection_id":9,"request_id":2,"endpoint_id":"peer-a","size":50}""",
				direction = "send",
			),
			event(
				id = "c1",
				phase = "transfer",
				kind = "completed",
				data = """{"connection_id":9,"request_id":1,"endpoint_id":"peer-a"}""",
				direction = "send",
			),
			event(
				id = "s1",
				phase = "transfer",
				kind = "started",
				data = """{"connection_id":9,"request_id":1,"endpoint_id":"peer-a","size":50}""",
				direction = "send",
			),
			// different receiver should not mix in
			event(
				id = "other",
				phase = "transfer",
				kind = "progress",
				data = """{"connection_id":3,"request_id":7,"endpoint_id":"peer-b","end_offset":99}""",
				direction = "send",
			),
		)
		// blob1 complete 50 + blob2 40 = 90 / total hint 100
		val progress = progressForReceiver(events, 7UL, "peer-a", totalSizeHint = 100UL)
		assertEquals(0.9f, progress?.progress)
		assertEquals(Res.string.progress_sending, progress?.label)
		assertEquals(null, progressForReceiver(events, 7UL, "missing"))
	}

	@Test
	fun progressForReceiverFallsBackToConnectionMap() {
		val events = listOf(
			event(
				id = "prog",
				phase = "transfer",
				kind = "progress",
				data = """{"connection_id":4,"request_id":1,"end_offset":25}""",
				direction = "send",
			),
			event(
				id = "start",
				phase = "transfer",
				kind = "started",
				data = """{"connection_id":4,"request_id":1,"size":100}""",
				direction = "send",
			),
			CoreEventModel(
				id = "conn",
				revision = 1UL,
				timestamp = 1L,
				scope = "endpoint",
				transferId = null,
				direction = null,
				phase = "provider",
				kind = "client-connected",
				dataJson = """{"connection_id":4,"endpoint_id":"peer-z"}""",
			),
		)
		val progress = progressForReceiver(events, 7UL, "peer-z")
		assertEquals(0.25f, progress?.progress)
	}

	@Test
	fun activeSendProgressPicksLiveReceiverSend() {
		val events = listOf(
			event(
				id = "p",
				phase = "transfer",
				kind = "progress",
				data = """{"connection_id":1,"request_id":1,"endpoint_id":"peer-a","end_offset":30}""",
				direction = "send",
			),
			event(
				id = "s",
				phase = "transfer",
				kind = "started",
				data = """{"connection_id":1,"request_id":1,"endpoint_id":"peer-a","size":100}""",
				direction = "send",
			),
		)
		val progress = activeSendProgress(events, 7UL, setOf("peer-a"), totalSizeHint = 100UL)
		assertEquals(0.3f, progress?.progress)
		assertEquals(Res.string.progress_sending, progress?.label)
		assertEquals(null, activeSendProgress(events, 7UL, emptySet(), totalSizeHint = 100UL))
	}

	@Test
	fun canCancelOnlyActiveStatuses() {
		assertTrue(storedTransfer(status = TransferStatus.Sharing).canCancelTransfer())
		assertTrue(storedTransfer(status = TransferStatus.Receiving).canCancelTransfer())
		assertFalse(storedTransfer(status = TransferStatus.Done).canCancelTransfer())
	}

	@Test
	fun unknownProgressEventsUseGenericWorkingLabel() {
		// Tracked phase/kind that has no dedicated product copy must not dump tokens.
		val progress = progressForTransfer(
			listOf(event(id = "x", phase = "lifecycle", kind = "progress", data = "{}")),
			7UL,
		)
		assertEquals(Res.string.progress_working, progress?.label)
	}

	@Test
	fun transferFileCountUsesPluralResource() {
		assertEquals(Res.plurals.transfer_file_count, transferFileCountResource())
	}

	@Test
	fun progressForReceiverInterruptedUsesInterruptedLabel() {
		val events = listOf(
			event(
				id = "aborted",
				phase = "transfer",
				kind = "aborted",
				data = """{"connection_id":1,"request_id":1,"endpoint_id":"peer-a"}""",
				direction = "send",
			),
		)
		val progress = progressForReceiver(events, 7UL, "peer-a")
		assertEquals(Res.string.progress_interrupted, progress?.label)
		assertEquals(null, progress?.progress)
	}

	@Test
	fun progressForReceiverCompletedWithoutProgressUsesCompletedLabel() {
		val events = listOf(
			event(
				id = "done",
				phase = "transfer",
				kind = "completed",
				data = """{"connection_id":1,"request_id":1,"endpoint_id":"peer-a"}""",
				direction = "send",
			),
		)
		val progress = progressForReceiver(events, 7UL, "peer-a")
		assertEquals(Res.string.progress_completed, progress?.label)
		assertEquals(1f, progress?.progress)
	}

	@Test
	fun progressForReceiverCompletedAfterProgressUsesCompletedLabel() {
		val events = listOf(
			event(
				id = "done",
				phase = "transfer",
				kind = "completed",
				data = """{"connection_id":1,"request_id":1,"endpoint_id":"peer-a"}""",
				direction = "send",
			),
			event(
				id = "progress",
				phase = "transfer",
				kind = "progress",
				data = """{"connection_id":1,"request_id":1,"endpoint_id":"peer-a","end_offset":100}""",
				direction = "send",
			),
			event(
				id = "started",
				phase = "transfer",
				kind = "started",
				data = """{"connection_id":1,"request_id":1,"endpoint_id":"peer-a","size":100}""",
				direction = "send",
			),
		)

		val progress = progressForReceiver(events, 7UL, "peer-a", totalSizeHint = 100UL)
		assertEquals("completed", progress?.kind)
		assertEquals(Res.string.progress_completed, progress?.label)
		assertEquals(1f, progress?.progress)
		assertEquals(null, activeSendProgress(events, 7UL, setOf("peer-a"), totalSizeHint = 100UL))
	}

	private fun storedTransfer(status: TransferStatus): Transfer =
		Transfer(
			localId = "local-1",
			transferId = 1UL,
			peerId = null,
			direction = TransferDirection.Send,
			status = status,
			transferName = "Demo",
			contentHash = "hash",
			ticket = null,
			fileCount = 1UL,
			totalSize = 128UL,
			accessPolicy = ShareAccessPolicy.RequireApproval,
			createdAt = 1L,
			updatedAt = 1L,
		)

	private fun event(
		id: String,
		phase: String,
		kind: String,
		data: String,
		transferId: ULong = 7UL,
		direction: String = "receive",
	) = CoreEventModel(
		id = id,
		revision = 1UL,
		timestamp = 1L,
		scope = "transfer",
		transferId = transferId,
		direction = direction,
		phase = phase,
		kind = kind,
		dataJson = data,
	)
}
