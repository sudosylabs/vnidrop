package com.vnidrop.app.runtime

/**
 * User opted into background Saved-device receive and the OS granted notification permission.
 * Device count is a separate listen reason; this is only the user's intent.
 */
data class SavedDeviceListenIntent(
	val optedIn: Boolean,
	val permissionGranted: Boolean,
)

internal fun savedDeviceListenActive(
	savedDeviceCount: Int,
	intent: SavedDeviceListenIntent,
): Boolean = savedDeviceCount > 0 && intent.optedIn && intent.permissionGranted
