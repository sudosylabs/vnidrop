package com.vnidrop.app.core

/**
 * Maps a durable core event phase to wake-up signals.
 * Pairing / targeted phases wake even when `transferId` is absent (endpoint-scoped events).
 */
internal fun signalsForCoreEvent(phase: String, transferId: ULong?): List<CoreSignal> {
	val signals = mutableListOf<CoreSignal>()
	when (phase) {
		"pairing" -> signals += CoreSignal.PairingChanged
		"targeted_transfer" -> signals += CoreSignal.TargetedTransferChanged
	}
	if (transferId != null) {
		when (phase) {
			"approval", "access" -> signals += CoreSignal.ApprovalChanged(transferId)
			"delivery" -> signals += CoreSignal.ReceiverHistoryChanged(transferId)
		}
	}
	return signals
}
