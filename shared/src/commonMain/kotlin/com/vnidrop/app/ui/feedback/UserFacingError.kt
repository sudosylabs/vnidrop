package com.vnidrop.app.ui.feedback

import uniffi.vnidrop.VnidropException
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.error_device_info
import vnidrop.shared.generated.resources.error_destination_exists
import vnidrop.shared.generated.resources.error_filesystem
import vnidrop.shared.generated.resources.error_generic
import vnidrop.shared.generated.resources.error_initialization
import vnidrop.shared.generated.resources.error_invalid_input
import vnidrop.shared.generated.resources.error_invalid_ticket
import vnidrop.shared.generated.resources.error_invitation_empty
import vnidrop.shared.generated.resources.error_missing_native_library
import vnidrop.shared.generated.resources.error_permission
import vnidrop.shared.generated.resources.error_repository
import vnidrop.shared.generated.resources.error_selection_failed
import vnidrop.shared.generated.resources.error_socket_bind
import vnidrop.shared.generated.resources.error_camera
import vnidrop.shared.generated.resources.error_network
import vnidrop.shared.generated.resources.error_share_empty
import vnidrop.shared.generated.resources.error_starting_up
import vnidrop.shared.generated.resources.error_storage_full
import vnidrop.shared.generated.resources.error_transfer

/**
 * Maps technical failures to stable, user-facing copy.
 *
 * Never expose raw exception messages (especially UniFFI `reason=…` blobs) in snackbars.
 */
fun Throwable.toUiText(): UiText =
	when (this) {
		is VnidropException.Ticket -> UiText.Resource(Res.string.error_invalid_ticket)
		is VnidropException.Permission -> UiText.Resource(Res.string.error_permission)
		is VnidropException.Filesystem -> UiText.Resource(Res.string.error_filesystem)
		is VnidropException.FilesystemPermission -> UiText.Resource(Res.string.error_filesystem)
		is VnidropException.DestinationExists -> UiText.Resource(Res.string.error_destination_exists)
		is VnidropException.StorageFull -> UiText.Resource(Res.string.error_storage_full)
		is VnidropException.Network -> UiText.Resource(Res.string.error_network)
		is VnidropException.Transfer -> transferUiText(reason)
		is VnidropException.Repository -> UiText.Resource(Res.string.error_repository)
		is VnidropException.Cancelled -> UiText.Resource(Res.string.error_generic)
		is VnidropException.InvalidInput -> UiText.Resource(Res.string.error_invalid_input)
		is VnidropException.Initialization -> initializationUiText(reason)
		is VnidropException.Internal -> reasonHints(reason) ?: UiText.Resource(Res.string.error_generic)
		else -> reasonHints(technicalDetail()) ?: UiText.Resource(Res.string.error_generic)
	}

/** User intentionally backed out of a flow — do not treat as a failure snackbar. */
fun Throwable.isUserCancellation(): Boolean {
	if (this is VnidropException.Cancelled) return true
	val haystack = technicalDetail().lowercase()
	if (haystack.isBlank()) return false
	return haystack.contains("cancelled") ||
		haystack.contains("canceled") ||
		haystack.contains("user cancelled") ||
		haystack.contains("user canceled")
}

fun Throwable.canRetryWithoutChangingInput(): Boolean =
	this !is VnidropException.FilesystemPermission &&
		this !is VnidropException.DestinationExists &&
		this !is VnidropException.InvalidInput

/** Prefer [VnidropException.reason] when present; else [Throwable.message]. */
fun Throwable.technicalDetail(): String =
	when (this) {
		is VnidropException.Initialization -> reason
		is VnidropException.Ticket -> reason
		is VnidropException.Filesystem -> reason
		is VnidropException.FilesystemPermission -> reason
		is VnidropException.DestinationExists -> reason
		is VnidropException.StorageFull -> reason
		is VnidropException.Network -> reason
		is VnidropException.Transfer -> reason
		is VnidropException.Permission -> reason
		is VnidropException.Repository -> reason
		is VnidropException.Cancelled -> reason
		is VnidropException.InvalidInput -> reason
		is VnidropException.Internal -> reason
		else -> message.orEmpty()
	}

private fun transferUiText(reason: String): UiText {
	val detail = reason.lowercase()
	return when {
		detail.contains("refused") || detail.contains("denied") || detail.contains("not approved") ->
			UiText.Resource(Res.string.error_permission)
		else -> UiText.Resource(Res.string.error_transfer)
	}
}

private fun initializationUiText(reason: String): UiText {
	val detail = reason.lowercase()
	return when {
		detail.contains("native") && detail.contains("library") ->
			UiText.Resource(Res.string.error_missing_native_library)
		detail.contains("socket") || detail.contains("bind") ->
			UiText.Resource(Res.string.error_socket_bind)
		else -> UiText.Resource(Res.string.error_initialization)
	}
}

private fun reasonHints(detailRaw: String): UiText? {
	val detail = detailRaw.lowercase()
	if (detail.isBlank()) return null

	return when {
		detail.contains("still starting") || detail.contains("starting up") ->
			UiText.Resource(Res.string.error_starting_up)
		detail.contains("empty") && (detail.contains("invitation") || detail.contains("ticket") || detail.contains("qr")) ->
			UiText.Resource(Res.string.error_invitation_empty)
		detail.contains("select at least one") || detail.contains("no files found") ->
			UiText.Resource(Res.string.error_share_empty)
		detail.contains("camera") ->
			UiText.Resource(Res.string.error_camera)
		detail.contains("native") && detail.contains("library") ->
			UiText.Resource(Res.string.error_missing_native_library)
		detail.contains("socket") || detail.contains("bind") ->
			UiText.Resource(Res.string.error_socket_bind)
		detail.contains("device information") || detail.contains("device info") ->
			UiText.Resource(Res.string.error_device_info)
		detail.contains("refused") || detail.contains("denied") || detail.contains("permission") ||
			detail.contains("not approved") || detail.contains("waiting for approval") ->
			UiText.Resource(Res.string.error_permission)
		detail.contains("invalid ticket") || detail.contains("ticket error") ||
			detail.contains("could not be read") || detail.contains("malformed") ||
			detail.contains("invitation could not be opened") ->
			UiText.Resource(Res.string.error_invalid_ticket)
		detail.contains("selected") &&
			(detail.contains("file") || detail.contains("folder") || detail.contains("document") || detail.contains("open")) ->
			UiText.Resource(Res.string.error_selection_failed)
		detail.contains("could not open the selected") || detail.contains("could not open selected") ->
			UiText.Resource(Res.string.error_selection_failed)
		detail.contains("document picker") || detail.contains("folder picker") || detail.contains("file descriptor") ||
			detail.contains("view controller") ->
			UiText.Resource(Res.string.error_selection_failed)
		else -> null
	}
}
