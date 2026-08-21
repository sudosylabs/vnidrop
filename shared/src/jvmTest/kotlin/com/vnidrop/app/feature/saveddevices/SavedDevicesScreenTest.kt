package com.vnidrop.app.feature.saveddevices

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.v2.runComposeUiTest
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.PendingTargetedOfferModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.core.TargetedTransferStateModel
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.VniDropTheme
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.coroutines.runBlocking
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.getString
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_retry
import vnidrop.shared.generated.resources.saved_devices_block_action
import vnidrop.shared.generated.resources.saved_devices_block_confirm_title
import vnidrop.shared.generated.resources.saved_devices_attention_title
import vnidrop.shared.generated.resources.saved_devices_empty
import vnidrop.shared.generated.resources.saved_devices_empty_title
import vnidrop.shared.generated.resources.saved_devices_forget_action
import vnidrop.shared.generated.resources.saved_devices_load_failed
import vnidrop.shared.generated.resources.saved_devices_send_action
import vnidrop.shared.generated.resources.saved_devices_transfer_resume
import vnidrop.shared.generated.resources.saved_devices_transfers_title

@OptIn(ExperimentalTestApi::class)
class SavedDevicesScreenTest {
	@Test
	fun emptyLoadingAndErrorStatesAreActionable() = runComposeUiTest {
		var retried = 0
		val state = mutableStateOf(SavedDevicesState(isLoading = true))
		setContent {
			VniDropTheme(isDarkTheme = false) {
				SavedDevicesScreen(state.value, WindowClass.Phone, onRetry = { retried += 1 })
			}
		}
		onAllNodesWithText(Res.string.saved_devices_empty.value).assertCountEquals(0)

		runOnIdle { state.value = SavedDevicesState(isLoading = false, loadFailed = true) }
		onNodeWithText(Res.string.saved_devices_load_failed.value).assertIsDisplayed()
		onNodeWithText(Res.string.button_retry.value).performClick()
		runOnIdle { assertEquals(1, retried) }

		runOnIdle { state.value = SavedDevicesState(isLoading = false) }
		onNodeWithText(Res.string.saved_devices_empty_title.value).assertIsDisplayed()
		onNodeWithText(Res.string.saved_devices_empty.value).assertIsDisplayed()
		onAllNodesWithText(Res.string.saved_devices_transfers_title.value).assertCountEquals(0)
	}

	@Test
	fun authenticatedNamesAndLocalLabelsArePrimaryWhileEndpointIsSecondary() = runComposeUiTest {
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Windows) {
				VniDropTheme(isDarkTheme = false) {
					SavedDevicesScreen(
						state = SavedDevicesState(
							isLoading = false,
							eligibilities = listOf(eligibility("eligible-peer", "Pixel 9")),
							pendingRelationships = listOf(incoming("eligible-peer")),
							savedDevices = listOf(device("saved-peer-long-identifier", "Office laptop", "Amira's PC")),
						),
						windowClass = WindowClass.Desktop,
					)
				}
			}
		}

		onNodeWithText("Office laptop").assertIsDisplayed()
		onAllNodesWithText("Remote name: Amira's PC").assertCountEquals(0)
		onAllNodesWithText("Pixel 9", useUnmergedTree = true).assertCountEquals(2)
		onNodeWithTag("saved-device-saved-peer-long-identifier").performClick()
		onNodeWithText("Remote name: Amira's PC").assertIsDisplayed()
		onNodeWithText("Device ID: saved-peer-long-…").assertIsDisplayed()
	}

	@Test
	fun busyDeviceDisablesSendAndDestructiveActionRequiresConfirmation() = runComposeUiTest {
		var blocked = 0
		val device = device("peer-one", null, "Riley's phone")
		val state = mutableStateOf(SavedDevicesState(isLoading = false, savedDevices = listOf(device), busyPeerIds = setOf(device.endpointId)))
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Android) {
				VniDropTheme(isDarkTheme = false) {
					SavedDevicesScreen(
						state = state.value,
						windowClass = WindowClass.Phone,
						onBlock = { blocked += 1 },
					)
				}
			}
		}
		onNodeWithTag("saved-device-peer-one").performClick()
		onNodeWithText(Res.string.saved_devices_send_action.value).assertIsNotEnabled()

		runOnIdle { state.value = state.value.copy(busyPeerIds = emptySet()) }
		onNodeWithText(Res.string.saved_devices_send_action.value).assertIsEnabled()
		onNodeWithContentDescription("More actions for Riley's phone").performClick()
		onNodeWithText(Res.string.saved_devices_block_action.value).performClick()
		onNodeWithText(Res.string.saved_devices_block_confirm_title.value).assertIsDisplayed()
		runOnIdle { assertEquals(0, blocked) }
		onNodeWithText(Res.string.saved_devices_block_action.value).performClick()
		runOnIdle { assertEquals(1, blocked) }
	}

	@Test
	fun forgetIsAvailableFromNativeOverflowMenu() = runComposeUiTest {
		val device = device("peer-two", null, "Desktop")
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Linux) {
				VniDropTheme(isDarkTheme = false) {
					SavedDevicesScreen(
						state = SavedDevicesState(isLoading = false, savedDevices = listOf(device)),
						windowClass = WindowClass.Desktop,
					)
				}
			}
		}
		onNodeWithTag("saved-device-peer-two").performClick()
		onNodeWithContentDescription("More actions for Desktop").performClick()
		onNodeWithText(Res.string.saved_devices_forget_action.value).assertIsDisplayed()
	}

	@Test
	fun deviceTransfersStayHiddenUntilTheDeviceDetailsAreOpened() = runComposeUiTest {
		val actions = mutableListOf<Pair<String, SavedDeviceTransferAction>>()
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Android) {
				VniDropTheme(isDarkTheme = false) {
					SavedDevicesScreen(
						state = SavedDevicesState(
							isLoading = false,
							savedDevices = listOf(device("peer", null, "Office PC")),
							targetedTransfers = listOf(
								SavedDeviceTransferItem(
									id = "transfer-resume",
									peerEndpointId = "peer",
									peerDisplayName = "Office PC",
									direction = SavedDeviceTransferDirection.Incoming,
									transferName = "Project files",
									fileCount = 2u,
									totalSize = 100u,
									verifiedBytes = 40u,
									state = TargetedTransferStateModel.Interrupted,
									createdAt = 1,
									updatedAt = 2,
									availableActions = listOf(
										SavedDeviceTransferAction.Resume,
										SavedDeviceTransferAction.Cancel,
									),
									progressFraction = 0.4f,
								),
							),
						),
						windowClass = WindowClass.Desktop,
						onTransferAction = { id, action -> actions += id to action },
					)
				}
			}
		}

		onAllNodesWithText(Res.string.saved_devices_transfers_title.value).assertCountEquals(0)
		onAllNodesWithText("Project files").assertCountEquals(0)
		onNodeWithTag("saved-device-peer").performClick()
		onNodeWithText(Res.string.saved_devices_transfers_title.value).assertIsDisplayed()
		onNodeWithText("Project files").assertIsDisplayed()
		onAllNodesWithText("%", substring = true).assertCountEquals(0)
		onNodeWithText(Res.string.saved_devices_transfer_resume.value).performClick()
		runOnIdle {
			assertEquals(listOf("transfer-resume" to SavedDeviceTransferAction.Resume), actions)
		}
	}

	@Test
	fun desktopHubUsesGroupedDeviceListAndShowsInlineAttentionActions() = runComposeUiTest {
		val acceptedOffers = mutableListOf<String>()
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Windows) {
				VniDropTheme(isDarkTheme = false) {
					SavedDevicesScreen(
						state = SavedDevicesState(
							isLoading = false,
							savedDevices = listOf(
								device("peer-one", null, "Amira's phone"),
								device("peer-two", "Studio PC", "Workstation"),
							),
							targetedOffers = TargetedOfferState(
								pending = listOf(offer("offer-one", "peer-one", "Holiday photos")),
								senderDisplayNames = mapOf("peer-one" to "Amira's phone"),
							),
						),
						windowClass = WindowClass.Desktop,
						onAcceptOffer = acceptedOffers::add,
					)
				}
			}
		}

		onNodeWithText(Res.string.saved_devices_attention_title.value).assertIsDisplayed()
		onNodeWithText("Amira's phone").assertIsDisplayed()
		onNodeWithText("Studio PC").assertIsDisplayed()
		onNodeWithTag("saved-device-peer-one").assertIsDisplayed()
		onNodeWithTag("saved-device-peer-two").assertIsDisplayed()
		onNodeWithText("Receive").performClick()
		runOnIdle { assertEquals(listOf("offer-one"), acceptedOffers) }
	}

	@Test
	fun compactHubPlacesAttentionBeforeSavedDevices() = runComposeUiTest {
		setContent {
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Android) {
				VniDropTheme(isDarkTheme = false) {
					SavedDevicesScreen(
						state = SavedDevicesState(
							isLoading = false,
							savedDevices = listOf(device("peer-one", null, "Phone")),
							eligibilities = listOf(eligibility("peer-two", "Laptop")),
						),
						windowClass = WindowClass.Phone,
					)
				}
			}
		}

		val attentionTop = onNodeWithText(Res.string.saved_devices_attention_title.value)
			.fetchSemanticsNode().boundsInRoot.top
		val devicesTop = onNodeWithTag("saved-device-peer-one")
			.fetchSemanticsNode().boundsInRoot.top
		assertTrue(attentionTop < devicesTop, "compact layouts should surface pending decisions before the device list")
	}

	private fun device(endpoint: String, label: String?, name: String?) = SavedDeviceModel(
		endpointId = endpoint,
		localLabel = label,
		remoteDisplayName = name,
		createdAt = 1,
		lastAuthenticatedAt = 2,
	)

	private fun eligibility(endpoint: String, name: String) = PairingEligibilityModel(
		peerEndpointId = endpoint,
		remoteDisplayName = name,
		sessionId = "session",
		protocolVersion = 1u,
		createdAt = 1,
		expiresAt = 2,
	)

	private fun incoming(endpoint: String) = DeviceRelationshipModel(
		remoteEndpointId = endpoint,
		state = DeviceRelationshipStateModel.PendingIncoming,
		generation = 1u,
		minimumProtocolVersion = 1u,
		createdAt = 1,
		updatedAt = 2,
	)

	private fun offer(id: String, sender: String, name: String) = PendingTargetedOfferModel(
		transferId = id,
		senderEndpointId = sender,
		receiverEndpointId = "local",
		manifestId = "manifest",
		contentHash = "hash",
		transferName = name,
		fileCount = 2u,
		totalSize = 100u,
		protocolVersion = 3u,
		receivedAt = 1,
	)
}

@Composable
private fun SavedDevicesScreen(
	state: SavedDevicesState,
	windowClass: WindowClass,
	onRetry: () -> Unit = {},
	onBlock: (String) -> Unit = {},
	onAcceptOffer: (String) -> Unit = {},
	onTransferAction: (String, SavedDeviceTransferAction) -> Unit = { _, _ -> },
) = SavedDevicesScreen(
	state = state,
	windowClass = windowClass,
	onRetry = onRetry,
	onRememberEligible = {},
	onDeclineEligible = {},
	onAcceptIncoming = {},
	onDeclineIncoming = {},
	onAcceptOffer = onAcceptOffer,
	onDeclineOffer = {},
	onSend = {},
	onOpenLabel = {},
	onForget = {},
	onBlock = onBlock,
	onTransferAction = onTransferAction,
	onLabelDraftChanged = {},
	onSaveLabel = {},
	onClearLabel = {},
	onDismissLabel = {},
)

private val StringResource.value: String
	get() = runBlocking { getString(this@value) }
