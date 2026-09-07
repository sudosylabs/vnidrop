package com.vnidrop.app.feature.send

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import java.awt.EventQueue
import java.awt.FileDialog
import java.awt.Frame
import java.awt.KeyboardFocusManager
import java.io.File

@Composable
actual fun rememberTransferShareActions(): TransferShareActions = remember {
	object : TransferShareActions {
		override val canUseNativeShare = false

		override fun exportInvitation(ticket: String, transferName: String, onResult: (Result<Unit>) -> Unit) {
			EventQueue.invokeLater {
				onResult(runCatching {
					val dialog = FileDialog(activeFrame(), "Save VniDrop invitation", FileDialog.SAVE).apply {
						file = invitationFileName(transferName)
					}
					try {
						dialog.isVisible = true
						val directory = dialog.directory
						val name = dialog.file
						if (directory != null && name != null) File(directory, name).writeText(ticket)
					} finally { dialog.dispose() }
				})
			}
		}

		override fun shareInvitation(ticket: String, transferName: String, onResult: (Result<Unit>) -> Unit) {
			onResult(Result.failure(UnsupportedOperationException("System sharing is unavailable on this desktop")))
		}
	}
}

private fun activeFrame(): Frame? =
	(KeyboardFocusManager.getCurrentKeyboardFocusManager().activeWindow as? Frame)
		?: Frame.getFrames().firstOrNull { it.isActive || it.isFocused }
