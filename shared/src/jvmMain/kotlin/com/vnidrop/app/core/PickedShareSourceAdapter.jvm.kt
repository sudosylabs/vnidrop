package com.vnidrop.app.core

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import java.io.File

@Composable
internal actual fun rememberPickedShareSourceAdapter(): PickedShareSourceAdapter =
	remember { JvmPickedShareSourceAdapter() }

internal class JvmPickedShareSourceAdapter : PickedShareSourceAdapter {
	override suspend fun <T> withShareSources(
		files: List<PickedShareFile>,
		operation: suspend (List<uniffi.vnidrop.ShareSource>) -> T,
	): Result<T> = runCatching {
		require(files.isNotEmpty()) { "Select at least one file to share" }
		operation(pathShareSources(files))
	}

	override suspend fun discardPickedFiles(files: List<PickedShareFile>) {
		files.filter(PickedShareFile::isTemporaryCopy).forEach { file ->
			val copy = File(file.value)
			check(!copy.exists() || copy.delete()) { "Could not discard app-owned picker copy" }
		}
	}

	private fun pathShareSources(files: List<PickedShareFile>) = files.map { file ->
		uniffi.vnidrop.ShareSource(
			kind = uniffi.vnidrop.SourceKind.PATH,
			value = file.value,
			displayName = file.displayName,
			isDirectory = file.isDirectory || File(file.value).isDirectory,
		)
	}
}
