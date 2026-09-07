package com.vnidrop.app.feature.send

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToIndex
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.v2.runComposeUiTest
import androidx.compose.ui.unit.dp
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.feature.receive.ReceiveScreen
import com.vnidrop.app.feature.receive.ReceiveState
import com.vnidrop.app.feature.receive.ReceiveInvitationActions
import com.vnidrop.app.feature.receive.ReceiveMethodAvailability
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.VniDropTheme
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.coroutines.runBlocking
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.getString
import vnidrop.shared.generated.resources.*

@OptIn(ExperimentalTestApi::class)
class AndroidSendFlowTest {
	@Test
	fun longFileNamesStayOnOneLineWithMiddleEllipsis() = runComposeUiTest {
		val name = "CV_Abass_Hammed_McDonalds_Application_September_2026.pdf"
		val screen = mutableStateOf(0)
		val invitations = object : ReceiveInvitationActions {
			override val fileAvailability = ReceiveMethodAvailability.Available
			override val qrAvailability = ReceiveMethodAvailability.Available
			override fun pickInvitation(onResult: (Result<String>) -> Unit) = Unit
			override fun scanQrCode(onResult: (Result<String>) -> Unit) = Unit
			override fun cancel() = Unit
		}
		setContent {
			Phone {
				if (screen.value == 1) TransferCatalog(
					listOf(transfer().copy(transferName = name)), emptyMap(), windowClass = WindowClass.Phone,
					onOpenComposer = {}, onTransferSelected = {},
				) else if (screen.value == 2) ReceiveScreen(
					CoreState(isInitialized = true, transfers = listOf(transfer().copy(transferName = name, direction = TransferDirection.Receive, status = TransferStatus.Done))),
					ReceiveState(), WindowClass.Phone, invitations,
					onOpenAcquisition = {}, onDismissAcquisition = {}, onReceiverNameChanged = {}, onInvitationResult = { _, _ -> },
					onReceive = {}, onRequestDeleteHistoryItem = {}, onRequestClearHistory = {}, onDismissHistoryDelete = {}, onConfirmHistoryDelete = {},
				) else if (screen.value == 3) SendScreen(
					CoreState(isInitialized = true, transfers = listOf(transfer().copy(transferName = name))),
					SendState(selectedTransferId = 9UL), WindowClass.Phone,
					onOpenComposer = {}, onTransferSelected = {}, onCloseTransferDetails = {}, onCopyTicket = {},
				) else Composer(draft().copy(sources = draft().sources.map { it.copy(displayName = name) }))
			}
		}
		for (page in 0..3) {
			runOnIdle { screen.value = page }
			val layouts = mutableListOf<TextLayoutResult>()
			onNodeWithText(name).performSemanticsAction(SemanticsActions.GetTextLayoutResult) { it(layouts) }
			val layout = layouts.single()
			assertEquals(1, layout.lineCount)
			assertEquals(TextOverflow.MiddleEllipsis, layout.layoutInput.overflow)
			assertEquals(name, layout.layoutInput.text.text)
		}
	}

	@Test
	fun composerKeepsSubmitVisibleAndDisablesEditingWhileSubmitting() = runComposeUiTest {
		val state = mutableStateOf(draft())
		var submitted = 0
		setContent {
			Phone {
				Composer(state.value, onSubmit = { submitted++ }, onName = { state.value = state.value.copy(transferName = it) })
			}
		}
		onNodeWithTag("submit-transfer").assertIsDisplayed().assertIsEnabled()
		onNodeWithText(Res.string.field_transfer_name.value).performScrollTo().performTextReplacement("")
		onNodeWithTag("submit-transfer").assertIsDisplayed().assertIsNotEnabled()
		onNodeWithText(Res.string.field_transfer_name.value).performTextReplacement("Trip photos")
		onNodeWithTag("submit-transfer").performClick()
		runOnIdle {
			assertEquals(1, submitted)
			state.value = state.value.copy(isSubmitting = true)
		}
		onNodeWithTag("submit-transfer").assertIsNotEnabled()
		onNodeWithContentDescription(Res.string.button_close.value).assertIsNotEnabled()
		onNodeWithText(Res.string.field_transfer_name.value).performScrollTo().assertIsNotEnabled()
	}

	@Test
	fun targetedDraftKeepsReceiverAndOmitsInvitationControls() = runComposeUiTest {
		setContent {
			Phone {
				Composer(draft().copy(destination = TransferDraftDestination.Targeted(LockedSavedDevice("peer", "Office laptop"))))
			}
		}
		onNodeWithText("Office laptop").performScrollTo().assertIsDisplayed()
		onAllNodesWithText(Res.string.field_sender_name.value).assertCountEquals(0)
		onAllNodesWithText(Res.string.send_access_title.value).assertCountEquals(0)
		onNodeWithTag("submit-transfer").assertIsDisplayed().assertIsEnabled()
	}

	@Test
	fun removingLastFileReturnsToSourceChoicesWithoutClosingComposer() = runComposeUiTest {
		val state = mutableStateOf(draft())
		var picked = 0
		setContent {
			Phone {
				Composer(
					state.value,
					onRemove = { state.value = state.value.copy(sources = emptyList(), transferName = "") },
					onFiles = { picked++ },
				)
			}
		}
		onNodeWithContentDescription(Res.string.button_remove_file.value).performClick()
		onNodeWithText(Res.string.send_choose_file_title.value).assertIsDisplayed()
		onNodeWithText(Res.string.button_choose_files.value).performClick()
		runOnIdle { assertEquals(1, picked) }
	}

	@Test
	fun detailsBackReturnsToListAndDeletionRequiresConfirmation() = runComposeUiTest {
		val state = mutableStateOf(SendState())
		var deleted = 0
		setContent {
			Phone {
				SendScreen(
					coreState = CoreState(isInitialized = true, transfers = listOf(transfer())),
					state = state.value, windowClass = WindowClass.Phone,
					onOpenComposer = {}, onCopyTicket = {},
					onTransferSelected = { state.value = state.value.copy(selectedTransferId = it) },
					onCloseTransferDetails = { state.value = SendState() },
					onRequestDelete = { state.value = state.value.copy(isDeleteConfirmationOpen = true, deleteTargetTransferId = 9UL) },
					onDismissDelete = { state.value = state.value.copy(isDeleteConfirmationOpen = false) },
					onConfirmDelete = { deleted++ },
				)
			}
		}
		onNodeWithText("Photos").performClick()
		onNodeWithText("1 file · 1.5 KB").assertIsDisplayed()
		onNodeWithContentDescription(Res.string.button_back.value).performClick()
		onNodeWithText(Res.string.send_title.value).assertIsDisplayed()
		onNodeWithText("Photos").performClick()
		onNodeWithTag("transfer-delete-action").performScrollTo().performClick()
		onNodeWithText(Res.string.transfer_delete_title.value).assertIsDisplayed()
		runOnIdle { assertEquals(0, deleted) }
		onNodeWithText(Res.string.button_cancel.value).performClick()
		onNodeWithContentDescription(Res.string.button_back.value).assertIsDisplayed()
		runOnIdle { assertEquals(0, deleted) }
	}

	@Test
	fun returningFromDetailsPreservesListPositionAndAppBar() = runComposeUiTest {
		val state = mutableStateOf(SendState())
		val transfers = (1..30).map { transfer().copy(localId = "local-$it", transferId = it.toULong(), transferName = "Transfer $it") }
		setContent {
			Phone {
				SendScreen(
					CoreState(isInitialized = true, transfers = transfers), state.value, WindowClass.Phone,
					onOpenComposer = {}, onCopyTicket = {},
					onTransferSelected = { state.value = state.value.copy(selectedTransferId = it) },
					onCloseTransferDetails = { state.value = SendState() },
				)
			}
		}
		onNodeWithTag("send-transfer-list").performScrollToIndex(29)
		onNodeWithText(Res.string.send_title.value).assertIsDisplayed()
		onNodeWithText("Transfer 30").performClick()
		onNodeWithContentDescription(Res.string.button_back.value).performClick()
		onNodeWithText("Transfer 30").assertIsDisplayed()
		onNodeWithText(Res.string.send_title.value).assertIsDisplayed()
	}

	@Test
	fun tabletCatalogStillOffersCreationWhenItHasTransfers() = runComposeUiTest {
		var created = 0
		setContent {
			Phone {
				TransferCatalog(listOf(transfer()), emptyMap(), windowClass = WindowClass.Tablet, onOpenComposer = { created++ }, onTransferSelected = {})
			}
		}
		onNodeWithContentDescription(Res.string.button_create_new_transfer.value).assertIsDisplayed().performClick()
		runOnIdle { assertEquals(1, created) }
	}

	@Composable
	private fun Phone(content: @Composable () -> Unit) {
		CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Android) {
			VniDropTheme(isDarkTheme = false) { Box(Modifier.size(393.dp, 640.dp)) { content() } }
		}
	}

	@Composable
	private fun Composer(
		state: TransferDraftState,
		onSubmit: () -> Unit = {},
		onName: (String) -> Unit = {},
		onRemove: (DraftSourceId) -> Unit = {},
		onFiles: () -> Unit = {},
	) {
		TransferComposer(
			true, state, WindowClass.Phone,
			onSelectFile = onFiles, onSelectFolder = {}, onClearFile = {}, onRemoveFile = onRemove,
			onTransferNameChanged = onName, onSenderNameChanged = {}, onAccessPolicyChanged = {}, onSubmit = onSubmit,
		)
	}

	private fun draft() = TransferDraftState(
		destination = TransferDraftDestination.Invitation,
		sources = listOf(TransferDraftSource(DraftSourceId("source"), "Photos from the weekend.zip", 1536UL, null, false)),
		transferName = "Photos", senderName = "My phone",
	)

	private fun transfer() = Transfer(
		localId = "local-9", transferId = 9UL, direction = TransferDirection.Send, status = TransferStatus.Sharing,
		peerId = null, transferName = "Photos", contentHash = "hash", fileCount = 1UL, totalSize = 1536UL,
		ticket = "ticket", accessPolicy = ShareAccessPolicy.RequireApproval, createdAt = 1L, updatedAt = 1L,
	)
}

private val StringResource.value: String get() = runBlocking { getString(this@value) }
