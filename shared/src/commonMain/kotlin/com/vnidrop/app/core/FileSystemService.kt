package com.vnidrop.app.core

import androidx.compose.runtime.Composable
import uniffi.vnidrop.ReceiveOutputSinkV2

enum class ReceiveFolderKind {
	FileSystemPath,
	/** Shared system Downloads via MediaStore (Android 10+). */
	AndroidPublicDownloads,
	AndroidTreeUri,
}

/** Stable token stored in preferences for [ReceiveFolderKind.AndroidPublicDownloads]. */
const val AndroidPublicDownloadsToken = "media-store:downloads"

data class ReceiveFolder(
	val kind: ReceiveFolderKind,
	val value: String,
	val displayName: String,
)

data class ReceivedStorageInspection(
	val existingBytes: ULong,
	val existingCount: Int,
	val missingCount: Int,
	val inaccessibleCount: Int,
)

enum class FolderAccessStatus {
	Writable,
	PermissionRequired,
	Unavailable,
}

interface FileSystemService {
	val supportsCustomReceiveFolders: Boolean get() = true

	fun defaultReceiveFolder(): ReceiveFolder
	fun effectiveReceiveFolder(configuredFolder: ReceiveFolder): ReceiveFolder =
		if (supportsCustomReceiveFolders) configuredFolder else defaultReceiveFolder()
	suspend fun validateReceiveFolder(folder: ReceiveFolder): FolderAccessStatus
	suspend fun inspectReceivedArtifacts(artifacts: List<ReceivedArtifactModel>): ReceivedStorageInspection
	suspend fun temporaryUsage(receiveFolder: ReceiveFolder): ULong
	fun createReceiveOutputSink(folder: ReceiveFolder): ReceiveOutputSinkV2?
	fun canRevealReceiveFolder(folder: ReceiveFolder): Boolean = false
	suspend fun revealReceiveFolder(folder: ReceiveFolder): Result<Unit> =
		Result.failure(UnsupportedOperationException("Revealing the receive folder is not supported"))
	/** Releases only app-owned picker copies; implementations must never delete original user sources. */
	suspend fun discardPickedFiles(files: List<PickedShareFile>) = Unit
	suspend fun sharePickedFile(
		repository: CoreGateway,
		file: PickedShareFile,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share> = sharePickedFiles(repository, listOf(file), transferName, senderName, accessPolicy)

	suspend fun sharePickedFiles(
		repository: CoreGateway,
		files: List<PickedShareFile>,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share>
}

@Composable
expect fun rememberFileSystemService(): FileSystemService

fun ReceiveFolder.isFileSystemPath(): Boolean =
	kind == ReceiveFolderKind.FileSystemPath
