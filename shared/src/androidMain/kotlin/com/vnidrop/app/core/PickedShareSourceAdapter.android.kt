package com.vnidrop.app.core

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import java.io.Closeable
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

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
	): Result<T> = withContext(Dispatchers.IO) {
		try {
			require(files.isNotEmpty()) { "Select at least one file to share" }
			// Expand SAF trees into file descriptors with relative collection paths.
			val expanded = files.flatMap { file ->
				if (file.isDirectory) context.expandShareDirectory(file) else listOf(file)
			}
			require(expanded.isNotEmpty()) { "No files found in the selected folder" }
			val result = withOpenedShareDescriptors(expanded, open = { file ->
				context.contentResolver.openFileDescriptor(Uri.parse(file.value), "r")
					?: error("Could not open selected file descriptor for ${file.displayName}")
			}) { descriptors ->
				val sources = expanded.zip(descriptors) { file, descriptor ->
					uniffi.vnidrop.ShareSource(
						kind = uniffi.vnidrop.SourceKind.FILE_DESCRIPTOR,
						value = descriptor.fd.toString(),
						displayName = file.displayName,
						isDirectory = false,
					)
				}
				operation(sources)
			}
			Result.success(result)
		} catch (error: CancellationException) {
			throw error
		} catch (error: Exception) {
			Result.failure(error)
		}
	}

	// Android's picker returns content owned by the user, so there is no app copy to delete.
	override suspend fun discardPickedFiles(files: List<PickedShareFile>) = Unit
}

internal suspend fun <Source, Descriptor : Closeable, Result> withOpenedShareDescriptors(
	sources: List<Source>,
	open: (Source) -> Descriptor,
	operation: suspend (List<Descriptor>) -> Result,
): Result {
	val descriptors = mutableListOf<Descriptor>()
	var failure: Throwable? = null
	try {
		for (source in sources) descriptors.add(open(source))
		return operation(descriptors)
	} catch (error: Throwable) {
		failure = error
		throw error
	} finally {
		val operationFailure = failure
		for (descriptor in descriptors.asReversed()) {
			try {
				descriptor.close()
			} catch (error: Exception) {
				val previous = failure
				if (previous == null) failure = error
				else if (previous !== error) previous.addSuppressed(error)
			}
		}
		if (operationFailure == null) failure?.let { throw it }
	}
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
