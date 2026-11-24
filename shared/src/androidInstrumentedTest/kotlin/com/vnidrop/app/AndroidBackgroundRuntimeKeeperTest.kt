package com.vnidrop.app

import android.app.ForegroundServiceStartNotAllowedException
import android.content.ComponentName
import android.content.ContextWrapper
import android.content.Intent
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.SdkSuppress
import androidx.test.platform.app.InstrumentationRegistry
import kotlin.test.Test
import kotlin.test.assertEquals
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class AndroidBackgroundRuntimeKeeperTest {
	@Test
	@SdkSuppress(minSdkVersion = Build.VERSION_CODES.S)
	fun rejectedForegroundStartLeavesKeeperAvailableForRetry() {
		var starts = 0
		var stops = 0
		val context = object : ContextWrapper(InstrumentationRegistry.getInstrumentation().targetContext) {
			override fun startForegroundService(service: Intent): ComponentName {
				starts += 1
				if (starts == 1) throw ForegroundServiceStartNotAllowedException("test start rejected")
				return requireNotNull(service.component)
			}

			override fun stopService(service: Intent): Boolean {
				stops += 1
				return true
			}
		}
		val keeper = AndroidBackgroundRuntimeKeeper(context)
		try {
			keeper.setRequired(true)
			keeper.setRequired(true)
			keeper.setRequired(true)
		} finally {
			keeper.close()
		}
		assertEquals(2, starts)
		assertEquals(1, stops)
	}

	@Test
	@SdkSuppress(minSdkVersion = Build.VERSION_CODES.VANILLA_ICE_CREAM)
	fun serviceOverridesDataSyncTimeoutCallback() {
		val service = VniDropBackgroundRuntimeService::class.java
		val intType = requireNotNull(Int::class.javaPrimitiveType)
		val callback = service.getMethod("onTimeout", intType, intType)
		assertEquals(service, callback.declaringClass)
	}
}
