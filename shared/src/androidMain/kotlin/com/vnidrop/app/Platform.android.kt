package com.vnidrop.app

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.BatteryManager
import android.os.Build
import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import com.vnidrop.app.background.AndroidBackgroundSharingController
import com.vnidrop.app.core.rememberFileSystemService
import com.vnidrop.app.notifications.rememberAndroidLocalNotificationService
import com.vnidrop.app.feature.receive.ExternalInvitationController
import java.net.NetworkInterface

@Composable
fun rememberAndroidAppDependencies(activity: ComponentActivity, externalInvitations: ExternalInvitationController): AppDependencies {
	val context = activity.applicationContext
	val fileSystemService = rememberFileSystemService()
	val notificationService = rememberAndroidLocalNotificationService(activity)
	val backgroundSharingController = remember(context) { AndroidBackgroundSharingController(context) }
	return remember(context, fileSystemService, notificationService, backgroundSharingController) {
		AppDependencies(
			environment = PlatformEnvironment(
				name = "Android ${Build.VERSION.SDK_INT}",
				appVersion = context.appVersion(),
				defaultCoreDataDir = context.filesDir.resolve("vnidrop").absolutePath,
				defaultUsername = Build.DEVICE.takeIf(String::isNotBlank) ?: "Receiver",
				uiPlatform = UiPlatform.Android,
			),
			deviceInfoProvider = AndroidDeviceInfoProvider(context),
			fileSystemService = fileSystemService,
			localNotificationService = notificationService,
			backgroundSharingController = backgroundSharingController,
			externalInvitations = externalInvitations,
			backgroundRuntimeKeeper = AndroidBackgroundRuntimeKeeper(context),
		)
	}
}

private class AndroidDeviceInfoProvider(
	private val context: Context,
) : DeviceInfoProvider {
	override suspend fun load(): DeviceInfo = DeviceInfo(
		deviceName = Build.DEVICE,
		deviceModel = listOf(Build.MANUFACTURER, Build.MODEL)
			.filter(String::isNotBlank)
			.joinToString(" ")
			.ifBlank { null },
		operatingSystem = "Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})",
		network = context.activeNetworkSummary(),
		batteryLevel = context.batteryLevel(),
	)
}

private fun Context.appVersion(): String = runCatching {
	packageManager.getPackageInfo(packageName, 0).versionName
}.getOrNull()?.takeIf(String::isNotBlank) ?: "unknown"

private fun Context.activeNetworkSummary(): String? = runCatching {
	val manager = getSystemService(Context.CONNECTIVITY_SERVICE) as? ConnectivityManager
		?: return@runCatching networkInterfaceName()
	val activeNetwork = manager.activeNetwork ?: return@runCatching networkInterfaceName()
	val capabilities = manager.getNetworkCapabilities(activeNetwork) ?: return@runCatching networkInterfaceName()
	when {
		capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "Wi-Fi"
		capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "Mobile"
		capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "Ethernet"
		capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "VPN"
		else -> networkInterfaceName()
	}
}.getOrNull()

private fun Context.batteryLevel(): String? = runCatching {
	val manager = getSystemService(BatteryManager::class.java)
	val level = manager.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY)
	level.takeIf { it >= 0 }?.let { "$it%" }
}.getOrNull()

private fun networkInterfaceName(): String? =
	NetworkInterface.getNetworkInterfaces()
		.asSequence()
		.filter { it.isUp && !it.isLoopback }
		.map { it.displayName }
		.firstOrNull()
