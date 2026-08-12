package com.vnidrop.app.core

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Point
import android.net.Uri
import android.os.Build
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext

@Composable
actual fun rememberShareFilePicker(
	onFilesPicked: (List<PickedShareFile>) -> Unit,
	onError: (String) -> Unit,
): ShareFilePicker {
	val context = LocalContext.current
	val filesLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
		if (uris.isEmpty()) {
			onFilesPicked(emptyList())
			return@rememberLauncherForActivityResult
		}
		runCatching {
			uris.map { uri -> context.pickedShareFile(uri) }
		}.fold(
			onSuccess = onFilesPicked,
			onFailure = { onError(it.message ?: "Could not open the selected files") },
		)
	}
	val folderLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
		if (uri == null) {
			onFilesPicked(emptyList())
			return@rememberLauncherForActivityResult
		}
		runCatching {
			// Read permission only — we expand the tree into file FDs at share time.
			context.contentResolver.takePersistableUriPermission(
				uri,
				Intent.FLAG_GRANT_READ_URI_PERMISSION,
			)
			val displayName = uri.lastPathSegment
				?.substringAfterLast(':')
				?.substringAfterLast('/')
				?.ifBlank { null }
				?: "Folder"
			listOf(
				PickedShareFile(
					value = uri.toString(),
					displayName = displayName,
					sizeBytes = null,
					thumbnailBytes = null,
					isDirectory = true,
				),
			)
		}.fold(
			onSuccess = onFilesPicked,
			onFailure = { onError(it.message ?: "Could not open the selected folder") },
		)
	}
	return remember(filesLauncher, folderLauncher) {
		object : ShareFilePicker {
			override fun pickFiles() {
				filesLauncher.launch(arrayOf("*/*"))
			}

			override fun pickFolder() {
				folderLauncher.launch(null)
			}
		}
	}
}

@Composable
actual fun rememberReceiveFolderPicker(
	onFolderPicked: (ReceiveFolder) -> Unit,
	onError: (String) -> Unit,
): ReceiveFolderPicker {
	val context = LocalContext.current
	val launcher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
		if (uri != null) {
			runCatching {
				context.contentResolver.takePersistableUriPermission(
					uri,
					Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
				)
			}
			onFolderPicked(
				ReceiveFolder(
					kind = ReceiveFolderKind.AndroidTreeUri,
					value = uri.toString(),
					displayName = uri.lastPathSegment ?: "Downloads",
				),
			)
		}
	}
	return remember(launcher) {
		object : ReceiveFolderPicker {
			override fun pickFolder() {
				launcher.launch(null)
			}
		}
	}
}

private fun Context.pickedShareFile(uri: Uri): PickedShareFile {
	var displayName: String? = null
	var sizeBytes: ULong? = null
	contentResolver.query(uri, null, null, null, null)?.use { cursor ->
		val nameIndex = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
		val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
		if (cursor.moveToFirst()) {
			if (nameIndex >= 0 && !cursor.isNull(nameIndex)) displayName = cursor.getString(nameIndex)
			if (sizeIndex >= 0 && !cursor.isNull(sizeIndex)) {
				sizeBytes = cursor.getLong(sizeIndex).takeIf { it >= 0L }?.toULong()
			}
		}
	}
	return PickedShareFile(
		value = uri.toString(),
		displayName = displayName ?: uri.lastPathSegment ?: "transfer",
		sizeBytes = sizeBytes,
		thumbnailBytes = runCatching {
			val bitmap = (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
				contentResolver.loadThumbnail(uri, android.util.Size(192, 192), null)
			} else {
				DocumentsContract.getDocumentThumbnail(contentResolver, uri, Point(192, 192), null)
			}) ?: error("The document provider did not return a thumbnail")
			java.io.ByteArrayOutputStream().use { output ->
				bitmap.compress(Bitmap.CompressFormat.PNG, 100, output)
				output.toByteArray()
			}
		}.getOrNull(),
	)
}
