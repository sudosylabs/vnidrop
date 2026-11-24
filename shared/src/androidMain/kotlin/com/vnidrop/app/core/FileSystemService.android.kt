package com.vnidrop.app.core

import android.Manifest
import android.content.ContentValues
import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.system.ErrnoException
import android.system.OsConstants
import android.webkit.MimeTypeMap
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import androidx.core.net.toUri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.CompletableDeferred
import uniffi.vnidrop.PublishedOutput
import uniffi.vnidrop.ReceiveOutputSinkV2
import uniffi.vnidrop.ReceivedLocatorKind
import uniffi.vnidrop.TargetedOutputVerifier
import uniffi.vnidrop.VnidropException
import java.io.File
import java.io.IOException
import java.io.OutputStream
import java.net.URLConnection
import java.util.UUID

@Composable
actual fun rememberFileSystemService(): FileSystemService {
	val context = LocalContext.current.applicationContext
	val permission = viewModel { LegacyDownloadsPermissionRequest() }
	val launcher = rememberLauncherForActivityResult(
		ActivityResultContracts.RequestPermission(),
		permission::complete,
	)
	SideEffect {
		permission.launch = { launcher.launch(Manifest.permission.WRITE_EXTERNAL_STORAGE) }
	}
	return remember(context, permission) { AndroidFileSystemService(context, permission::request) }
}

private class LegacyDownloadsPermissionRequest : ViewModel() {
	var launch: (() -> Unit)? = null
	private var pending: CompletableDeferred<Boolean>? = null

	suspend fun request(): Boolean {
		pending?.let { return it.await() }
		val launchPermission = launch ?: return false
		val result = CompletableDeferred<Boolean>()
		pending = result
		try {
			launchPermission()
		} catch (_: IllegalStateException) {
			complete(false)
		}
		return result.await()
	}

	fun complete(granted: Boolean) {
		val result = pending
		pending = null
		result?.complete(granted)
	}

	override fun onCleared() {
		pending?.cancel()
		pending = null
		launch = null
	}
}

private class AndroidFileSystemService(
	private val context: Context,
	private val requestLegacyDownloadsPermission: suspend () -> Boolean,
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

	override fun needsReceiveFolderPermission(folder: ReceiveFolder): Boolean =
		Build.VERSION.SDK_INT < Build.VERSION_CODES.Q &&
			folder.kind == ReceiveFolderKind.FileSystemPath &&
			folder.value == defaultReceiveFolder().value &&
			ContextCompat.checkSelfPermission(context, Manifest.permission.WRITE_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED

	override suspend fun requestReceiveFolderPermission(folder: ReceiveFolder): Boolean =
		!needsReceiveFolderPermission(folder) || requestLegacyDownloadsPermission()

	override suspend fun validateReceiveFolder(folder: ReceiveFolder): FolderAccessStatus {
		if (needsReceiveFolderPermission(folder)) return FolderAccessStatus.PermissionRequired
		return when (folder.kind) {
			ReceiveFolderKind.FileSystemPath -> context.validatePath(folder.value)
			ReceiveFolderKind.AndroidPublicDownloads -> context.validatePublicDownloads()
			ReceiveFolderKind.AndroidTreeUri -> context.validateTreeUri(folder.value)
		}
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

	override suspend fun temporaryUsage(appDataDir: String): ULong = directorySize(context.cacheDir)

	override suspend fun reclaimTemporaryStorage(appDataDir: String, receiveFolder: ReceiveFolder): ULong {
		var reclaimed = 0UL
		context.cacheDir.listFiles().orEmpty().forEach { entry ->
			val size = if (entry.isDirectory) directorySize(entry) else entry.length().coerceAtLeast(0L).toULong()
			if (entry.deleteRecursively()) reclaimed += size
		}
		val appDataRoot = File(appDataDir)
		val appDataIsOwned = runCatching {
			val appDataPath = appDataRoot.canonicalPath
			val filesPath = context.filesDir.canonicalPath
			appDataPath == filesPath || appDataPath.startsWith(filesPath + File.separator)
		}.getOrDefault(false)
		if (appDataIsOwned) {
			appDataRoot.walkTopDown()
				.filter { it.isDirectory && it.name == ".Trash" }
				.toList()
				.forEach { trash ->
					val size = directorySize(trash)
					if (trash.deleteRecursively()) reclaimed += size
				}
		}
		return reclaimed
	}

	override fun createReceiveOutputSink(folder: ReceiveFolder): ReceiveOutputSinkV2? =
		when (folder.kind) {
			ReceiveFolderKind.AndroidPublicDownloads -> AndroidMediaStoreDownloadsSink(context)
			ReceiveFolderKind.AndroidTreeUri -> AndroidTreeReceiveOutputSink(context, folder.value.toUri())
			ReceiveFolderKind.FileSystemPath -> null
		}
}

/**
	 * Probe a real create/write/delete instead of [File.canWrite].
	 *
	 * Scoped storage often reports public directories as writable even when
	 * the process cannot create files there. A probe matches what receive needs.
	 */
private fun Context.validatePath(path: String): FolderAccessStatus =
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

private fun Context.validatePublicDownloads(): FolderAccessStatus =
		runCatching {
			val probeName = ".vnidrop-write-test-${UUID.randomUUID()}"
			val sink = AndroidMediaStoreDownloadsSink(this)
			sink.startFile(probeName)
			sink.writeChunk(probeName, byteArrayOf(1))
			sink.abortFile(probeName, "write probe complete")
			FolderAccessStatus.Writable
		}.getOrDefault(FolderAccessStatus.Unavailable)

private fun Context.validateTreeUri(value: String): FolderAccessStatus {
		val uri = Uri.parse(value)
		val hasPermission = contentResolver.persistedUriPermissions.any { permission ->
			permission.uri == uri && permission.isWritePermission
		}
		if (!hasPermission) return FolderAccessStatus.PermissionRequired
		return runCatching {
			val probe = AndroidTreeReceiveOutputSink(this, uri)
			val probeName = ".vnidrop-write-test-${UUID.randomUUID()}"
			probe.startFile(probeName)
			probe.writeChunk(probeName, byteArrayOf())
			probe.abortFile(probeName, "write probe complete")
			FolderAccessStatus.Writable
		}.getOrDefault(FolderAccessStatus.Unavailable)
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
) : ReceiveOutputSinkV2, TargetedOutputVerifier {
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

	override fun destinationKey(): String = downloadsCollection().toString()

	override fun pendingOutputLocator(relativePath: String): String = receiveSinkCall {
		checkNotNull(pending[relativePath]).uri.toString()
	}

	override fun openPublishedOutput(relativePath: String, locator: String, published: Boolean): Int? = receiveSinkCall {
		val collection = downloadsCollection()
		val parts = requireSafeRelativePathParts(relativePath)
		val expectedDirectory = mediaStoreRelativePath(parts.dropLast(1)).trimEnd('/')
		val uri = Uri.parse(locator)
		if (uri.authority != collection.authority || uri.pathSegments.dropLast(1) != collection.pathSegments) {
			return@receiveSinkCall null
		}
		val projection = arrayOf(
			MediaStore.MediaColumns.DISPLAY_NAME,
			MediaStore.MediaColumns.RELATIVE_PATH,
			MediaStore.MediaColumns.IS_PENDING,
		)
		val matches = resolver.query(uri, projection, null, null, null)?.use { cursor ->
			cursor.moveToFirst() && cursor.getString(0) == parts.last() &&
				cursor.getString(1)?.trimEnd('/') == expectedDirectory && cursor.getInt(2) == 0
		} == true
		if (matches) resolver.openFileDescriptor(uri, "r")?.use { it.detachFd() } else null
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
) : ReceiveOutputSinkV2, TargetedOutputVerifier {
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
			val (parent, finalName) = checkNotNull(resolveParent(relativePath))
			if (findChild(parent, finalName) != null) {
				throw VnidropException.DestinationExists("Destination already exists: $relativePath")
			}
			val temporaryName = ".vnidrop-${UUID.randomUUID()}.part"
			val temporaryUri = DocumentsContract.createDocument(
				context.contentResolver,
				parent,
				"application/octet-stream",
				temporaryName,
			) ?: error("Could not create temporary file for $relativePath")
			val stream = try {
				context.contentResolver.openOutputStream(temporaryUri, "w")
					?: error("Could not open output stream for $relativePath")
			} catch (error: Exception) {
				runCatching { DocumentsContract.deleteDocument(context.contentResolver, temporaryUri) }
					.exceptionOrNull()?.let { cleanupError -> error.addSuppressed(cleanupError) }
				throw error
			}
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


	override fun destinationKey(): String = treeUri.toString()

	override fun pendingOutputLocator(relativePath: String): String = receiveSinkCall {
		checkNotNull(pending[relativePath]).temporaryUri.toString()
	}

	override fun openPublishedOutput(relativePath: String, locator: String, published: Boolean): Int? = receiveSinkCall {
		val (parent, name) = resolveParent(relativePath, createMissing = false) ?: return@receiveSinkCall null
		val uri = findChild(parent, name) ?: return@receiveSinkCall null
		val recorded = Uri.parse(locator)
		if (uri.authority != recorded.authority || !DocumentsContract.isDocumentUri(context, recorded)) {
			return@receiveSinkCall null
		}
		val recordedId = DocumentsContract.getDocumentId(recorded)
		if (DocumentsContract.getDocumentId(uri) != recordedId) {
			// Some providers issue a new ID on rename. An owned stage still present
			// under its old ID has not been published.
			if (published || findChildMatching(parent) { id, _ -> id == recordedId } != null) {
				return@receiveSinkCall null
			}
		}
		context.contentResolver.openFileDescriptor(uri, "r")?.use { it.detachFd() }
	}

	private fun resolveParent(relativePath: String, createMissing: Boolean = true): Pair<Uri, String>? {
		val parts = requireSafeRelativePathParts(relativePath)
		var parent = DocumentsContract.buildDocumentUriUsingTree(
			treeUri,
			DocumentsContract.getTreeDocumentId(treeUri),
		)
		for (name in parts.dropLast(1)) {
			val existing = findChild(parent, name)
			if (existing != null) {
				parent = existing
			} else {
				if (!createMissing) return null
				parent = DocumentsContract.createDocument(
					context.contentResolver, parent, DocumentsContract.Document.MIME_TYPE_DIR, name,
				) ?: error("Could not create directory $name")
			}
		}
		return parent to parts.last()
	}

	private fun findChild(parent: Uri, name: String): Uri? =
		findChildMatching(parent) { _, displayName -> displayName == name }

	private fun findChildMatching(parent: Uri, matches: (String, String) -> Boolean): Uri? {
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
				val id = cursor.getString(idIndex) ?: continue
				val displayName = cursor.getString(nameIndex) ?: continue
				if (matches(id, displayName)) {
					return DocumentsContract.buildDocumentUriUsingTree(treeUri, id)
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
