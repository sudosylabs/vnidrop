package com.vnidrop.app.core

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext

@Composable
internal actual fun rememberPickedShareSourceAdapter(): PickedShareSourceAdapter {
	val context = LocalContext.current.applicationContext
	return remember(context) { AndroidPickedShareSourceAdapter(context) }
}

private class AndroidPickedShareSourceAdapter(
	private val context: Context,
) : PickedShareSourceAdapter {
	override suspend fun <T> withShareSources(
		files: List<PickedShareFile>,
		operation: suspend (List<uniffi.vnidrop.ShareSource>) -> T,
	): Result<T> = runCatching {
		require(files.isNotEmpty()) { "Select at least one file to share" }
		// Android cannot pass a directory as a single FD. Expand SAF trees into
		// individual document files with relative collection paths, then open FDs.
		val expanded = files.flatMap { file ->
			if (file.isDirectory) context.expandShareDirectory(file) else listOf(file)
		}
		require(expanded.isNotEmpty()) { "No files found in the selected folder" }
		val descriptors = expanded.map { file ->
			context.contentResolver.openFileDescriptor(Uri.parse(file.value), "r")
				?: error("Could not open selected file descriptor for ${file.displayName}")
		}
		try {
			val sources = expanded.zip(descriptors) { file, descriptor ->
				uniffi.vnidrop.ShareSource(
					kind = uniffi.vnidrop.SourceKind.FILE_DESCRIPTOR,
					value = descriptor.fd.toString(),
					displayName = file.displayName,
					isDirectory = false,
				)
			}
			operation(sources)
		} finally {
			descriptors.forEach { it.close() }
		}
	}

	// Android's picker returns content owned by the user, so there is no app copy to delete.
	override suspend fun discardPickedFiles(files: List<PickedShareFile>) = Unit
}

/**
 * Expand a SAF document tree into individual file documents.
 *
 * Rust cannot accept a directory FD. Collection paths preserve the folder
 * root name so receivers see `Folder/nested/file.txt`.
 */
private fun Context.expandShareDirectory(folder: PickedShareFile): List<PickedShareFile> {
	val treeUri = Uri.parse(folder.value)
	val rootId = DocumentsContract.getTreeDocumentId(treeUri)
	val out = mutableListOf<PickedShareFile>()
	fun walk(documentId: String, relativePath: String) {
		val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, documentId)
		contentResolver.query(
			childrenUri,
			arrayOf(
				DocumentsContract.Document.COLUMN_DOCUMENT_ID,
				DocumentsContract.Document.COLUMN_DISPLAY_NAME,
				DocumentsContract.Document.COLUMN_MIME_TYPE,
				DocumentsContract.Document.COLUMN_SIZE,
			),
			null,
			null,
			null,
		)?.use { cursor ->
			val idIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
			val nameIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
			val mimeIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_MIME_TYPE)
			val sizeIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_SIZE)
			while (cursor.moveToNext()) {
				val id = cursor.getString(idIndex) ?: continue
				val name = cursor.getString(nameIndex) ?: continue
				val mime = cursor.getString(mimeIndex)
				val childRelative = if (relativePath.isEmpty()) name else "$relativePath/$name"
				if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
					walk(id, childRelative)
				} else {
					val documentUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, id)
					val size = if (sizeIndex >= 0 && !cursor.isNull(sizeIndex)) {
						cursor.getLong(sizeIndex).takeIf { it >= 0L }?.toULong()
					} else {
						null
					}
					out += PickedShareFile(
						value = documentUri.toString(),
						displayName = childRelative,
						sizeBytes = size,
						isDirectory = false,
					)
				}
			}
		}
	}
	// Prefix paths with the folder display name so nested structure is preserved.
	walk(rootId, folder.displayName)
	return out
}
