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
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performMouseInput
import androidx.compose.ui.test.rightClick
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.v2.runComposeUiTest
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import com.vnidrop.app.feature.approvals.ApprovalModalHost
import com.vnidrop.app.feature.approvals.ApprovalState
import com.vnidrop.app.feature.approvals.PendingApproval
import com.vnidrop.app.feature.saveddevices.TargetedOfferModalHost
import com.vnidrop.app.feature.saveddevices.TargetedOfferState
import com.vnidrop.app.feature.receive.ReceiveHistoryDeleteTarget
import com.vnidrop.app.feature.receive.ReceiveInvitationActions
import com.vnidrop.app.feature.receive.ReceiveMethodAvailability
import com.vnidrop.app.feature.receive.ReceiveScreen
import com.vnidrop.app.feature.receive.ReceiveState
import com.vnidrop.app.feature.settings.SettingsScreen
import com.vnidrop.app.feature.settings.SettingsSection
import com.vnidrop.app.feature.settings.SettingsState
import com.vnidrop.app.feature.settings.StorageBreakdown
import com.vnidrop.app.feature.settings.SettingsOverview
import com.vnidrop.app.feature.send.SendScreen
import com.vnidrop.app.feature.send.SendState
import com.vnidrop.app.feature.send.DraftSourceId
import com.vnidrop.app.feature.send.TransferComposer
import com.vnidrop.app.feature.send.TransferDraftDestination
import com.vnidrop.app.feature.send.TransferDraftSource
import com.vnidrop.app.feature.send.TransferDraftState
import com.vnidrop.app.feature.send.TransferCatalog
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.PendingTargetedOfferModel
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
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.getString
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.about_is_direct
import vnidrop.shared.generated.resources.about_is_title
import vnidrop.shared.generated.resources.about_isnt_title
import vnidrop.shared.generated.resources.about_privacy_title
import vnidrop.shared.generated.resources.about_tagline
import vnidrop.shared.generated.resources.approval_endpoint_id
import vnidrop.shared.generated.resources.button_approve
import vnidrop.shared.generated.resources.button_cancel
import vnidrop.shared.generated.resources.button_choose_files
import vnidrop.shared.generated.resources.button_close
import vnidrop.shared.generated.resources.button_create_new_transfer
import vnidrop.shared.generated.resources.button_download_invitation
import vnidrop.shared.generated.resources.button_more_actions
import vnidrop.shared.generated.resources.button_open_settings
import vnidrop.shared.generated.resources.button_receive_files
import vnidrop.shared.generated.resources.nav_receive
import vnidrop.shared.generated.resources.nav_saved_devices
import vnidrop.shared.generated.resources.nav_send
import vnidrop.shared.generated.resources.notifications_description
import vnidrop.shared.generated.resources.notifications_enable_action
import vnidrop.shared.generated.resources.notifications_local_title
import vnidrop.shared.generated.resources.notifications_request_prompt
import vnidrop.shared.generated.resources.notifications_title
import vnidrop.shared.generated.resources.preferences_title
import vnidrop.shared.generated.resources.receive_choose_method_title
import vnidrop.shared.generated.resources.receive_clear_history
import vnidrop.shared.generated.resources.receive_clear_history_description
import vnidrop.shared.generated.resources.receive_clear_history_title
import vnidrop.shared.generated.resources.receive_delete_history_item
import vnidrop.shared.generated.resources.receive_delete_history_description
import vnidrop.shared.generated.resources.receive_empty_title
import vnidrop.shared.generated.resources.receive_method_file
import vnidrop.shared.generated.resources.receive_new_subtitle
import vnidrop.shared.generated.resources.receive_title
import vnidrop.shared.generated.resources.relay_add_url
import vnidrop.shared.generated.resources.relay_apply
import vnidrop.shared.generated.resources.relay_mode_automatic
import vnidrop.shared.generated.resources.relay_mode_custom
import vnidrop.shared.generated.resources.relay_strict_warning
import vnidrop.shared.generated.resources.send_access_anyone
import vnidrop.shared.generated.resources.send_choose_file_title
import vnidrop.shared.generated.resources.send_subtitle
import vnidrop.shared.generated.resources.send_title
import vnidrop.shared.generated.resources.settings_network_title
import vnidrop.shared.generated.resources.settings_subtitle
import vnidrop.shared.generated.resources.settings_title
import vnidrop.shared.generated.resources.snackbar_dismiss
import vnidrop.shared.generated.resources.status_available
import vnidrop.shared.generated.resources.storage_calculating
import vnidrop.shared.generated.resources.storage_clear_transfer_cache
import vnidrop.shared.generated.resources.storage_clear_transfer_cache_description
import vnidrop.shared.generated.resources.storage_delete_transfers
import vnidrop.shared.generated.resources.storage_delete_transfers_description
import vnidrop.shared.generated.resources.storage_received_files
import vnidrop.shared.generated.resources.storage_transfer_data
import vnidrop.shared.generated.resources.transfer_qr_unavailable
import vnidrop.shared.generated.resources.transfer_scan_qr
import vnidrop.shared.generated.resources.transfer_share_title
import vnidrop.shared.generated.resources.transfer_delete_description

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
		onNodeWithText(Res.string.button_approve.value).performClick()
		runOnIdle { assertEquals("request", accepted) }
	}

	@Test
	fun approvalPromptOffersNotificationEnableAction() = runComposeUiTest {
		var enabled = false
		setContent {
			VniDropTheme(isDarkTheme = false) {
				ApprovalModalHost(
					state = ApprovalState(pending = listOf(approval())),
					onAccept = {},
					onRefuse = {},
					showNotificationPrompt = true,
					onEnableNotifications = { enabled = true },
				)
			}
		}

		onNodeWithText(Res.string.notifications_request_prompt.value).assertIsDisplayed()
		onNodeWithText(Res.string.notifications_enable_action.value).performClick()
		runOnIdle { assertTrue(enabled) }
	}

	@Test
	fun targetedOfferPromptOffersNotificationEnableAction() = runComposeUiTest {
		var enabled = false
		setContent {
			VniDropTheme(isDarkTheme = false) {
				TargetedOfferModalHost(
					state = TargetedOfferState(
						pending = listOf(
							PendingTargetedOfferModel(
								transferId = "targeted-1",
								senderEndpointId = "sender",
								receiverEndpointId = "receiver",
								manifestId = "manifest",
								contentHash = "hash",
								transferName = "Photos",
								fileCount = 1UL,
								totalSize = 1UL,
								protocolVersion = 1U,
								receivedAt = 1L,
							),
						),
					),
					onAccept = {},
					onDecline = {},
					showNotificationPrompt = true,
					onEnableNotifications = { enabled = true },
				)
			}
		}

		onNodeWithText(Res.string.notifications_request_prompt.value).assertIsDisplayed()
		onNodeWithText(Res.string.notifications_enable_action.value).performClick()
		runOnIdle { assertTrue(enabled) }
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
					onBugWhatChanged = {},
					onBugExpectedChanged = {},
					onBugStepsChanged = {},
					onBugContactChanged = {},
					onBugIncludeLogsChanged = {},
					onSubmitBugReport = {},
				)
			}
		}
		onNodeWithText(Res.string.notifications_title.value).performClick()
		onNodeWithText(Res.string.notifications_description.value).assertIsDisplayed()
	}

	@Test
	fun phoneSettingsPlaceLongValuesBelowStableRowTitles() = runComposeUiTest {
		val longUsername = "A very long device username that must not replace the row title"
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Android) {
				VniDropTheme(isDarkTheme = false) {
					Box(Modifier.width(320.dp)) {
						SettingsOverview(
							state = SettingsState(username = longUsername),
							onSectionSelected = {},
						)
					}
				}
			}
		}

		val preferencesBounds = onNodeWithText(Res.string.preferences_title.value, useUnmergedTree = true).getUnclippedBoundsInRoot()
		val usernameBounds = onNodeWithText(longUsername, useUnmergedTree = true).getUnclippedBoundsInRoot()
		val networkBounds = onNodeWithText(Res.string.settings_network_title.value, useUnmergedTree = true).getUnclippedBoundsInRoot()
		val modeBounds = onNodeWithText(Res.string.relay_mode_automatic.value, useUnmergedTree = true).getUnclippedBoundsInRoot()
		assertTrue(preferencesBounds.bottom <= usernameBounds.top)
		assertTrue(networkBounds.bottom <= modeBounds.top)
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

		onNodeWithText(Res.string.settings_network_title.value).performClick()
		onNodeWithText(Res.string.approval_endpoint_id.value("endpoint-for-allowlist")).assertIsDisplayed()
		onNodeWithText(Res.string.relay_mode_custom.value).performClick()
		onNodeWithText(Res.string.relay_strict_warning.value).assertIsDisplayed()
		onNodeWithText(Res.string.relay_add_url.value).assertIsDisplayed()
		onNodeWithText(Res.string.relay_apply.value).performClick()
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

		onNodeWithText(Res.string.storage_clear_transfer_cache.value).performClick()
		onNodeWithText(Res.string.storage_clear_transfer_cache_description.value).assertIsDisplayed()
		runOnIdle { assertFalse(cacheClearRequested) }
		onNodeWithTag("confirm-clear-transfer-cache").performClick()
		runOnIdle { assertTrue(cacheClearRequested) }

		onNodeWithText(Res.string.storage_delete_transfers.value).performClick()
		onNodeWithText(Res.string.storage_delete_transfers_description.value).assertIsDisplayed()
		runOnIdle { assertFalse(deleteRequested) }

		onNodeWithTag("confirm-delete-all-transfers").performClick()
		runOnIdle { assertTrue(deleteRequested) }
	}

	@Test
	fun storageKeepsCurrentUsageVisibleWhileRefreshing() = runComposeUiTest {
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SettingsScreen(
					state = SettingsState(
						selectedSection = SettingsSection.Storage,
						isCalculatingStorage = true,
						storage = StorageBreakdown(
							transferCacheBytes = 1UL,
							appDataBytes = 2UL,
							temporaryBytes = 3UL,
							receivedBytes = 4UL,
							receivedFileCount = 1,
							missingReceivedFileCount = 0,
							inaccessibleReceivedFileCount = 0,
						),
					),
					windowClass = WindowClass.Desktop,
					onSectionSelected = {},
					onUsernameChanged = {},
					onThemeModeChanged = {},
					onChooseFolder = {},
					onResetFolder = {},
					onNotificationsChanged = {},
					onOpenNotificationSettings = {},
					onBugWhatChanged = {},
					onBugExpectedChanged = {},
					onBugStepsChanged = {},
					onBugContactChanged = {},
					onBugIncludeLogsChanged = {},
					onSubmitBugReport = {},
				)
			}
		}

		onNodeWithText(Res.string.storage_received_files.value).assertIsDisplayed()
		onNodeWithText(Res.string.storage_transfer_data.value).assertIsDisplayed()
		onAllNodesWithText(Res.string.storage_calculating.value).assertCountEquals(0)
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

		onNodeWithText(Res.string.about_tagline.value).assertIsDisplayed()
		onNodeWithText(Res.string.about_is_title.value).assertIsDisplayed()
		onAllNodesWithText(Res.string.about_isnt_title.value).assertCountEquals(1)
		onAllNodesWithText(Res.string.about_privacy_title.value).assertCountEquals(1)
		onAllNodesWithText("Apache 2.0").assertCountEquals(1)
		val explanationBounds = onNodeWithText(Res.string.about_is_direct.value).getUnclippedBoundsInRoot()
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
					onBugWhatChanged = {},
					onBugExpectedChanged = {},
					onBugStepsChanged = {},
					onBugContactChanged = {},
					onBugIncludeLogsChanged = {},
					onSubmitBugReport = {},
				)
			}
		}
		onNodeWithText(Res.string.notifications_local_title.value).performClick()
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
					onBugWhatChanged = {},
					onBugExpectedChanged = {},
					onBugStepsChanged = {},
					onBugContactChanged = {},
					onBugIncludeLogsChanged = {},
					onSubmitBugReport = {},
				)
			}
		}
		onNodeWithText(Res.string.button_open_settings.value).performClick()
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
		onNodeWithContentDescription(Res.string.snackbar_dismiss.value).assertIsDisplayed()
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
		val closeBottom = onNodeWithContentDescription(Res.string.snackbar_dismiss.value).getUnclippedBoundsInRoot().bottom
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
		val navigationLabelTop = onNodeWithText(Res.string.nav_send.value).getUnclippedBoundsInRoot().top
		assertTrue(overlayBottom <= floatingActionTop)
		assertTrue(overlayBottom <= navigationLabelTop)
	}

	@Test
	fun androidBottomNavigationPromotesSavedDevicesAsAProductDestination() = runComposeUiTest {
		var selected = AppDestination.Send
		setContent {
			VniDropTheme(isDarkTheme = false) {
				AppShell(
					selectedDestination = selected,
					windowClass = WindowClass.Phone,
					uiPlatform = UiPlatform.Android,
					onDestinationSelected = { selected = it },
				) {
					Text("Content")
				}
			}
		}

		onNodeWithText(Res.string.nav_saved_devices.value).assertIsDisplayed().performClick()
		runOnIdle { assertEquals(AppDestination.SavedDevices, selected) }
	}

	@Test
	fun androidBottomNavigationKeepsLabelColorStableWhenSelectionChanges() = runComposeUiTest {
		val selected = mutableStateOf(AppDestination.Send)
		setContent {
			VniDropTheme(isDarkTheme = false) {
				AppShell(
					selectedDestination = selected.value,
					windowClass = WindowClass.Phone,
					uiPlatform = UiPlatform.Android,
					onDestinationSelected = { selected.value = it },
				) {
					Text("Content")
				}
			}
		}

		val selectedLabel = onNodeWithText(Res.string.nav_send.value, useUnmergedTree = true).captureToImage().toPixelMap()
		val selectedIndicator = onNodeWithTag("bottom-nav-indicator-Send", useUnmergedTree = true).captureToImage().toPixelMap()
		runOnIdle { selected.value = AppDestination.Receive }
		waitForIdle()
		val unselectedLabel = onNodeWithText(Res.string.nav_send.value, useUnmergedTree = true).captureToImage().toPixelMap()
		val unselectedIndicator = onNodeWithTag("bottom-nav-indicator-Send", useUnmergedTree = true).captureToImage().toPixelMap()

		assertEquals(selectedLabel.width, unselectedLabel.width)
		assertEquals(selectedLabel.height, unselectedLabel.height)
		for (x in 0 until selectedLabel.width) {
			for (y in 0 until selectedLabel.height) {
				assertEquals(selectedLabel[x, y].toArgb(), unselectedLabel[x, y].toArgb())
			}
		}
		assertFalse(selectedIndicator[2, selectedIndicator.height / 2].toArgb() == unselectedIndicator[2, unselectedIndicator.height / 2].toArgb())
		assertEquals(
			selectedIndicator[8, selectedIndicator.height / 2].toArgb(),
			selectedIndicator[selectedIndicator.width - 9, selectedIndicator.height / 2].toArgb(),
		)
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
		onNodeWithText(Res.string.nav_receive.value).performClick()
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
						Box(Modifier.size(width = 300.dp, height = 500.dp)) {
							TransferCatalog(
								transfers = emptyList(),
								transferThumbnails = emptyMap(),
								windowClass = WindowClass.Phone,
								onOpenComposer = {},
								onTransferSelected = {},
							)
						}
						Box(Modifier.size(width = 300.dp, height = 500.dp)) {
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
						Box(Modifier.size(width = 300.dp, height = 500.dp)) {
							SettingsOverview(SettingsState(), onSectionSelected = {})
						}
					}
				}
			}
		}

		onNodeWithTag("send-empty-icon").assertIsDisplayed()
		onNodeWithTag("receive-empty-icon").assertIsDisplayed()
		val sendIconBounds = onNodeWithTag("send-empty-icon").getUnclippedBoundsInRoot()
		val receiveIconBounds = onNodeWithTag("receive-empty-icon").getUnclippedBoundsInRoot()
		assertEquals(36.dp, sendIconBounds.bottom - sendIconBounds.top)
		assertEquals(36.dp, receiveIconBounds.bottom - receiveIconBounds.top)
		onNodeWithTag("send-empty-action-icon", useUnmergedTree = true).assertIsDisplayed()
		onNodeWithTag("receive-empty-action-icon", useUnmergedTree = true).assertIsDisplayed()
		val sendTitleBounds = onNodeWithText(Res.string.send_title.value, useUnmergedTree = true).getUnclippedBoundsInRoot()
		val receiveTitleBounds = onNodeWithText(Res.string.receive_title.value, useUnmergedTree = true).getUnclippedBoundsInRoot()
		val settingsTitleBounds = onNodeWithText(Res.string.settings_title.value, useUnmergedTree = true).getUnclippedBoundsInRoot()
		val titleHeight = sendTitleBounds.bottom - sendTitleBounds.top
		assertEquals(titleHeight, receiveTitleBounds.bottom - receiveTitleBounds.top)
		assertEquals(titleHeight, settingsTitleBounds.bottom - settingsTitleBounds.top)
		onAllNodesWithText(Res.string.send_subtitle.value).assertCountEquals(0)
		onAllNodesWithText(Res.string.receive_new_subtitle.value).assertCountEquals(0)
		onAllNodesWithText(Res.string.settings_subtitle.value).assertCountEquals(0)
	}

	@Test
	fun phoneTransferComposerShowsSourceChoices() = runComposeUiTest {
		setContent {
			VniDropTheme(isDarkTheme = false) {
				TransferComposer(
					coreInitialized = true,
					state = TransferDraftState(destination = TransferDraftDestination.Invitation),
					windowClass = WindowClass.Phone,
					onSelectFile = {},
					onSelectFolder = {},
					onClearFile = {},
					onRemoveFile = {},
					onTransferNameChanged = {},
					onSenderNameChanged = {},
					onAccessPolicyChanged = {},
					onSubmit = {},
				)
			}
		}

		onNodeWithText(Res.string.send_choose_file_title.value).assertIsDisplayed()
		onNodeWithText(Res.string.button_choose_files.value).assertIsDisplayed()
	}

	@Test
	fun desktopTransferComposerReviewsFileAndAccessPolicy() = runComposeUiTest {
		var selectedPolicy: ShareAccessPolicy? = null
		setContent {
			VniDropTheme(isDarkTheme = false) {
				TransferComposer(
					coreInitialized = true,
					state = TransferDraftState(
						destination = TransferDraftDestination.Invitation,
						sources = listOf(TransferDraftSource(DraftSourceId("source-1"), "photos.zip", 1536UL, null, false)),
						transferName = "photos.zip",
						senderName = "Sender",
					),
					windowClass = WindowClass.Desktop,
					onSelectFile = {},
					onSelectFolder = {},
					onClearFile = {},
					onRemoveFile = {},
					onTransferNameChanged = {},
					onSenderNameChanged = {},
					onAccessPolicyChanged = { selectedPolicy = it },
					onSubmit = {},
				)
			}
		}

		onNodeWithText("1.5 KB").assertIsDisplayed()
		onNodeWithText(Res.string.send_access_anyone.value).performClick()
		runOnIdle { assertEquals(ShareAccessPolicy.AnyoneWithTransfer, selectedPolicy) }
	}

	@Test
	fun transferCatalogOpensSelectedTransfer() = runComposeUiTest {
		var selectedId: ULong? = null
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Windows) {
				VniDropTheme(isDarkTheme = false) {
					SendScreen(
						coreState = CoreState(isInitialized = true, transfers = listOf(outgoingTransfer())),
						state = SendState(),
						windowClass = WindowClass.Desktop,
						onOpenComposer = {},
						onTransferSelected = { selectedId = it },
						onCloseTransferDetails = {},
						onCopyTicket = {},
					)
				}
			}
		}

		val titleBounds = onNodeWithText("Photos").getUnclippedBoundsInRoot()
		val statusBounds = onNodeWithText(Res.string.status_available.value).getUnclippedBoundsInRoot()
		assertTrue(statusBounds.left - titleBounds.right <= 12.dp)
		onNodeWithTag("send-header-action-icon", useUnmergedTree = true).assertIsDisplayed()
		onNodeWithText("Photos").performClick()
		runOnIdle { assertEquals(9UL, selectedId) }
	}

	@Test
	fun desktopSendRowsUseRightClickActionsWithoutThreeDotButtons() = runComposeUiTest {
		var sharedTransfer = false
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Windows) {
				VniDropTheme(isDarkTheme = true) {
					TransferCatalog(
						transfers = listOf(outgoingTransfer()),
						transferThumbnails = emptyMap(),
						windowClass = WindowClass.Desktop,
						onOpenComposer = {},
						onTransferSelected = {},
						onShare = { sharedTransfer = true },
					)
				}
			}
		}

		onAllNodesWithContentDescription(Res.string.button_more_actions.value).assertCountEquals(0)
		onNodeWithText("Photos").performMouseInput { rightClick() }
		val menuItem = onNodeWithText(Res.string.transfer_share_title.value).assertIsDisplayed()
		val menuImage = menuItem.captureToImage().toPixelMap()
		val brightPixels = (0 until menuImage.width).sumOf { x ->
			(0 until menuImage.height).count { y ->
				val pixel = menuImage[x, y]
				pixel.red > 0.95f && pixel.green > 0.95f && pixel.blue > 0.95f
			}
		}
		assertTrue(brightPixels < menuImage.width * menuImage.height / 2)
		menuItem.performClick()
		runOnIdle { assertTrue(sharedTransfer) }
	}

	@Test
	fun desktopDeleteConfirmationIsNarrowAndEmphasizesTheTransferName() = runComposeUiTest {
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Windows) {
				VniDropTheme(isDarkTheme = false) {
					Box(Modifier.size(width = 1200.dp, height = 800.dp)) {
						SendScreen(
							coreState = CoreState(isInitialized = true, transfers = listOf(outgoingTransfer())),
							state = SendState(isDeleteConfirmationOpen = true, deleteTargetTransferId = 9UL),
							windowClass = WindowClass.Desktop,
							onOpenComposer = {},
							onTransferSelected = {},
							onCloseTransferDetails = {},
							onCopyTicket = {},
						)
					}
				}
			}
		}

		val description = Res.string.transfer_delete_description.value("Photos")
		assertFalse(description.contains('“') || description.contains('”'))
		val dialogBounds = onNodeWithTag("adaptive-dialog-surface").getUnclippedBoundsInRoot()
		val dialogWidth = dialogBounds.right - dialogBounds.left
		assertTrue(dialogWidth <= 440.dp)
		val annotatedText = onNodeWithText(description).fetchSemanticsNode().config[SemanticsProperties.Text].single()
		assertTrue(annotatedText.spanStyles.any { range ->
			range.item.fontWeight == androidx.compose.ui.text.font.FontWeight.Bold &&
				annotatedText.substring(range.start, range.end) == "Photos"
		})
	}

	@Test
	fun transferDetailsRevealSharingOnlyAfterSelection() = runComposeUiTest {
		val state = mutableStateOf(SendState(selectedTransferId = 9UL))
		var destructiveColor = Color.Unspecified
		setContent {
			VniDropTheme(isDarkTheme = false) {
				destructiveColor = LocalVniDropColors.current.destructiveDefault
				SendScreen(
					coreState = CoreState(isInitialized = true, transfers = listOf(outgoingTransfer())),
					state = state.value,
					windowClass = WindowClass.Desktop,
					onOpenComposer = {}, onTransferSelected = {}, onCloseTransferDetails = {}, onCopyTicket = {},
					onShare = { state.value = state.value.copy(detailPanel = com.vnidrop.app.feature.send.TransferDetailPanel.Share) },
				)
			}
		}

		onNodeWithContentDescription(Res.string.transfer_share_title.value).assertIsDisplayed()
		listOf("transfer-stop-sharing-action", "transfer-delete-action").forEach { tag ->
			val pixels = onNodeWithTag(tag).captureToImage().toPixelMap()
			assertFalse(pixels[8, pixels.height / 2].toArgb() == destructiveColor.toArgb())
		}
		onAllNodesWithText(Res.string.transfer_scan_qr.value).assertCountEquals(0)
		onNodeWithContentDescription(Res.string.transfer_share_title.value).performClick()
		runOnIdle { assertEquals(com.vnidrop.app.feature.send.TransferDetailPanel.Share, state.value.detailPanel) }
		waitUntil(timeoutMillis = 5_000) {
			onAllNodesWithText(Res.string.transfer_scan_qr.value).fetchSemanticsNodes().isNotEmpty()
		}
		onNodeWithText(Res.string.transfer_scan_qr.value).assertIsDisplayed()
		onNodeWithText(Res.string.button_download_invitation.value).assertIsDisplayed()
		onNodeWithContentDescription(Res.string.button_close.value).assertIsDisplayed()
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
					onOpenComposer = {}, onTransferSelected = {}, onCloseTransferDetails = {}, onCopyTicket = {},
				)
			}
		}

		onAllNodesWithText(Res.string.transfer_share_title.value).assertCountEquals(0)
		onAllNodesWithText(Res.string.button_download_invitation.value).assertCountEquals(0)

		runOnIdle { transfer.value = transfer.value.copy(status = TransferStatus.Failed) }
		onAllNodesWithText(Res.string.transfer_share_title.value).assertCountEquals(0)
		onAllNodesWithText(Res.string.button_download_invitation.value).assertCountEquals(0)
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
					onOpenComposer = {}, onTransferSelected = {}, onCloseTransferDetails = {}, onCopyTicket = {},
				)
			}
		}

		onNodeWithText(Res.string.transfer_qr_unavailable.value).assertIsDisplayed()
		onNodeWithText(Res.string.button_download_invitation.value).assertIsDisplayed()
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

		onNodeWithText(Res.string.receive_empty_title.value).assertIsDisplayed()
		onNodeWithText(Res.string.button_receive_files.value).performClick()
		onNodeWithText(Res.string.receive_choose_method_title.value).assertIsDisplayed()
		onNodeWithText(Res.string.receive_method_file.value).assertIsDisplayed()
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

		onNodeWithContentDescription(Res.string.receive_delete_history_item.value).assertIsDisplayed()
		onNodeWithContentDescription(Res.string.receive_delete_history_item.value).performClick()
		val itemDescription = Res.string.receive_delete_history_description.value("Holiday photos")
		val itemText = onNodeWithText(itemDescription).fetchSemanticsNode().config[SemanticsProperties.Text].single()
		assertTrue(itemText.spanStyles.any { range ->
			range.item.fontWeight == androidx.compose.ui.text.font.FontWeight.Bold &&
				itemText.substring(range.start, range.end) == "Holiday photos"
		})
		onNodeWithText(Res.string.button_cancel.value).performClick()
		onNodeWithText(Res.string.receive_clear_history.value).performClick()
		onNodeWithText(Res.string.receive_clear_history_title.value).assertIsDisplayed()
		onNodeWithText(Res.string.receive_clear_history_description.value).assertIsDisplayed()
		onAllNodesWithText(Res.string.button_close.value).assertCountEquals(0)
		onAllNodesWithContentDescription(Res.string.button_close.value).assertCountEquals(0)
	}

	@Test
	fun desktopReceiveRowsUseRightClickActionsWithoutTrailingDeleteButtons() = runComposeUiTest {
		var deletedTransferId: ULong? = null
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
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Linux) {
				VniDropTheme(isDarkTheme = false) {
					ReceiveScreen(
						coreState = CoreState(isInitialized = true, transfers = listOf(receivedTransfer())),
						state = ReceiveState(),
						windowClass = WindowClass.Desktop,
						actions = actions,
						onOpenAcquisition = {},
						onDismissAcquisition = {},
						onReceiverNameChanged = {},
						onInvitationResult = { _, _ -> },
						onWaitingForNfc = {},
						onReceive = {},
						onRequestDeleteHistoryItem = { deletedTransferId = it },
						onRequestClearHistory = {},
						onDismissHistoryDelete = {},
						onConfirmHistoryDelete = {},
					)
				}
			}
		}

		onAllNodesWithContentDescription(Res.string.receive_delete_history_item.value).assertCountEquals(0)
		onNodeWithText("Holiday photos").performMouseInput { rightClick() }
		onNodeWithText(Res.string.receive_delete_history_item.value).assertIsDisplayed().performClick()
		runOnIdle { assertEquals(10UL, deletedTransferId) }
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

private val StringResource.value: String
	get() = runBlocking { getString(this@value) }

private fun StringResource.value(vararg formatArgs: Any): String =
	runBlocking { getString(this@value, *formatArgs) }
