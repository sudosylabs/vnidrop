package com.vnidrop.app.runtime

import com.vnidrop.app.BackgroundRuntimeKeeper
import com.vnidrop.app.UiPlatform
import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreSignal
import com.vnidrop.app.core.TargetedTransferModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

internal class RuntimeRetentionCoordinator(
	private val repository: CoreGateway,
	private val keeper: BackgroundRuntimeKeeper,
	platform: UiPlatform,
	applicationScope: CoroutineScope,
) {
	private val coordinatorJob = SupervisorJob(applicationScope.coroutineContext[Job])
	private val scope = CoroutineScope(applicationScope.coroutineContext + coordinatorJob)
	private val targetedTransfers = MutableStateFlow<List<TargetedTransferModel>>(emptyList())
	private val targetedRefreshMutex = Mutex()
	private var closed = false

	init {
		if (platform == UiPlatform.Android) {
			observeObligations()
			observeInitialization()
			observeCoreSignals()
		}
	}

	private fun observeObligations() {
		scope.launch {
			combine(
				repository.state.map { it.transfers }.distinctUntilChanged(),
				targetedTransfers,
				::hasRuntimeObligation,
			).distinctUntilChanged().collect(keeper::setRequired)
		}
	}

	private fun observeInitialization() {
		scope.launch {
			repository.state.map { it.isInitialized }.distinctUntilChanged().collect { initialized ->
				if (initialized) refreshTargetedTransfers() else targetedTransfers.value = emptyList()
			}
		}
	}

	private fun observeCoreSignals() {
		scope.launch {
			repository.signals.collect { signal ->
				when (signal) {
					CoreSignal.TargetedTransferChanged -> refreshTargetedTransfers()
					is CoreSignal.TransfersChanged -> repository.refresh()
					is CoreSignal.ApprovalChanged,
						is CoreSignal.ReceiverHistoryChanged,
						CoreSignal.PairingChanged -> Unit
				}
			}
		}
	}

	private suspend fun refreshTargetedTransfers() {
		targetedRefreshMutex.withLock {
			if (!repository.state.value.isInitialized) {
				targetedTransfers.value = emptyList()
				return@withLock
			}
			repository.listTargetedTransfers().onSuccess { targetedTransfers.value = it }
		}
	}

	fun close() {
		if (closed) return
		closed = true
		scope.cancel()
		keeper.close()
	}
}
