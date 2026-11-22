package com.vnidrop.app.core

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import uniffi.vnidrop.ReceiveOutputSinkV2
import java.io.File

@Composable
actual fun rememberFileSystemService(): FileSystemService =
	remember { JvmFileSystemService() }

private class JvmFileSystemService : FileSystemService {
	override fun defaultReceiveFolder(): ReceiveFolder =
		ReceiveFolder(
			kind = ReceiveFolderKind.FileSystemPath,
			value = File(System.getProperty("user.home"), "Downloads").absolutePath,
			displayName = "Downloads",
		)

	override suspend fun validateReceiveFolder(folder: ReceiveFolder): FolderAccessStatus {
		if (folder.kind != ReceiveFolderKind.FileSystemPath) return FolderAccessStatus.Unavailable
		return runCatching {
			val directory = File(folder.value)
			if (!directory.exists()) directory.mkdirs()
			if (directory.isDirectory && directory.canWrite()) FolderAccessStatus.Writable else FolderAccessStatus.Unavailable
		}.getOrDefault(FolderAccessStatus.Unavailable)
	}

	override suspend fun inspectReceivedArtifacts(artifacts: List<ReceivedArtifactModel>): ReceivedStorageInspection {
		var bytes = 0UL
		var existing = 0
		var missing = 0
		for (artifact in artifacts) {
			val file = File(artifact.locator)
			if (file.isFile) {
				bytes += file.length().toULong()
				existing += 1
			} else {
				missing += 1
			}
		}
		return ReceivedStorageInspection(bytes, existing, missing, 0)
	}

	override suspend fun temporaryUsage(appDataDir: String): ULong =
		uniffi.vnidrop.receiveStagingBytes(appDataDir, reclaim = false)

	override suspend fun reclaimTemporaryStorage(appDataDir: String, receiveFolder: ReceiveFolder): ULong =
		desktopReclaimTemporaryStorage(appDataDir)

	override fun createReceiveOutputSink(folder: ReceiveFolder): ReceiveOutputSinkV2? = null


}

internal fun desktopReclaimTemporaryStorage(appDataDir: String): ULong {
	var reclaimed = uniffi.vnidrop.receiveStagingBytes(appDataDir, reclaim = true)
	val appDataRoot = File(appDataDir)
	if (appDataRoot.isDirectory) {
		appDataRoot.walkTopDown()
			.filter { it.isDirectory && it.name == ".Trash" }
			.toList()
			.forEach { trash ->
				val size = trash.walkTopDown()
					.filter(File::isFile)
					.fold(0UL) { total, file -> total + file.length().coerceAtLeast(0L).toULong() }
				if (trash.deleteRecursively()) reclaimed += size
			}
	}
	return reclaimed
}
