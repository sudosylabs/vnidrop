package com.vnidrop.app.background

import com.vnidrop.app.core.CoreGateway
import com.vnidrop.app.core.CoreState
import com.vnidrop.app.core.TransferDirection
import com.vnidrop.app.core.TransferStatus
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/** Keeps the platform process eligible to serve an outgoing share while its UI is backgrounded. */
fun interface BackgroundSharingController {
	fun setSharingActive(active: Boolean)
}

class BackgroundSharingCoordinator(
	repository: CoreGateway,
	private val controller: BackgroundSharingController,
	scope: CoroutineScope,
) {
	init {
		scope.launch {
			repository.state
				.map(::requiresBackgroundSharing)
				.distinctUntilChanged()
				.collect(controller::setSharingActive)
		}
	}

	fun stop() = controller.setSharingActive(false)
}

internal fun requiresBackgroundSharing(state: CoreState): Boolean =
	state.isInitialized && state.transfers.any { transfer ->
		transfer.direction == TransferDirection.Send &&
			transfer.status in setOf(TransferStatus.Importing, TransferStatus.Sharing)
	}
