package com.vnidrop.app.core

import android.content.Context

internal object AndroidCoreRuntime {
	init {
		System.loadLibrary("vnidrop")
	}

	external fun initialize(context: Context): Boolean
}

fun initializeAndroidCoreRuntime(context: Context) {
	check(AndroidCoreRuntime.initialize(context.applicationContext)) {
		"The protected VniDrop runtime could not initialize"
	}
}
