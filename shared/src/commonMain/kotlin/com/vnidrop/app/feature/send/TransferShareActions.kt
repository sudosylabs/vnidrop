package com.vnidrop.app.feature.send

import androidx.compose.runtime.Composable

interface TransferShareActions {
	val canUseNativeShare: Boolean
	fun exportInvitation(ticket: String, transferName: String, onResult: (Result<Unit>) -> Unit)
	fun shareInvitation(ticket: String, transferName: String, onResult: (Result<Unit>) -> Unit)
}

object UnavailableTransferShareActions : TransferShareActions {
	override val canUseNativeShare = false
	override fun exportInvitation(ticket: String, transferName: String, onResult: (Result<Unit>) -> Unit) =
		onResult(Result.failure(UnsupportedOperationException("Invitation export is unavailable")))
	override fun shareInvitation(ticket: String, transferName: String, onResult: (Result<Unit>) -> Unit) =
		onResult(Result.failure(UnsupportedOperationException("System sharing is unavailable")))
}

@Composable
expect fun rememberTransferShareActions(): TransferShareActions

internal fun invitationFileName(transferName: String): String {
	val safe = transferName.trim()
		.map { character -> if (character.isLetterOrDigit() || character in "-_. ") character else '_' }
		.joinToString("")
		.trim('.', ' ')
		.ifBlank { "VniDrop transfer" }
	return if (safe.endsWith(".vnd", ignoreCase = true)) safe else "$safe.vnd"
}
