package com.vnidrop.app

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.lifecycleScope
import com.vnidrop.app.core.initializeAndroidCoreRuntime
import com.vnidrop.app.feature.receive.ExternalInvitationController
import com.vnidrop.app.feature.receive.MaxVniDropInvitationBytes
import com.vnidrop.app.feature.receive.decodeInvitationBytes
import com.vnidrop.app.feature.receive.isVniDropInvitationDocument
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainActivity : ComponentActivity() {
	private val externalInvitations = ExternalInvitationController()

	override fun onCreate(savedInstanceState: Bundle?) {
		enableEdgeToEdge()
		super.onCreate(savedInstanceState)
		initializeAndroidCoreRuntime(applicationContext)
		setContent {
			App(rememberAndroidAppDependencies(this, externalInvitations))
		}
		handleInvitationIntent(intent)
	}

	override fun onNewIntent(intent: Intent) {
		super.onNewIntent(intent)
		setIntent(intent)
		handleInvitationIntent(intent)
	}

	private fun handleInvitationIntent(source: Intent?) {
		if (source?.action != Intent.ACTION_VIEW) return
		val uri = source.data
		setIntent(Intent(this, MainActivity::class.java).setAction(Intent.ACTION_MAIN))
		if (uri == null) {
			externalInvitations.reportOpenFailure("The invitation could not be opened")
			return
		}
		lifecycleScope.launch {
			val result = withContext(Dispatchers.IO) { readInvitation(uri, source.type) }
			result.fold(externalInvitations::openInvitation) { error ->
				externalInvitations.reportOpenFailure(error.message ?: "The invitation could not be opened")
			}
		}
	}

	private fun readInvitation(uri: Uri, declaredType: String?): Result<String> = runCatching {
		val resolvedType = declaredType ?: contentResolver.getType(uri)
		val displayName = runCatching {
			contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
				if (cursor.moveToFirst()) cursor.getString(0) else null
			}
		}.getOrNull()
		require(isVniDropInvitationDocument(resolvedType, uri.path, displayName)) { "This is not a VniDrop invitation" }
		val bytes = contentResolver.openInputStream(uri)?.use { it.readNBytes(MaxVniDropInvitationBytes + 1) }
			?: error("The invitation could not be opened")
		decodeInvitationBytes(bytes)
	}
}
