import XCTest
@testable import VniDrop

final class BackgroundActivityPolicyTests: XCTestCase {
	func testEveryNeutralObligationRetainsTheRuntime() {
		let facts: [RuntimeObligationFactsModel] = [
			facts(activeInvitationTransfers: 1),
			facts(invitationProviderAvailability: 1),
			facts(targetedPreparations: 1),
			facts(activeTargetedTransfers: 1),
			facts(targetedProviderAvailability: 1),
		]
		XCTAssertTrue(facts.allSatisfy(BackgroundActivityPolicy.requiresAssertion))
	}

	func testNoObligationsReleasesTheRuntime() {
		XCTAssertFalse(BackgroundActivityPolicy.requiresAssertion(facts()))
	}

	private func facts(
		activeInvitationTransfers: UInt64 = 0,
		invitationProviderAvailability: UInt64 = 0,
		targetedPreparations: UInt64 = 0,
		activeTargetedTransfers: UInt64 = 0,
		targetedProviderAvailability: UInt64 = 0
	) -> RuntimeObligationFactsModel {
		RuntimeObligationFactsModel(
			activeInvitationTransfers: activeInvitationTransfers,
			invitationProviderAvailability: invitationProviderAvailability,
			targetedPreparations: targetedPreparations,
			activeTargetedTransfers: activeTargetedTransfers,
			targetedProviderAvailability: targetedProviderAvailability
		)
	}
}
