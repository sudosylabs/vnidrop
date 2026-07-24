package com.vnidrop.app.core

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import uniffi.vnidrop.ReceiveOutputSinkV2
import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption

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

	override suspend fun temporaryUsage(receiveFolder: ReceiveFolder): ULong {
		return desktopTemporaryUsage(receiveFolder)
	}

	override fun createReceiveOutputSink(folder: ReceiveFolder): ReceiveOutputSinkV2? = null

	override suspend fun sharePickedFiles(
		repository: CoreGateway,
		files: List<PickedShareFile>,
		transferName: String,
		senderName: String,
		accessPolicy: ShareAccessPolicy,
	): Result<Share> {
		require(files.isNotEmpty()) { "Select at least one file to share" }
		val sources = files.map { file ->
			uniffi.vnidrop.ShareSource(
				kind = uniffi.vnidrop.SourceKind.PATH,
				value = file.value,
				displayName = file.displayName,
				isDirectory = file.isDirectory || File(file.value).isDirectory,
			)
		}
		return repository.shareSources(sources, transferName, senderName, accessPolicy)
	}
}

internal fun desktopTemporaryUsage(receiveFolder: ReceiveFolder): ULong {
	if (receiveFolder.kind != ReceiveFolderKind.FileSystemPath) return 0UL
	val root = File(receiveFolder.value).toPath()
	if (!Files.isDirectory(root, LinkOption.NOFOLLOW_LINKS)) return 0UL
	return runCatching {
		Files.walk(root).use { paths ->
			paths.iterator().asSequence()
				.filter { path ->
					val name = path.fileName?.toString().orEmpty()
					Files.isRegularFile(path, LinkOption.NOFOLLOW_LINKS) &&
						name.startsWith(".") &&
						name.contains(".vnidrop-") &&
						name.endsWith(".part")
				}
				.fold(0UL) { total, path -> total + Files.size(path).toULong() }
		}
	}.getOrDefault(0UL)
}
