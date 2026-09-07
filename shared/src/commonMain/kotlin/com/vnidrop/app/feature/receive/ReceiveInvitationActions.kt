package com.vnidrop.app.feature.receive

import androidx.compose.runtime.Composable

enum class ReceiveMethod { InvitationFile, QrCode }

enum class ReceiveMethodAvailability { Available, Unavailable, Hidden }

interface ReceiveInvitationActions {
	val fileAvailability: ReceiveMethodAvailability
	val qrAvailability: ReceiveMethodAvailability

	fun pickInvitation(onResult: (Result<String>) -> Unit)
	fun scanQrCode(onResult: (Result<String>) -> Unit)
	fun cancel()
}

@Composable
expect fun rememberReceiveInvitationActions(): ReceiveInvitationActions

internal const val InvitationMimeType = VniDropInvitationMimeType
internal const val MaxInvitationBytes = MaxVniDropInvitationBytes
