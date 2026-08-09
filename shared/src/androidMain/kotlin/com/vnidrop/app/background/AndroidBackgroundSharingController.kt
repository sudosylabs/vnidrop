package com.vnidrop.app.background

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat

class AndroidBackgroundSharingController(
	private val context: Context,
) : BackgroundSharingController {
	override fun setSharingActive(active: Boolean) {
		val intent = Intent(context, BackgroundSharingService::class.java)
		if (active) {
			ContextCompat.startForegroundService(context, intent)
		} else {
			context.stopService(intent)
		}
	}
}
