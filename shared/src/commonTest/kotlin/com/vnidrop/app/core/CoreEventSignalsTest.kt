package com.vnidrop.app.core

import kotlin.test.Test
import kotlin.test.assertEquals

class CoreEventSignalsTest {
	@Test
	fun pairingPhaseWakesWithoutTransferId() {
		assertEquals(
			listOf(CoreSignal.PairingChanged),
			signalsForCoreEvent("pairing", transferId = null),
		)
	}

	@Test
	fun targetedTransferPhaseWakesWithoutTransferId() {
		assertEquals(
			listOf(CoreSignal.TargetedTransferChanged),
			signalsForCoreEvent("targeted_transfer", transferId = null),
		)
	}

	@Test
	fun runtimeObligationPhaseWakesWithoutTransferId() {
		assertEquals(
			listOf(CoreSignal.RuntimeObligationChanged),
			signalsForCoreEvent("runtime_obligation", transferId = null),
		)
	}

	@Test
	fun invitationApprovalStillEmitsTransferScopedSignal() {
		assertEquals(
			listOf(CoreSignal.ApprovalChanged(42uL)),
			signalsForCoreEvent("approval", transferId = 42uL),
		)
	}
}
