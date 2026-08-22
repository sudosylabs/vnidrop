package com.vnidrop.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.content.ContextCompat

internal class AndroidBackgroundRuntimeKeeper(
	private val context: Context,
) : BackgroundRuntimeKeeper {
	private val lock = Any()
	private var required = false
	private var closed = false

	override fun setRequired(required: Boolean) = synchronized(lock) {
		if (closed || this.required == required) return@synchronized
		val serviceIntent = Intent(context, VniDropBackgroundRuntimeService::class.java)
		if (required) {
			ContextCompat.startForegroundService(context, serviceIntent)
		} else {
			context.stopService(serviceIntent)
		}
		this.required = required
	}

	override fun close() = synchronized(lock) {
		if (closed) return@synchronized
		closed = true
		if (required) {
			context.stopService(Intent(context, VniDropBackgroundRuntimeService::class.java))
			required = false
		}
	}
}

class VniDropBackgroundRuntimeService : Service() {
	override fun onCreate() {
		super.onCreate()
		if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
			getSystemService(NotificationManager::class.java).createNotificationChannel(
				NotificationChannel(ChannelId, applicationInfo.loadLabel(packageManager), NotificationManager.IMPORTANCE_LOW),
			)
		}
	}

	override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
		val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
		val contentIntent = launchIntent?.let {
			PendingIntent.getActivity(
				this,
				NotificationId,
				it,
				PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
			)
		}
		val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
			Notification.Builder(this, ChannelId)
		} else {
			@Suppress("DEPRECATION")
			Notification.Builder(this)
		}
		val notification = builder
			.setSmallIcon(android.R.drawable.stat_sys_upload)
			.setContentTitle(applicationInfo.loadLabel(packageManager))
			.setOngoing(true)
			.setContentIntent(contentIntent)
			.build()
		if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
			startForeground(NotificationId, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
		} else {
			startForeground(NotificationId, notification)
		}
		return START_NOT_STICKY
	}

	override fun onBind(intent: Intent?): IBinder? = null

	internal companion object {
		private const val ChannelId = "vnidrop-active-sharing"
		private const val NotificationId = 0x564E44
	}
}
