package com.vnidrop.app.core

import androidx.compose.runtime.Composable

internal interface PickedShareSourceAdapter {
	/** Keeps platform descriptors and leases valid until [operation] returns. */
	suspend fun <T> withShareSources(
		files: List<PickedShareFile>,
		operation: suspend (List<uniffi.vnidrop.ShareSource>) -> T,
	): Result<T>

	/** Releases only app-owned picker copies; implementations must never delete original user sources. */
	suspend fun discardPickedFiles(files: List<PickedShareFile>) = Unit
}

@Composable
internal expect fun rememberPickedShareSourceAdapter(): PickedShareSourceAdapter
