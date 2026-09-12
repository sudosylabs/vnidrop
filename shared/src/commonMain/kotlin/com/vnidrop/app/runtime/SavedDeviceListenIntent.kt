package com.vnidrop.app.runtime

/** Opt-in for background Saved-device receive; device count is a separate listen reason. */
data class SavedDeviceListenIntent(
	val optedIn: Boolean,
	val permissionGranted: Boolean,
)

internal fun savedDeviceListenActive(
	savedDeviceCount: Int,
	intent: SavedDeviceListenIntent,
): Boolean = savedDeviceCount > 0 && intent.optedIn && intent.permissionGranted
