package com.vnidrop.app.feature.receive

import android.content.pm.PackageManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable
actual fun rememberReceiveInvitationActions(): ReceiveInvitationActions {
	val context = LocalContext.current
	val activity = context as? ComponentActivity
	var qrSession by remember { mutableStateOf<QrScanSession?>(null) }
	qrSession?.let { session ->
		if (activity != null) key(session) {
			QrScanner(activity, onResult = session::complete, onDismiss = { session.close(); qrSession = null })
		}
	}
	val scope = rememberCoroutineScope()
	var fileRead by remember { mutableStateOf<Job?>(null) }
	var fileResult by remember { mutableStateOf<((Result<String>) -> Unit)?>(null) }
	val filePicker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
		val callback = fileResult.also { fileResult = null } ?: return@rememberLauncherForActivityResult
		if (uri == null) return@rememberLauncherForActivityResult
		fileRead = scope.launch {
			val result = withContext(Dispatchers.IO) {
				runCatching {
					val bytes = context.contentResolver.openInputStream(uri)?.use { it.readInvitationBytes() }
						?: error("The invitation could not be opened")
					decodeInvitationBytes(bytes)
				}
			}
			callback(result)
		}
	}
	return remember(activity, filePicker) {
		object : ReceiveInvitationActions {
			override val fileAvailability = ReceiveMethodAvailability.Available
			override val qrAvailability = if (activity != null && context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)) ReceiveMethodAvailability.Available else ReceiveMethodAvailability.Unavailable

			override fun pickInvitation(onResult: (Result<String>) -> Unit) {
				cancel()
				fileResult = onResult
				filePicker.launch(arrayOf(InvitationMimeType, "application/octet-stream", "text/plain", "*/*"))
			}

			override fun scanQrCode(onResult: (Result<String>) -> Unit) {
				if (activity == null) return onResult(Result.failure(UnsupportedOperationException("QR scanning is unavailable")))
				cancel()
				qrSession = QrScanSession { result -> qrSession = null; onResult(result) }
			}

			override fun cancel() {
				fileRead?.cancel()
				fileRead = null
				fileResult = null
				qrSession?.close()
				qrSession = null
			}
		}
	}
}
