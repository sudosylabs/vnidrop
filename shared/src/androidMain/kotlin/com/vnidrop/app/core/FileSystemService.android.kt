package com.vnidrop.app.core

import android.content.ContentValues
import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.system.ErrnoException
import android.system.OsConstants
import android.webkit.MimeTypeMap
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.core.net.toUri
import uniffi.vnidrop.PublishedOutput
import uniffi.vnidrop.ReceiveOutputSinkV2
import uniffi.vnidrop.ReceivedLocatorKind
import uniffi.vnidrop.VnidropException
import java.io.File
import java.io.IOException
import java.io.OutputStream
import java.net.URLConnection
import java.util.UUID

@Composable
actual fun rememberFileSystemService(): FileSystemService {
	val context = LocalContext.current.applicationContext
	return remember(context) { AndroidFileSystemService(context) }
}

private class AndroidFileSystemService(
	private val context: Context,
) : FileSystemService {
	override fun defaultReceiveFolder(): ReceiveFolder {
		// Match desktop: shared system Downloads. On Android 10+ this is MediaStore,
		// not a raw filesystem path (scoped storage). Older APIs fall back to the
		// public Downloads directory when legacy storage writes are allowed.
		return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
			ReceiveFolder(
				kind = ReceiveFolderKind.AndroidPublicDownloads,
				value = AndroidPublicDownloadsToken,
				displayName = "Downloads",
			)
		} else {
			val publicDownloads = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
			ReceiveFolder(
				kind = ReceiveFolderKind.FileSystemPath,
				value = publicDownloads.absolutePath,
				displayName = "Downloads",
			)
		}
	}

	override suspend fun validateReceiveFolder(folder: ReceiveFolder): FolderAccessStatus =
		when (folder.kind) {
			ReceiveFolderKind.FileSystemPath -> validatePath(folder.value)
			ReceiveFolderKind.AndroidPublicDownloads -> validatePublicDownloads()
			ReceiveFolderKind.AndroidTreeUri -> validateTreeUri(folder.value)
		}

	override suspend fun inspectReceivedArtifacts(artifacts: List<ReceivedArtifactModel>): ReceivedStorageInspection {
		var bytes = 0UL
		var existing = 0
		var missing = 0
		var inaccessible = 0
		for (artifact in artifacts) {
			when (artifact.locatorKind) {
				ReceivedLocatorKind.FILESYSTEM_PATH -> {
					val file = File(artifact.locator)
					if (file.isFile) {
						bytes += file.length().toULong()
						existing += 1
					} else {
						missing += 1
					}
				}
				ReceivedLocatorKind.ANDROID_MEDIA_STORE, ReceivedLocatorKind.ANDROID_DOCUMENT -> {
					val result = runCatching {
						context.contentResolver.query(
							artifact.locator.toUri(),
							arrayOf(android.provider.OpenableColumns.SIZE),
							null,
							null,
							null,
						)?.use { cursor ->
							if (!cursor.moveToFirst()) null else cursor.getLong(0).coerceAtLeast(0L).toULong()
						}
					}
					result.fold(
						onSuccess = { size ->
							if (size == null) missing += 1 else {
								bytes += size
								existing += 1
							}
						},
						onFailure = { inaccessible += 1 },
					)
				}
			}
		}
		return ReceivedStorageInspection(bytes, existing, missing, inaccessible)
	}

	override suspend fun temporaryUsage(receiveFolder: ReceiveFolder): ULong = directorySize(context.cacheDir)

	override fun createReceiveOutputSink(folder: ReceiveFolder): ReceiveOutputSinkV2? =
		when (folder.kind) {
			ReceiveFolderKind.AndroidPublicDownloads -> AndroidMediaStoreDownloadsSink(context)
			ReceiveFolderKind.AndroidTreeUri -> AndroidTreeReceiveOutputSink(context, folder.value.toUri())
			ReceiveFolderKind.FileSystemPath -> null
		}

	override suspend fun sharePickedFiles(
		repository: CoreGateway,
		files: List<PickedShareFile>,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share> = runCatching {
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
			repository.shareSources(sources, transferName, senderName, accessPolicy).getOrThrow()
		} finally {
			descriptors.forEach { it.close() }
		}
	}

	/**
	 * Probe a real create/write/delete instead of [File.canWrite].
	 *
	 * Scoped storage often reports public directories as writable even when
	 * the process cannot create files there. A probe matches what receive needs.
	 */
	private fun validatePath(path: String): FolderAccessStatus =
		runCatching {
			val directory = File(path)
			if (!directory.exists() && !directory.mkdirs()) {
				return FolderAccessStatus.Unavailable
			}
			if (!directory.isDirectory) return FolderAccessStatus.Unavailable
			val probe = File(directory, ".vnidrop-write-test-${UUID.randomUUID()}")
			try {
				probe.outputStream().use { stream -> stream.write(1) }
				if (!probe.exists()) return FolderAccessStatus.Unavailable
				FolderAccessStatus.Writable
			} finally {
				probe.delete()
			}
		}.getOrDefault(FolderAccessStatus.Unavailable)

	private fun validatePublicDownloads(): FolderAccessStatus =
		runCatching {
			val probeName = ".vnidrop-write-test-${UUID.randomUUID()}"
			val sink = AndroidMediaStoreDownloadsSink(context)
			sink.startFile(probeName)
			sink.writeChunk(probeName, byteArrayOf(1))
			sink.abortFile(probeName, "write probe complete")
			FolderAccessStatus.Writable
		}.getOrDefault(FolderAccessStatus.Unavailable)

	private fun validateTreeUri(value: String): FolderAccessStatus {
		val uri = Uri.parse(value)
		val hasPermission = context.contentResolver.persistedUriPermissions.any { permission ->
			permission.uri == uri && permission.isWritePermission
		}
		if (!hasPermission) return FolderAccessStatus.PermissionRequired
		return runCatching {
			val probe = AndroidTreeReceiveOutputSink(context, uri)
			val probeName = ".vnidrop-write-test-${UUID.randomUUID()}"
			probe.startFile(probeName)
			probe.writeChunk(probeName, byteArrayOf())
			probe.abortFile(probeName, "write probe complete")
			FolderAccessStatus.Writable
		}.getOrDefault(FolderAccessStatus.Unavailable)
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

private inline fun <T> receiveSinkCall(block: () -> T): T =
	try {
		block()
	} catch (error: VnidropException) {
		throw error
	} catch (error: Throwable) {
		throw error.toReceiveSinkException()
	}

private fun Throwable.toReceiveSinkException(): VnidropException {
	val reason = message ?: toString()
	var current: Throwable? = this
	while (current != null) {
		if (current is ErrnoException && current.errno == OsConstants.ENOSPC) {
			return VnidropException.StorageFull(reason)
		}
		current = current.cause
	}
	return when (this) {
		is SecurityException -> VnidropException.FilesystemPermission(reason)
		is IllegalArgumentException -> VnidropException.InvalidInput(reason)
		is IOException, is IllegalStateException -> VnidropException.Filesystem(reason)
		else -> VnidropException.Internal(reason)
	}
}

/**
 * Writes into the shared system Downloads collection via MediaStore.
 *
 * Files appear in the user's Downloads app / Files UI the same way a browser
 * download would. Nested relative paths become subfolders under Download/.
 */
private class AndroidMediaStoreDownloadsSink(
	private val context: Context,
) : ReceiveOutputSinkV2 {
	private data class PendingDocument(
		val stream: OutputStream,
		val uri: Uri,
	)

	private val pending = mutableMapOf<String, PendingDocument>()
	private val resolver = context.contentResolver

	override fun startFile(relativePath: String) {
		receiveSinkCall {
			check(Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
				"MediaStore Downloads requires Android 10 or newer"
			}
			check(relativePath !in pending) { "Output stream is already open for $relativePath" }
			val parts = requireSafeRelativePathParts(relativePath)
			val finalName = parts.last()
			val relativeDir = mediaStoreRelativePath(parts.dropLast(1))
			if (mediaStoreItemExists(finalName, relativeDir)) {
				throw VnidropException.DestinationExists("Destination already exists: $relativePath")
			}

			val values = ContentValues().apply {
				put(MediaStore.MediaColumns.DISPLAY_NAME, finalName)
				put(MediaStore.MediaColumns.MIME_TYPE, mimeTypeFor(finalName))
				put(MediaStore.MediaColumns.RELATIVE_PATH, relativeDir)
				put(MediaStore.MediaColumns.IS_PENDING, 1)
			}
			val uri = resolver.insert(downloadsCollection(), values)
				?: error("Could not create Downloads entry for $relativePath")
			val stream = runCatching { resolver.openOutputStream(uri, "w") }
				.getOrElse { error ->
					resolver.delete(uri, null, null)
					throw error
				} ?: run {
				resolver.delete(uri, null, null)
					error("Could not open output stream for $relativePath")
				}
			pending[relativePath] = PendingDocument(stream, uri)
		}
	}

	override fun writeChunk(relativePath: String, bytes: ByteArray) {
		receiveSinkCall {
			val document = pending[relativePath] ?: error("Output stream is not open for $relativePath")
			document.stream.write(bytes)
		}
	}

	override fun finishFile(relativePath: String): PublishedOutput = receiveSinkCall {
		val document = pending.remove(relativePath) ?: error("Output stream is not open for $relativePath")
		try {
			document.stream.flush()
			document.stream.close()
			val published = ContentValues().apply {
				put(MediaStore.MediaColumns.IS_PENDING, 0)
			}
			val updated = resolver.update(document.uri, published, null, null)
			check(updated == 1) { "Could not publish received file $relativePath" }
		} catch (error: Throwable) {
			runCatching { document.stream.close() }
			resolver.delete(document.uri, null, null)
			throw error
		}
		PublishedOutput(ReceivedLocatorKind.ANDROID_MEDIA_STORE, document.uri.toString())
	}

	override fun abortFile(relativePath: String, reason: String) {
		receiveSinkCall {
			val document = pending.remove(relativePath) ?: return@receiveSinkCall
			runCatching { document.stream.close() }
			resolver.delete(document.uri, null, null)
		}
	}

	private fun downloadsCollection(): Uri =
		MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY)

	private fun mediaStoreRelativePath(subdirs: List<String>): String {
		val base = Environment.DIRECTORY_DOWNLOADS
		return if (subdirs.isEmpty()) {
			"$base/"
		} else {
			"$base/${subdirs.joinToString("/")}/"
		}
	}

	private fun mediaStoreItemExists(displayName: String, relativePath: String): Boolean {
		val projection = arrayOf(MediaStore.MediaColumns._ID)
		val selection =
			"${MediaStore.MediaColumns.DISPLAY_NAME}=? AND ${MediaStore.MediaColumns.RELATIVE_PATH}=?"
		resolver.query(
			downloadsCollection(),
			projection,
			selection,
			arrayOf(displayName, relativePath),
			null,
		)?.use { cursor ->
			return cursor.moveToFirst()
		}
		// Some providers omit the trailing slash; check the alternate form.
		val altPath = relativePath.trimEnd('/')
		if (altPath != relativePath) {
			resolver.query(
				downloadsCollection(),
				projection,
				selection,
				arrayOf(displayName, altPath),
				null,
			)?.use { cursor ->
				return cursor.moveToFirst()
			}
		}
		return false
	}

	private fun mimeTypeFor(fileName: String): String {
		val extension = fileName.substringAfterLast('.', missingDelimiterValue = "").lowercase()
		if (extension.isNotEmpty()) {
			MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)?.let { return it }
			URLConnection.guessContentTypeFromName(fileName)?.let { return it }
		}
		return "application/octet-stream"
	}
}

private class AndroidTreeReceiveOutputSink(
	private val context: Context,
	private val treeUri: Uri,
) : ReceiveOutputSinkV2 {
	private data class PendingDocument(
		val stream: OutputStream,
		val temporaryUri: Uri,
		val parentUri: Uri,
		val finalName: String,
	)

	private val pending = mutableMapOf<String, PendingDocument>()

	override fun startFile(relativePath: String) {
		receiveSinkCall {
			check(relativePath !in pending) { "Output stream is already open for $relativePath" }
			// Defense in depth: Rust also validates, but sinks must reject traversal alone.
			requireSafeRelativePathParts(relativePath)
			val (parent, finalName) = resolveParent(relativePath)
			if (findChild(parent, finalName) != null) {
				throw VnidropException.DestinationExists("Destination already exists: $relativePath")
			}
			val temporaryName = ".$finalName.vnidrop-${UUID.randomUUID()}.part"
			val temporaryUri = DocumentsContract.createDocument(
				context.contentResolver,
				parent,
				"application/octet-stream",
				temporaryName,
			) ?: error("Could not create temporary file for $relativePath")
			val stream = context.contentResolver.openOutputStream(temporaryUri, "w")
				?: error("Could not open output stream for $relativePath")
			pending[relativePath] = PendingDocument(stream, temporaryUri, parent, finalName)
		}
	}

	override fun writeChunk(relativePath: String, bytes: ByteArray) {
		receiveSinkCall {
			val document = pending[relativePath] ?: error("Output stream is not open for $relativePath")
			document.stream.write(bytes)
		}
	}

	override fun finishFile(relativePath: String): PublishedOutput = receiveSinkCall {
		val document = pending.remove(relativePath) ?: error("Output stream is not open for $relativePath")
		try {
			document.stream.close()
			if (findChild(document.parentUri, document.finalName) != null) {
				throw VnidropException.DestinationExists("Destination already exists: $relativePath")
			}
			val finalUri = checkNotNull(
				DocumentsContract.renameDocument(
					context.contentResolver,
					document.temporaryUri,
					document.finalName,
				),
			) { "Could not commit received file $relativePath" }
			PublishedOutput(ReceivedLocatorKind.ANDROID_DOCUMENT, finalUri.toString())
		} catch (error: Throwable) {
			runCatching { document.stream.close() }
			DocumentsContract.deleteDocument(context.contentResolver, document.temporaryUri)
			throw error
		}
	}

	override fun abortFile(relativePath: String, reason: String) {
		receiveSinkCall {
			val document = pending.remove(relativePath) ?: return@receiveSinkCall
			runCatching { document.stream.close() }
			DocumentsContract.deleteDocument(context.contentResolver, document.temporaryUri)
		}
	}

	private fun resolveParent(relativePath: String): Pair<Uri, String> {
		val parts = requireSafeRelativePathParts(relativePath)
		var parent = DocumentsContract.buildDocumentUriUsingTree(
			treeUri,
			DocumentsContract.getTreeDocumentId(treeUri),
		)
		parts.dropLast(1).forEach { name ->
			parent = findChild(parent, name)
				?: DocumentsContract.createDocument(context.contentResolver, parent, DocumentsContract.Document.MIME_TYPE_DIR, name)
					?: error("Could not create directory $name")
		}
		return parent to parts.last()
	}

	private fun findChild(parent: Uri, name: String): Uri? {
		val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(
			treeUri,
			DocumentsContract.getDocumentId(parent),
		)
		context.contentResolver.query(
			childrenUri,
			arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME),
			null,
			null,
			null,
		)?.use { cursor ->
			val idIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
			val nameIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
			while (cursor.moveToNext()) {
				if (cursor.getString(nameIndex) == name) {
					return DocumentsContract.buildDocumentUriUsingTree(treeUri, cursor.getString(idIndex))
				}
			}
		}
		return null
	}
}

/**
 * Split a receive relative path and reject traversal / absolute-style components.
 * Rust already validates; this keeps Android sinks safe if called incorrectly.
 */
private fun requireSafeRelativePathParts(relativePath: String): List<String> {
	val parts = relativePath.split('/').filter { it.isNotBlank() }
	require(parts.isNotEmpty()) { "relative path must not be empty" }
	require(parts.none { part ->
		part == "." || part == ".." || part.contains('\\') || part.contains('\u0000')
	}) {
		"relative path contains invalid components: $relativePath"
	}
	return parts
}

private fun directorySize(directory: File): ULong =
	if (!directory.exists()) 0UL else directory.walkTopDown()
		.filter(File::isFile)
		.fold(0UL) { total, file -> total + file.length().toULong() }
