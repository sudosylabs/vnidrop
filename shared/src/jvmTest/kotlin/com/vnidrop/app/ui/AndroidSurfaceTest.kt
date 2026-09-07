package com.vnidrop.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.v2.runComposeUiTest
import androidx.compose.ui.unit.dp
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.feature.receive.AndroidReceiveAcquisition
import com.vnidrop.app.feature.receive.ReceiveInvitationActions
import com.vnidrop.app.feature.receive.ReceiveMethodAvailability
import com.vnidrop.app.feature.receive.ReceiveState
import com.vnidrop.app.feature.send.AndroidTransferComposer
import com.vnidrop.app.feature.send.TransferDraftDestination
import com.vnidrop.app.feature.send.TransferDraftState
import com.vnidrop.app.ui.platform.LocalUiPlatform
import kotlin.test.Test
import kotlin.test.assertEquals

@OptIn(ExperimentalTestApi::class)
class AndroidSurfaceTest {
	@Test
	fun fullScreenFlowsUseOneSurfaceWhenSystemBackgroundDiffers() = runComposeUiTest {
		val receive = mutableStateOf(false)
		val dark = mutableStateOf(true)
		var surface = Color.Unspecified
		setContent {
			// OEM palettes can give background and surface different tones.
			val scheme = if (dark.value) darkColorScheme(background = Color.Black, surface = Color(0xFF161616))
			else lightColorScheme(background = Color.White, surface = Color(0xFFF5F2FA))
			surface = scheme.surface
			CompositionLocalProvider(LocalUiPlatform provides UiPlatform.Android) {
				MaterialTheme(colorScheme = scheme) {
					Box(Modifier.size(360.dp, 700.dp)) {
						if (receive.value) AndroidReceiveAcquisition(
							CoreState(), ReceiveState(), invitations, onDismiss = {}, onNameChanged = {},
							onResult = { _, _ -> }, onReceive = {}, onCancelReceive = {}, messages = null,
						) else AndroidTransferComposer(
							coreInitialized = true,
							state = TransferDraftState(destination = TransferDraftDestination.Invitation),
							onSelectFile = {}, onSelectFolder = {}, onRemoveFile = {}, onTransferNameChanged = {},
							onSenderNameChanged = {}, onAccessPolicyChanged = {}, onSubmit = {}, onDismiss = {}, snackbarHost = {},
						)
					}
				}
			}
		}
		for (isDark in listOf(true, false)) {
			for (isReceive in listOf(false, true)) {
				runOnIdle { dark.value = isDark; receive.value = isReceive }
				val tag = if (isReceive) "receive-acquisition" else "android-transfer-composer"
				val pixels = onNodeWithTag(tag).captureToImage().toPixelMap()
				for (y in 0 until pixels.height) {
					assertEquals(surface.toArgb(), pixels[1, y].toArgb(), "Page edge differs at y=$y ($tag, dark=$isDark)")
				}
			}
		}
	}

	private val invitations = object : ReceiveInvitationActions {
		override val fileAvailability = ReceiveMethodAvailability.Available
		override val qrAvailability = ReceiveMethodAvailability.Available
		override fun pickInvitation(onResult: (Result<String>) -> Unit) = Unit
		override fun scanQrCode(onResult: (Result<String>) -> Unit) = Unit
		override fun cancel() = Unit
	}
}
