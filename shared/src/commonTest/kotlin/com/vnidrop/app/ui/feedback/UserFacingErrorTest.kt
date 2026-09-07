package com.vnidrop.app.ui.feedback

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import uniffi.vnidrop.VnidropException
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.error_filesystem
import vnidrop.shared.generated.resources.error_destination_exists
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

class UserFacingErrorTest {
	@Test
	fun mapsVnidropExceptionVariantsToStableCopy() {
		assertEquals(
			UiText.Resource(Res.string.error_invalid_ticket),
			VnidropException.Ticket("decode failed: bad base32").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_permission),
			VnidropException.Permission("receiver refused").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_filesystem),
			VnidropException.Filesystem("permission denied opening path").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_filesystem),
			VnidropException.FilesystemPermission("folder is read-only").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_destination_exists),
			VnidropException.DestinationExists("destination already exists").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_storage_full),
			VnidropException.StorageFull("no space left").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_network),
			VnidropException.Network("connection reset").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_invalid_input),
			VnidropException.InvalidInput("invalid collection name").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_transfer),
			VnidropException.Transfer("connection reset").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_repository),
			VnidropException.Repository("database locked").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_initialization),
			VnidropException.Initialization("endpoint bootstrap failed").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_generic),
			VnidropException.Internal("unexpected").toUiText(),
		)
	}

	@Test
	fun neverSurfacesRawReasonPrefixFromUniffi() {
		val text = VnidropException.Transfer("blob fetch failed").toUiText()
		assertTrue(text is UiText.Resource)
		assertFalse(text.toString().contains("reason="))
	}

	@Test
	fun mapsCommonPlatformReasons() {
		assertEquals(
			UiText.Resource(Res.string.error_permission),
			IllegalStateException("sender refused").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_invalid_ticket),
			IllegalArgumentException("invalid ticket").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_invitation_empty),
			IllegalArgumentException("The invitation is empty").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_selection_failed),
			IllegalStateException("Could not open the selected files").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_socket_bind),
			IllegalStateException("socket bind failed on port").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_missing_native_library),
			IllegalStateException("native library missing from this build").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_starting_up),
			IllegalStateException("VniDrop is still starting up").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_camera),
			IllegalStateException("Camera access is required to scan QR codes").toUiText(),
		)
		assertEquals(
			UiText.Resource(Res.string.error_share_empty),
			IllegalStateException("Select at least one file to share").toUiText(),
		)
	}

	@Test
	fun unknownErrorsUseGenericCopy() {
		assertEquals(
			UiText.Resource(Res.string.error_generic),
			RuntimeException("opaque backend code 0xdead").toUiText(),
		)
	}

	@Test
	fun detectsUserCancellations() {
		assertTrue(IllegalStateException("QR scanning was cancelled").isUserCancellation())
		assertTrue(IllegalStateException("File selection was cancelled").isUserCancellation())
		assertTrue(VnidropException.Transfer("transfer cancelled by user").isUserCancellation())
		assertTrue(VnidropException.Cancelled("cancel requested").isUserCancellation())
		assertFalse(IllegalStateException("sender refused").isUserCancellation())
	}

	@Test
	fun retryRequiresChangingCollisionOrInvalidInput() {
		assertFalse(VnidropException.FilesystemPermission("read-only").canRetryWithoutChangingInput())
		assertFalse(VnidropException.DestinationExists("target exists").canRetryWithoutChangingInput())
		assertFalse(VnidropException.InvalidInput("bad path").canRetryWithoutChangingInput())
		assertTrue(VnidropException.Network("offline").canRetryWithoutChangingInput())
	}
}
