package com.vnidrop.app.platform

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

class AppVisibility(initiallyForeground: Boolean = true) {
	private val _isForeground = MutableStateFlow(initiallyForeground)
	val isForeground: StateFlow<Boolean> = _isForeground.asStateFlow()
	private var lifecycleForeground = initiallyForeground
	private var windowFocused = true

	fun setForeground(value: Boolean) {
		lifecycleForeground = value
		update()
	}

	fun setWindowFocused(value: Boolean) {
		windowFocused = value
		update()
	}

	private fun update() {
		_isForeground.value = lifecycleForeground && windowFocused
	}
}
