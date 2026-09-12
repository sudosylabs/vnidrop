package com.vnidrop.app.runtime

import com.vnidrop.app.BackgroundRuntimeKeeper
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.RuntimeObligationFactsModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal fun processRetentionRequired(
	coreObligation: Boolean,
	savedDeviceListen: Boolean,
): Boolean = coreObligation || savedDeviceListen

internal class ProcessRetentionCoordinator(
	private val repository: CoreGateway,
	private val keeper: BackgroundRuntimeKeeper,
	platform: UiPlatform,
	applicationScope: CoroutineScope,
	listenIntent: Flow<SavedDeviceListenIntent>,
) {
	private val coordinatorJob = SupervisorJob(applicationScope.coroutineContext[Job])
	private val scope = CoroutineScope(applicationScope.coroutineContext + coordinatorJob)
	private val facts = MutableStateFlow<RuntimeObligationFactsModel?>(null)
	private val savedDeviceCount = MutableStateFlow(0)
	private val refreshMutex = Mutex()
	private var closed = false

	init {
		if (platform == UiPlatform.Android) {
			observeRetention(listenIntent)
			observeInitialization()
			observeCoreSignals()
		}
	}

	private fun observeRetention(listenIntent: Flow<SavedDeviceListenIntent>) {
		scope.launch {
			combine(facts, savedDeviceCount, listenIntent) { currentFacts, devices, intent ->
				processRetentionRequired(
					coreObligation = currentFacts?.requiresRuntime == true,
					savedDeviceListen = savedDeviceListenActive(devices, intent),
				)
			}.distinctUntilChanged().collect(keeper::setRequired)
		}
	}

	private fun observeInitialization() {
		scope.launch {
			repository.state.map { it.isInitialized }.distinctUntilChanged().collect { initialized ->
				if (initialized) {
					refreshFacts()
					refreshSavedDevices()
				} else {
					facts.value = null
					savedDeviceCount.value = 0
				}
			}
		}
	}

	private fun observeCoreSignals() {
		scope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					CoreSignal.RuntimeObligationChanged,
					CoreSignal.TargetedTransferChanged,
					is CoreSignal.TransfersChanged -> refreshFacts()
					CoreSignal.PairingChanged -> refreshSavedDevices()
					is CoreSignal.ApprovalChanged,
					is CoreSignal.ReceiverHistoryChanged -> Unit
				}
			}
		}
	}

	private suspend fun refreshFacts() {
		refreshMutex.withLock {
			if (!repository.state.value.isInitialized) {
				facts.value = null
				return@withLock
			}
			repository.runtimeObligationFacts().onSuccess { facts.value = it }
		}
	}

	private suspend fun refreshSavedDevices() {
		refreshMutex.withLock {
			if (!repository.state.value.isInitialized) {
				savedDeviceCount.value = 0
				return@withLock
			}
			repository.listSavedDevices().onSuccess { savedDeviceCount.value = it.size }
		}
	}

	fun close() {
		if (closed) return
		closed = true
		scope.cancel()
		keeper.close()
	}
}
