package com.vnidrop.app.background

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import kotlinx.coroutines.runBlocking
import org.jetbrains.compose.resources.getString
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.notifications_background_sharing_body
import vnidrop.shared.generated.resources.notifications_background_sharing_channel
import vnidrop.shared.generated.resources.notifications_background_sharing_title

class BackgroundSharingService : Service() {
	override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
		ensureNotificationChannel()
		startForeground(NotificationId, createNotification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
		return START_NOT_STICKY
	}

	override fun onBind(intent: Intent?): IBinder? = null

	override fun onDestroy() {
		stopForeground(STOP_FOREGROUND_REMOVE)
		super.onDestroy()
	}

	private fun ensureNotificationChannel() {
		val manager = getSystemService(NotificationManager::class.java)
		manager.createNotificationChannel(
			NotificationChannel(
				ChannelId,
				localizedString(Res.string.notifications_background_sharing_channel),
				NotificationManager.IMPORTANCE_LOW,
			),
		)
	}

	private fun createNotification(): Notification {
		val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
		val contentIntent = launchIntent?.let {
			PendingIntent.getActivity(
				this,
				0,
				it,
				PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
			)
		}
		return Notification.Builder(this, ChannelId)
			.setSmallIcon(android.R.drawable.stat_sys_upload)
			.setContentTitle(localizedString(Res.string.notifications_background_sharing_title))
			.setContentText(localizedString(Res.string.notifications_background_sharing_body))
			.setOngoing(true)
			.setContentIntent(contentIntent)
			.build()
	}

	private fun localizedString(resource: org.jetbrains.compose.resources.StringResource): String =
		runBlocking { getString(resource) }

	private companion object {
		const val ChannelId = "vnidrop-background-sharing"
		const val NotificationId = 3_427
	}
}
