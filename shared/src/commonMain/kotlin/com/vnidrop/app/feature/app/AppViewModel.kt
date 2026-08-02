package com.vnidrop.app.feature.app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.vnidrop.app.PlatformEnvironment
import com.vnidrop.app.AppDependencies
import com.vnidrop.app.AppGraph
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.logging.AppLogger
import com.vnidrop.app.preferences.PreferencesRepository
import com.vnidrop.app.ui.feedback.UiMessageController
import com.vnidrop.app.ui.navigation.AppDestination
import com.vnidrop.app.ui.theme.ThemeMode
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class AppState(
	val destination: AppDestination = AppDestination.Send,
	val themeMode: ThemeMode = ThemeMode.System,
)

class AppGraphViewModel(dependencies: AppDependencies) : ViewModel() {
	val graph = AppGraph(dependencies)

	override fun onCleared() {
		graph.close()
		super.onCleared()
	}
}

class AppViewModel(
	private val environment: PlatformEnvironment,
	private val repository: CoreGateway,
	preferencesRepository: PreferencesRepository,
	private val messages: UiMessageController,
) : ViewModel() {
	private val _state = MutableStateFlow(AppState())
	val state: StateFlow<AppState> = _state.asStateFlow()

	init {
		AppLogger.info("lifecycle", "app started", mapOf("platform" to environment.name))
		viewModelScope.launch {
			val relaySettings = preferencesRepository.preferences.first().relaySettings
			repository.initialize(environment.defaultCoreDataDir, relaySettings).onFailure(messages::error)
		}
		viewModelScope.launch {
			preferencesRepository.preferences.collect { preferences ->
				_state.update { it.copy(themeMode = preferences.themeMode) }
			}
		}
	}

	fun selectDestination(destination: AppDestination) {
		_state.update { it.copy(destination = destination) }
	}
}
