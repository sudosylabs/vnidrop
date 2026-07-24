package com.vnidrop.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.background
import androidx.compose.material3.Text
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.v2.runComposeUiTest
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import com.vnidrop.app.feature.approvals.ApprovalModalHost
import com.vnidrop.app.feature.approvals.ApprovalState
import com.vnidrop.app.feature.approvals.PendingApproval
import com.vnidrop.app.feature.receive.ReceiveHistoryDeleteTarget
import com.vnidrop.app.feature.receive.ReceiveInvitationActions
import com.vnidrop.app.feature.receive.ReceiveMethodAvailability
import com.vnidrop.app.feature.receive.ReceiveScreen
import com.vnidrop.app.feature.receive.ReceiveState
import com.vnidrop.app.feature.settings.SettingsScreen
import com.vnidrop.app.feature.settings.SettingsSection
import com.vnidrop.app.feature.settings.SettingsState
import com.vnidrop.app.feature.settings.SettingsOverview
import com.vnidrop.app.feature.send.SendScreen
import com.vnidrop.app.feature.send.SendState
import com.vnidrop.app.feature.send.TransferCatalog
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.PickedShareFile
import com.vnidrop.app.core.ShareAccessPolicy
import com.vnidrop.app.core.Transfer
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import com.vnidrop.app.notifications.NotificationPermission
import com.vnidrop.app.ui.feedback.UiMessage
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.feedback.UiText
import com.vnidrop.app.ui.feedback.VniDropSnackbarHost
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.navigation.AppDestination
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.shell.AppShell
import com.vnidrop.app.ui.theme.VniDropTheme
import com.vnidrop.app.ui.theme.LocalVniDropColors
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

@OptIn(ExperimentalTestApi::class)
class FoundationComposeTest {
	@Test
	fun approvalBannerInvokesAcceptAction() = runComposeUiTest {
		var accepted: String? = null
		setContent {
			VniDropTheme(isDarkTheme = false) {
				ApprovalModalHost(
					state = ApprovalState(pending = listOf(approval())),
					onAccept = { accepted = it },
					onRefuse = {},
				)
			}
		}
		onNodeWithText("Approve").performClick()
		runOnIdle { assertEquals("request", accepted) }
	}

	@Test
	fun phoneSettingsNavigatesToNotificationSection() = runComposeUiTest {
		val state = mutableStateOf(SettingsState())
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SettingsScreen(
					state = state.value,
					windowClass = WindowClass.Phone,
					onSectionSelected = { state.value = state.value.copy(selectedSection = it) },
					onUsernameChanged = {},
					onThemeModeChanged = {},
					onChooseFolder = {},
					onResetFolder = {},
					onNotificationsChanged = {},
					onOpenNotificationSettings = {},
					onDiagnosticsChanged = {},
					onBugWhatChanged = {},
					onBugExpectedChanged = {},
					onBugStepsChanged = {},
					onBugContactChanged = {},
					onBugIncludeLogsChanged = {},
					onSubmitBugReport = {},
				)
			}
		}
		onNodeWithText("Notifications").performClick()
		onNodeWithText("Get notified about new receive requests while VniDrop is in the background.").assertIsDisplayed()
	}

	@Test
	fun phoneSettingsOpensCustomRelayConfiguration() = runComposeUiTest {
		val state = mutableStateOf(SettingsState(endpointId = "endpoint-for-allowlist"))
		var applied = false
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SettingsScreen(
					state = state.value,
					windowClass = WindowClass.Phone,
					onSectionSelected = { state.value = state.value.copy(selectedSection = it) },
					onUsernameChanged = {},
					onThemeModeChanged = {},
					onChooseFolder = {},
					onResetFolder = {},
					onNotificationsChanged = {},
					onOpenNotificationSettings = {},
					onDiagnosticsChanged = {},
					onBugWhatChanged = {},
					onBugExpectedChanged = {},
					onBugStepsChanged = {},
					onBugContactChanged = {},
					onBugIncludeLogsChanged = {},
					onSubmitBugReport = {},
					onRelayModeChanged = {
						state.value = state.value.copy(
							relayMode = it,
							relayUrls = state.value.relayUrls.ifEmpty { listOf("") },
						)
					},
					onRelayUrlChanged = { index, value ->
						state.value = state.value.copy(
							relayUrls = state.value.relayUrls.toMutableList().apply { this[index] = value },
						)
					},
					onAddRelayUrl = {
						state.value = state.value.copy(relayUrls = state.value.relayUrls + "")
					},
					onRemoveRelayUrl = { index ->
						state.value = state.value.copy(
							relayUrls = state.value.relayUrls.toMutableList().apply { removeAt(index) }.ifEmpty { listOf("") },
						)
					},
					onApplyRelaySettings = { applied = true },
				)
			}
		}

		onNodeWithText("Network").performClick()
		onNodeWithText("Device ID: endpoint-for-allowlist").assertIsDisplayed()
		onNodeWithText("Strict custom").performClick()
		onNodeWithText(
			"Strict custom mode will not start unless at least one configured relay is reachable. " +
				"VniDrop never uses public relays or public discovery in this mode.",
		).assertIsDisplayed()
		onNodeWithText("Add relay server").assertIsDisplayed()
		onNodeWithText("Apply network settings").performClick()
		runOnIdle { assertTrue(applied) }
	}

	@Test
	fun storageDeleteAllTransfersRequiresConfirmation() = runComposeUiTest {
		var deleteRequested = false
		var cacheClearRequested = false
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SettingsScreen(
					state = SettingsState(selectedSection = SettingsSection.Storage),
					windowClass = WindowClass.Desktop,
					onSectionSelected = {},
					onUsernameChanged = {},
					onThemeModeChanged = {},
					onChooseFolder = {},
					onResetFolder = {},
					onNotificationsChanged = {},
					onOpenNotificationSettings = {},
					onDiagnosticsChanged = {},
					onBugWhatChanged = {},
					onBugExpectedChanged = {},
					onBugStepsChanged = {},
					onBugContactChanged = {},
					onBugIncludeLogsChanged = {},
					onSubmitBugReport = {},
					onDeleteAllTransfers = { deleteRequested = true },
					onClearTransferCache = { cacheClearRequested = true },
				)
			}
		}

		onNodeWithText("Clear transfer cache").performClick()
		onNodeWithText(
			"Removes cached transfer content after briefly restarting VniDrop. " +
				"Finish ongoing transfers and stop active shares first. Received files and transfer history are not deleted.",
		).assertIsDisplayed()
		runOnIdle { assertFalse(cacheClearRequested) }
		onNodeWithTag("confirm-clear-transfer-cache").performClick()
		runOnIdle { assertTrue(cacheClearRequested) }

		onNodeWithText("Delete all transfers").performClick()
		onNodeWithText(
			"This clears all sent and received transfer records from your history and immediately reclaims unused transfer cache. " +
				"Ongoing transfers and received files are not deleted. This can’t be undone.",
		).assertIsDisplayed()
		runOnIdle { assertFalse(deleteRequested) }

		onNodeWithTag("confirm-delete-all-transfers").performClick()
		runOnIdle { assertTrue(deleteRequested) }
	}

	@Test
	fun aboutSettingsShowsTheSharedProductAndPrivacyContent() = runComposeUiTest {
		setContent {
			VniDropTheme(isDarkTheme = false) {
				Box(Modifier.width(393.dp)) {
					SettingsScreen(
						state = SettingsState(selectedSection = SettingsSection.About),
						windowClass = WindowClass.Phone,
						onSectionSelected = {},
						onUsernameChanged = {},
						onThemeModeChanged = {},
						onChooseFolder = {},
						onResetFolder = {},
						onNotificationsChanged = {},
						onOpenNotificationSettings = {},
						onDiagnosticsChanged = {},
						onBugWhatChanged = {},
						onBugExpectedChanged = {},
						onBugStepsChanged = {},
						onBugContactChanged = {},
						onBugIncludeLogsChanged = {},
						onSubmitBugReport = {},
					)
				}
			}
		}

		onNodeWithText("Send files directly. Stay in control of who receives them.").assertIsDisplayed()
		onNodeWithText("What VniDrop is").assertIsDisplayed()
		onNodeWithText("What VniDrop isn’t").assertIsDisplayed()
		onAllNodesWithText("Privacy & security").assertCountEquals(1)
		onAllNodesWithText("Apache 2.0").assertCountEquals(1)
		val explanationBounds = onNodeWithText(
			"A direct device-to-device transfer — your files go straight to the receiver.",
		).getUnclippedBoundsInRoot()
		assertTrue(explanationBounds.bottom - explanationBounds.top > 32.dp)
	}

	@Test
	fun notificationSettingCanBeToggledFromItsRow() = runComposeUiTest {
		var enabled = false
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SettingsScreen(
					state = SettingsState(selectedSection = SettingsSection.Notifications),
					windowClass = WindowClass.Phone,
					onSectionSelected = {},
					onUsernameChanged = {},
					onThemeModeChanged = {},
					onChooseFolder = {},
					onResetFolder = {},
					onNotificationsChanged = { enabled = it },
					onOpenNotificationSettings = {},
					onDiagnosticsChanged = {},
					onBugWhatChanged = {},
					onBugExpectedChanged = {},
					onBugStepsChanged = {},
					onBugContactChanged = {},
					onBugIncludeLogsChanged = {},
					onSubmitBugReport = {},
				)
			}
		}
		onNodeWithText("Allow notifications").performClick()
		runOnIdle { assertEquals(true, enabled) }
	}

	@Test
	fun deniedNotificationSettingOffersSystemSettingsAction() = runComposeUiTest {
		var opened = false
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SettingsScreen(
					state = SettingsState(
						selectedSection = SettingsSection.Notifications,
						notificationPermission = NotificationPermission.Denied,
					),
					windowClass = WindowClass.Phone,
					onSectionSelected = {},
					onUsernameChanged = {},
					onThemeModeChanged = {},
					onChooseFolder = {},
					onResetFolder = {},
					onNotificationsChanged = {},
					onOpenNotificationSettings = { opened = true },
					onDiagnosticsChanged = {},
					onBugWhatChanged = {},
					onBugExpectedChanged = {},
					onBugStepsChanged = {},
					onBugContactChanged = {},
					onBugIncludeLogsChanged = {},
					onSubmitBugReport = {},
				)
			}
		}
		onNodeWithText("Open Settings").performClick()
		runOnIdle { assertTrue(opened) }
	}

	@Test
	fun snackbarDisplaysBufferedMessage() = runComposeUiTest {
		val controller = UiMessageController()
		controller.tryShow(UiMessage(UiText.Dynamic("Saved successfully")))
		setContent {
			VniDropTheme(isDarkTheme = false) { VniDropSnackbarHost(controller) }
		}
		onNodeWithText("Saved successfully").assertIsDisplayed()
		onNodeWithContentDescription("Dismiss").assertIsDisplayed()
	}

	@Test
	fun compactSnackbarMovesActionBelowMessageAndClose() = runComposeUiTest {
		val controller = UiMessageController()
		controller.tryShow(
			UiMessage(
				text = UiText.Dynamic("Notifications are turned off for VniDrop. You can enable them in Settings."),
				actionLabel = UiText.Dynamic("Open Settings"),
			),
		)
		setContent {
			VniDropTheme(isDarkTheme = false) {
				Box(Modifier.width(320.dp)) { VniDropSnackbarHost(controller) }
			}
		}

		val messageBottom = onNodeWithText("Notifications are turned off for VniDrop. You can enable them in Settings.")
			.getUnclippedBoundsInRoot().bottom
		val closeBottom = onNodeWithContentDescription("Dismiss").getUnclippedBoundsInRoot().bottom
		val actionTop = onNodeWithText("Open Settings").getUnclippedBoundsInRoot().top
		assertTrue(messageBottom <= actionTop)
		assertTrue(closeBottom <= actionTop)
	}

	@Test
	fun phoneSnackbarOverlayStopsAboveBottomNavigation() = runComposeUiTest {
		setContent {
			VniDropTheme(isDarkTheme = false) {
				AppShell(
					selectedDestination = AppDestination.Send,
					windowClass = WindowClass.Phone,
					uiPlatform = UiPlatform.Android,
					onDestinationSelected = {},
					overlay = {
						Box(Modifier.align(Alignment.BottomCenter).size(20.dp).testTag("snackbar-overlay"))
					},
					floatingAction = {
						Box(Modifier.align(Alignment.BottomEnd).size(56.dp).testTag("floating-action"))
					},
				) {
					Text("Content")
				}
			}
		}

		val overlayBottom = onNodeWithTag("snackbar-overlay").getUnclippedBoundsInRoot().bottom
		val floatingActionTop = onNodeWithTag("floating-action").getUnclippedBoundsInRoot().top
		val navigationLabelTop = onNodeWithText("Send").getUnclippedBoundsInRoot().top
		assertTrue(overlayBottom <= floatingActionTop)
		assertTrue(overlayBottom <= navigationLabelTop)
	}

	@Test
	fun narrowDesktopWindowKeepsDesktopSourceListNavigation() = runComposeUiTest {
		var selected = AppDestination.Send
		setContent {
			VniDropTheme(isDarkTheme = false) {
				Box(Modifier.size(width = 560.dp, height = 640.dp)) {
					AppShell(
						selectedDestination = selected,
						windowClass = WindowClass.Phone,
						uiPlatform = UiPlatform.Windows,
						onDestinationSelected = { selected = it },
					) {
						Text("Content")
					}
				}
			}
		}

		onNodeWithText("VniDrop").assertIsDisplayed()
		onNodeWithText("Receive").performClick()
		runOnIdle { assertEquals(AppDestination.Receive, selected) }
	}

	@Test
	fun nativeWindowBackdropShowsThroughDesktopChromeButNotMainContent() = runComposeUiTest {
		val sentinel = Color.Magenta
		var expectedMain = Color.Unspecified
		setContent {
			VniDropTheme(isDarkTheme = false) {
				expectedMain = LocalVniDropColors.current.backgroundDashCanvas
				Box(
					Modifier
						.size(width = 320.dp, height = 200.dp)
						.background(sentinel)
						.testTag("native-backdrop-shell"),
				) {
					AppShell(
						selectedDestination = AppDestination.Send,
						windowClass = WindowClass.Desktop,
						uiPlatform = UiPlatform.Windows,
						mainContentTopStartRadius = 20.dp,
						useNativeWindowBackdrop = true,
						onDestinationSelected = {},
					) {
						Text("Content")
					}
				}
			}
		}

		val pixels = onNodeWithTag("native-backdrop-shell").captureToImage().toPixelMap()
		assertEquals(sentinel.toArgb(), pixels[pixels.width / 20, pixels.height * 9 / 10].toArgb())
		assertEquals(expectedMain.toArgb(), pixels[pixels.width * 19 / 20, pixels.height * 9 / 10].toArgb())
	}

	@Test
	fun desktopChromeKeepsSolidFallbackWithoutNativeBackdrop() = runComposeUiTest {
		var expectedSidebar = Color.Unspecified
		setContent {
			VniDropTheme(isDarkTheme = false) {
				expectedSidebar = LocalVniDropColors.current.backgroundSurface200
				Box(
					Modifier
						.size(width = 320.dp, height = 200.dp)
						.background(Color.Magenta)
						.testTag("solid-backdrop-shell"),
				) {
					AppShell(
						selectedDestination = AppDestination.Send,
						windowClass = WindowClass.Desktop,
						uiPlatform = UiPlatform.Windows,
						mainContentTopStartRadius = 20.dp,
						useNativeWindowBackdrop = false,
						onDestinationSelected = {},
					) {
						Text("Content")
					}
				}
			}
		}

		val pixels = onNodeWithTag("solid-backdrop-shell").captureToImage().toPixelMap()
		assertEquals(expectedSidebar.toArgb(), pixels[pixels.width / 20, pixels.height * 9 / 10].toArgb())
	}

	@Test
	fun androidPagesUseStaticFeatureIconsWithoutTitleDescriptions() = runComposeUiTest {
		val actions = object : ReceiveInvitationActions {
			override val fileAvailability = ReceiveMethodAvailability.Hidden
			override val qrAvailability = ReceiveMethodAvailability.Hidden
			override val nfcAvailability = ReceiveMethodAvailability.Hidden
			override fun pickInvitation(onResult: (Result<String>) -> Unit) = Unit
			override fun scanQrCode(onResult: (Result<String>) -> Unit) = Unit
			override fun readNfcInvitation(onResult: (Result<String>) -> Unit) = Unit
			override fun cancel() = Unit
		}
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Android) {
				VniDropTheme(isDarkTheme = false) {
					Row {
						Box(Modifier.size(500.dp)) {
							TransferCatalog(
								transfers = emptyList(),
								transferThumbnails = emptyMap(),
								windowClass = WindowClass.Phone,
								onOpenComposer = {},
								onTransferSelected = {},
							)
						}
						Box(Modifier.size(500.dp)) {
							ReceiveScreen(
								coreState = CoreState(isInitialized = true),
								state = ReceiveState(),
								windowClass = WindowClass.Phone,
								actions = actions,
								onOpenAcquisition = {},
								onDismissAcquisition = {},
								onReceiverNameChanged = {},
								onInvitationResult = { _, _ -> },
								onWaitingForNfc = {},
								onReceive = {},
								onRequestDeleteHistoryItem = {},
								onRequestClearHistory = {},
								onDismissHistoryDelete = {},
								onConfirmHistoryDelete = {},
							)
						}
						Box(Modifier.size(500.dp)) {
							SettingsOverview(SettingsState(), onSectionSelected = {}, largeTitle = false)
						}
					}
				}
			}
		}

		onNodeWithTag("send-empty-icon").assertIsDisplayed()
		onNodeWithTag("receive-empty-icon").assertIsDisplayed()
		onAllNodesWithText("Transfers you’re sharing from this device.").assertCountEquals(0)
		onAllNodesWithText("Transfers you’ve received on this device.").assertCountEquals(0)
		onAllNodesWithText("Your name, where transfers are saved, appearance, and notifications.").assertCountEquals(0)
	}

	@Test
	fun phoneSendEmptyStateOpensCreationDrawer() = runComposeUiTest {
		val state = mutableStateOf(SendState())
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SendScreen(
					coreState = CoreState(isInitialized = true),
					state = state.value,
					windowClass = WindowClass.Phone,
					onOpenComposer = { state.value = state.value.copy(isComposerOpen = true) },
					onDismissComposer = {},
					onSelectFile = {},
					onClearFile = {},
					onTransferNameChanged = {},
					onSenderNameChanged = {},
					onAccessPolicyChanged = {},
					onCreateShare = {},
					onTransferSelected = {},
					onCloseTransferDetails = {},
					onCopyTicket = {},
				)
			}
		}

		onNodeWithText("New transfer").performClick()
		onNodeWithText("Choose what to share").assertIsDisplayed()
		onNodeWithText("Choose files").assertIsDisplayed()
	}

	@Test
	fun desktopTransferComposerReviewsFileAndAccessPolicy() = runComposeUiTest {
		var selectedPolicy: ShareAccessPolicy? = null
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SendScreen(
					coreState = CoreState(isInitialized = true),
					state = SendState(
						isComposerOpen = true,
						selectedFiles = listOf(PickedShareFile("/tmp/photos.zip", "photos.zip", 1536UL)),
						transferName = "photos.zip",
						senderName = "Sender",
					),
					windowClass = WindowClass.Desktop,
					onOpenComposer = {},
					onDismissComposer = {},
					onSelectFile = {},
					onClearFile = {},
					onTransferNameChanged = {},
					onSenderNameChanged = {},
					onAccessPolicyChanged = { selectedPolicy = it },
					onCreateShare = {},
					onTransferSelected = {},
					onCloseTransferDetails = {},
					onCopyTicket = {},
				)
			}
		}

		onNodeWithText("1.5 KB").assertIsDisplayed()
		onNodeWithText("Anyone with this transfer").performClick()
		runOnIdle { assertEquals(ShareAccessPolicy.AnyoneWithTransfer, selectedPolicy) }
	}

	@Test
	fun transferCatalogOpensSelectedTransfer() = runComposeUiTest {
		var selectedId: ULong? = null
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SendScreen(
					coreState = CoreState(isInitialized = true, transfers = listOf(outgoingTransfer())),
					state = SendState(),
					windowClass = WindowClass.Phone,
					onOpenComposer = {},
					onDismissComposer = {},
					onSelectFile = {},
					onClearFile = {},
					onTransferNameChanged = {},
					onSenderNameChanged = {},
					onAccessPolicyChanged = {},
					onCreateShare = {},
					onTransferSelected = { selectedId = it },
					onCloseTransferDetails = {},
					onCopyTicket = {},
				)
			}
		}

		val titleBounds = onNodeWithText("Photos").getUnclippedBoundsInRoot()
		val statusBounds = onNodeWithText("Available").getUnclippedBoundsInRoot()
		assertTrue(statusBounds.left - titleBounds.right <= 12.dp)
		onNodeWithText("Photos").performClick()
		runOnIdle { assertEquals(9UL, selectedId) }
	}

	@Test
	fun transferDetailsRevealSharingOnlyAfterSelection() = runComposeUiTest {
		val state = mutableStateOf(SendState(selectedTransferId = 9UL))
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SendScreen(
					coreState = CoreState(isInitialized = true, transfers = listOf(outgoingTransfer())),
					state = state.value,
					windowClass = WindowClass.Desktop,
					onOpenComposer = {}, onDismissComposer = {}, onSelectFile = {}, onClearFile = {},
					onTransferNameChanged = {}, onSenderNameChanged = {}, onAccessPolicyChanged = {},
					onCreateShare = {}, onTransferSelected = {}, onCloseTransferDetails = {}, onCopyTicket = {},
					onShare = { state.value = state.value.copy(detailPanel = com.vnidrop.app.feature.send.TransferDetailPanel.Share) },
				)
			}
		}

		onNodeWithText("Share").assertIsDisplayed()
		onAllNodesWithText("Scan with VniDrop to receive this transfer").assertCountEquals(0)
		onNode(hasText("Share") and hasClickAction()).performClick()
		runOnIdle { assertEquals(com.vnidrop.app.feature.send.TransferDetailPanel.Share, state.value.detailPanel) }
		onNodeWithText("Scan with VniDrop to receive this transfer").assertIsDisplayed()
		onNodeWithText("Save .vnd file").assertIsDisplayed()
		onNodeWithContentDescription("Close").assertIsDisplayed()
	}

	@Test
	fun stoppedAndFailedTransfersDoNotExposeStaleInvitations() = runComposeUiTest {
		val transfer = mutableStateOf(outgoingTransfer().copy(status = TransferStatus.Stopped))
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SendScreen(
					coreState = CoreState(isInitialized = true, transfers = listOf(transfer.value)),
					state = SendState(
						selectedTransferId = transfer.value.transferId,
						detailPanel = com.vnidrop.app.feature.send.TransferDetailPanel.Share,
					),
					windowClass = WindowClass.Desktop,
					onOpenComposer = {}, onDismissComposer = {}, onSelectFile = {}, onClearFile = {},
					onTransferNameChanged = {}, onSenderNameChanged = {}, onAccessPolicyChanged = {},
					onCreateShare = {}, onTransferSelected = {}, onCloseTransferDetails = {}, onCopyTicket = {},
				)
			}
		}

		onAllNodesWithText("Share").assertCountEquals(0)
		onAllNodesWithText("Save .vnd file").assertCountEquals(0)

		runOnIdle { transfer.value = transfer.value.copy(status = TransferStatus.Failed) }
		onAllNodesWithText("Share").assertCountEquals(0)
		onAllNodesWithText("Save .vnd file").assertCountEquals(0)
	}

	@Test
	fun oversizedInvitationShowsQrUnavailableInsteadOfLoadingForever() = runComposeUiTest {
		val transfer = outgoingTransfer().copy(ticket = "a".repeat(2_954))
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SendScreen(
					coreState = CoreState(isInitialized = true, transfers = listOf(transfer)),
					state = SendState(
						selectedTransferId = transfer.transferId,
						detailPanel = com.vnidrop.app.feature.send.TransferDetailPanel.Share,
					),
					windowClass = WindowClass.Desktop,
					onOpenComposer = {}, onDismissComposer = {}, onSelectFile = {}, onClearFile = {},
					onTransferNameChanged = {}, onSenderNameChanged = {}, onAccessPolicyChanged = {},
					onCreateShare = {}, onTransferSelected = {}, onCloseTransferDetails = {}, onCopyTicket = {},
				)
			}
		}

		onNodeWithText("QR unavailable for this invitation. Use Share or Download instead.").assertIsDisplayed()
		onNodeWithText("Save .vnd file").assertIsDisplayed()
	}

	@Test
	fun phoneReceiveEmptyStateOpensAcquisitionMethods() = runComposeUiTest {
		val state = mutableStateOf(ReceiveState())
		val actions = object : ReceiveInvitationActions {
			override val fileAvailability = ReceiveMethodAvailability.Available
			override val qrAvailability = ReceiveMethodAvailability.Hidden
			override val nfcAvailability = ReceiveMethodAvailability.Hidden
			override fun pickInvitation(onResult: (Result<String>) -> Unit) = Unit
			override fun scanQrCode(onResult: (Result<String>) -> Unit) = Unit
			override fun readNfcInvitation(onResult: (Result<String>) -> Unit) = Unit
			override fun cancel() = Unit
		}
		setContent {
			VniDropTheme(isDarkTheme = false) {
				ReceiveScreen(
					coreState = CoreState(isInitialized = true),
					state = state.value,
					windowClass = WindowClass.Phone,
					actions = actions,
					onOpenAcquisition = { state.value = state.value.copy(isAcquisitionOpen = true) },
					onDismissAcquisition = {},
					onReceiverNameChanged = {},
					onInvitationResult = { _, _ -> },
					onWaitingForNfc = {},
					onReceive = {},
					onRequestDeleteHistoryItem = {},
					onRequestClearHistory = {},
					onDismissHistoryDelete = {},
					onConfirmHistoryDelete = {},
				)
			}
		}

		onNodeWithText("Nothing received yet").assertIsDisplayed()
		onNodeWithText("Start receiving").performClick()
		onNodeWithText("How would you like to connect?").assertIsDisplayed()
		onNodeWithText("Open a .vnd invitation").assertIsDisplayed()
	}

	@Test
	fun receiveHistoryOffersPerItemDeleteAndConfirmedClearAll() = runComposeUiTest {
		val state = mutableStateOf(ReceiveState())
		val actions = object : ReceiveInvitationActions {
			override val fileAvailability = ReceiveMethodAvailability.Available
			override val qrAvailability = ReceiveMethodAvailability.Hidden
			override val nfcAvailability = ReceiveMethodAvailability.Hidden
			override fun pickInvitation(onResult: (Result<String>) -> Unit) = Unit
			override fun scanQrCode(onResult: (Result<String>) -> Unit) = Unit
			override fun readNfcInvitation(onResult: (Result<String>) -> Unit) = Unit
			override fun cancel() = Unit
		}
		setContent {
			VniDropTheme(isDarkTheme = false) {
				ReceiveScreen(
					coreState = CoreState(isInitialized = true, transfers = listOf(receivedTransfer())),
					state = state.value,
					windowClass = WindowClass.Phone,
					actions = actions,
					onOpenAcquisition = {},
					onDismissAcquisition = {},
					onReceiverNameChanged = {},
					onInvitationResult = { _, _ -> },
					onWaitingForNfc = {},
					onReceive = {},
					onRequestDeleteHistoryItem = { state.value = state.value.copy(historyDeleteTarget = ReceiveHistoryDeleteTarget.Transfer(it)) },
					onRequestClearHistory = { state.value = state.value.copy(historyDeleteTarget = ReceiveHistoryDeleteTarget.All) },
					onDismissHistoryDelete = { state.value = state.value.copy(historyDeleteTarget = null) },
					onConfirmHistoryDelete = {},
				)
			}
		}

		onNodeWithContentDescription("Delete from receive history").assertIsDisplayed()
		onNodeWithText("Clear history").performClick()
		onNodeWithText("Clear receive history?").assertIsDisplayed()
		onNodeWithText("Downloaded files will remain on this device.", substring = true).assertIsDisplayed()
		onNodeWithContentDescription("Close").assertIsDisplayed()
	}

	@Test
	fun snackbarActionAndCancellationAreForwarded() = runComposeUiTest {
		val controller = UiMessageController()
		var actionCount = 0
		controller.tryShow(
			UiMessage(
				text = UiText.Dynamic("Undoable action"),
				actionLabel = UiText.Dynamic("Undo"),
				onAction = { actionCount += 1 },
			),
		)
		setContent { VniDropTheme(isDarkTheme = false) { VniDropSnackbarHost(controller) } }
		onNodeWithText("Undo").performClick()
		runOnIdle { assertEquals(1, actionCount) }

		controller.tryShow(UiMessage(UiText.Dynamic("Dismiss me")))
		onNodeWithText("Dismiss me").assertIsDisplayed()
		controller.dismissCurrent()
		onAllNodesWithText("Dismiss me").assertCountEquals(0)
	}

	private fun approval() = PendingApproval(
		id = "request",
		transferId = 1UL,
		transferName = "Photos",
		receiverName = "Alice",
		receiverDeviceName = "Phone",
		remoteEndpointId = "endpoint-alice",
		requestedAt = 1L,
	)

	private fun outgoingTransfer() = Transfer(
		localId = "local-9",
		transferId = 9UL,
		direction = TransferDirection.Send,
		status = TransferStatus.Sharing,
		peerId = null,
		transferName = "Photos",
		contentHash = "hash",
		fileCount = 1UL,
		totalSize = 1536UL,
		ticket = "ticket",
		accessPolicy = ShareAccessPolicy.RequireApproval,
		createdAt = 1L,
		updatedAt = 1L,
	)

	private fun receivedTransfer() = Transfer(
		localId = "receive-10",
		transferId = 10UL,
		direction = TransferDirection.Receive,
		status = TransferStatus.Done,
		peerId = "sender",
		transferName = "Holiday photos",
		contentHash = "received-hash",
		fileCount = 3UL,
		totalSize = 4096UL,
		ticket = null,
		accessPolicy = ShareAccessPolicy.RequireApproval,
		createdAt = 1L,
		updatedAt = 2L,
	)
}
