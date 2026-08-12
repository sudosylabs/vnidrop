package com.vnidrop.app.feature.saveddevices

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.v2.runComposeUiTest
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.DeviceRelationshipModel
import com.vnidrop.app.core.DeviceRelationshipStateModel
import com.vnidrop.app.core.PairingEligibilityModel
import com.vnidrop.app.core.SavedDeviceModel
import com.vnidrop.app.ui.platform.LocalUiPlatform
import com.vnidrop.app.ui.state.WindowClass
import com.vnidrop.app.ui.theme.VniDropTheme
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.coroutines.runBlocking
import org.jetbrains.compose.resources.StringResource
import org.jetbrains.compose.resources.getString
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.button_retry
import vnidrop.shared.generated.resources.saved_devices_block_action
import vnidrop.shared.generated.resources.saved_devices_block_confirm_title
import vnidrop.shared.generated.resources.saved_devices_empty
import vnidrop.shared.generated.resources.saved_devices_forget_action
import vnidrop.shared.generated.resources.saved_devices_load_failed
import vnidrop.shared.generated.resources.saved_devices_send_action

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
		onNodeWithText(Res.string.saved_devices_empty.value).assertIsDisplayed()
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
		onNodeWithText("Remote name: Amira's PC").assertIsDisplayed()
		onAllNodesWithText("Pixel 9", useUnmergedTree = true).assertCountEquals(2)
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
		onNodeWithText(Res.string.saved_devices_send_action.value).assertIsNotEnabled()

		runOnIdle { state.value = state.value.copy(busyPeerIds = emptySet()) }
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
		onNodeWithContentDescription("More actions for Desktop").performClick()
		onNodeWithText(Res.string.saved_devices_forget_action.value).assertIsDisplayed()
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
}

@Composable
private fun SavedDevicesScreen(
	state: SavedDevicesState,
	windowClass: WindowClass,
	onRetry: () -> Unit = {},
	onBlock: (String) -> Unit = {},
) = SavedDevicesScreen(
	state = state,
	windowClass = windowClass,
	onRetry = onRetry,
	onRememberEligible = {},
	onDeclineEligible = {},
	onAcceptIncoming = {},
	onDeclineIncoming = {},
	onSend = {},
	onOpenLabel = {},
	onForget = {},
	onBlock = onBlock,
	onLabelDraftChanged = {},
	onSaveLabel = {},
	onClearLabel = {},
	onDismissLabel = {},
)

private val StringResource.value: String
	get() = runBlocking { getString(this@value) }
