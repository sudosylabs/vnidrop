package com.vnidrop.app.core

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class RuntimeObligationFactsModelTest {
	@Test
	fun everyNeutralObligationRetainsTheRuntime() {
		val facts = listOf(
			facts(activeInvitationTransfers = 1UL),
			facts(invitationProviderAvailability = 1UL),
			facts(targetedPreparations = 1UL),
			facts(activeTargetedTransfers = 1UL),
			facts(targetedProviderAvailability = 1UL),
		)

		assertTrue(facts.all(RuntimeObligationFactsModel::requiresRuntime))
		assertFalse(facts().requiresRuntime)
	}

	private fun facts(
		activeInvitationTransfers: ULong = 0UL,
		invitationProviderAvailability: ULong = 0UL,
		targetedPreparations: ULong = 0UL,
		activeTargetedTransfers: ULong = 0UL,
		targetedProviderAvailability: ULong = 0UL,
	) = RuntimeObligationFactsModel(
		activeInvitationTransfers,
		invitationProviderAvailability,
		targetedPreparations,
		activeTargetedTransfers,
		targetedProviderAvailability,
	)
}
