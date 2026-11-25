package com.vnidrop.app.notifications

import android.annotation.SuppressLint
import android.Manifest
import android.app.NotificationChannel
import android.app.Notification
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume
import org.jetbrains.compose.resources.stringResource
import vnidrop.shared.generated.resources.Res
import vnidrop.shared.generated.resources.android_notification_channel_transfers

@Composable
fun rememberAndroidLocalNotificationService(activity: ComponentActivity): LocalNotificationService {
	val channelName = stringResource(Res.string.android_notification_channel_transfers)
	val holder = viewModel { AndroidNotificationServiceHolder(activity.applicationContext, channelName) }
	LaunchedEffect(holder, channelName) { holder.service.updateChannelName(channelName) }
	val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
		holder.service.completePermissionRequest(granted)
	}
	SideEffect {
		holder.service.attachPermissionLauncher {
			launcher.launch(Manifest.permission.POST_NOTIFICATIONS)
		}
	}
	return holder.service
}

private class AndroidNotificationServiceHolder(context: Context, channelName: String) : ViewModel() {
	val service = AndroidLocalNotificationService(context).also { it.updateChannelName(channelName) }
}

private class AndroidLocalNotificationService(
	private val context: Context,
) : LocalNotificationService {
	private val _permission = MutableStateFlow(currentPermission())
	override val permission: StateFlow<NotificationPermission> = _permission.asStateFlow()
	private var permissionContinuation: CancellableContinuation<NotificationPermission>? = null
	private var launchPermissionRequest: (() -> Unit)? = null

	fun updateChannelName(name: String) {
		if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
			val manager = context.getSystemService(NotificationManager::class.java)
			// Shown under system notification settings for this app.
			manager.createNotificationChannel(
				NotificationChannel(ChannelId, name, NotificationManager.IMPORTANCE_HIGH),
			)
		}
	}

	override suspend fun refreshPermission(): NotificationPermission = currentPermission().also { _permission.value = it }

	override suspend fun requestPermission(): NotificationPermission {
		val current = refreshPermission()
		if (current != NotificationPermission.NotDetermined) return current
		return suspendCancellableCoroutine { continuation ->
			permissionContinuation?.cancel()
			permissionContinuation = continuation
			continuation.invokeOnCancellation { permissionContinuation = null }
			val launcher = launchPermissionRequest
			if (launcher == null) {
				permissionContinuation = null
				continuation.resume(NotificationPermission.Denied)
			} else {
				markPermissionRequested()
				launcher()
			}
		}
	}

	override suspend fun openSettings(): Result<Unit> = runCatching {
		val intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
			Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
		} else {
			Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${context.packageName}"))
		}
		context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
	}

	@SuppressLint("MissingPermission")
	override suspend fun publish(notification: LocalNotification): Result<Unit> = runCatching {
		check(refreshPermission() == NotificationPermission.Granted) { "Notification permission is not granted" }
		val launchIntent = context.packageManager.getLaunchIntentForPackage(context.packageName)
		val pendingIntent = launchIntent?.let {
			PendingIntent.getActivity(
				context,
				notification.id.hashCode(),
				it,
				PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
			)
		}
		val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
			Notification.Builder(context, ChannelId)
		} else {
			@Suppress("DEPRECATION")
			Notification.Builder(context)
		}
		val built = builder
			.setSmallIcon(android.R.drawable.stat_sys_download_done)
			.setContentTitle(notification.title)
			.setContentText(notification.body)
			.setStyle(Notification.BigTextStyle().bigText(notification.body))
			.setAutoCancel(true)
			.setPriority(Notification.PRIORITY_HIGH)
			.setContentIntent(pendingIntent)
			.build()
		context.getSystemService(NotificationManager::class.java).notify(notification.id.hashCode(), built)
	}

	override suspend fun cancel(id: String) {
		context.getSystemService(NotificationManager::class.java).cancel(id.hashCode())
	}

	override suspend fun cancelAll() {
		context.getSystemService(NotificationManager::class.java).cancelAll()
	}

	fun completePermissionRequest(granted: Boolean) {
		markPermissionRequested()
		val result = if (granted) NotificationPermission.Granted else NotificationPermission.Denied
		_permission.value = result
		permissionContinuation?.takeIf { it.isActive }?.resume(result)
		permissionContinuation = null
	}

	fun attachPermissionLauncher(launcher: () -> Unit) {
		launchPermissionRequest = launcher
	}

	private fun currentPermission(): NotificationPermission {
		val manager = context.getSystemService(NotificationManager::class.java)
		if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
			return if (manager.areNotificationsEnabled()) NotificationPermission.Granted else NotificationPermission.Denied
		}
		val granted = context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
		return when {
			granted && manager.areNotificationsEnabled() -> NotificationPermission.Granted
			wasPermissionRequested() -> NotificationPermission.Denied
			else -> NotificationPermission.NotDetermined
		}
	}

	private fun markPermissionRequested() {
		context.getSharedPreferences(PreferencesName, Context.MODE_PRIVATE).edit().putBoolean(PermissionRequestedKey, true).apply()
	}

	private fun wasPermissionRequested(): Boolean =
		context.getSharedPreferences(PreferencesName, Context.MODE_PRIVATE).getBoolean(PermissionRequestedKey, false)

	private companion object {
		const val ChannelId = "vnidrop-connection-requests"
		const val PreferencesName = "vnidrop-notifications"
		const val PermissionRequestedKey = "permission-requested"
	}
}
