import XCTest
@testable import VniDrop

@MainActor
final class ContactsModelTests: XCTestCase {
	private func makeModel(
		_ gateway: FakeCoreGateway
	) -> (ContactsModel, AppPreferencesRepository) {
		let defaults = UserDefaults(suiteName: "contacts-tests-\(UUID().uuidString)")!
		let preferences = AppPreferencesRepository(
			defaults: defaults,
			fallback: AppPreferencesDefaults(
				username: "tester",
				receiveFolder: ReceiveFolder(
					kind: .fileSystemPath,
					value: "/tmp",
					displayName: "Downloads"
				),
				themeMode: .system
			)
		)
		let model = ContactsModel(
			repository: gateway,
			messages: UiMessageController(),
			preferences: preferences
		)
		return (model, preferences)
	}

	private func contact(
		_ endpointId: String,
		label: String? = nil,
		remoteName: String? = nil,
		canSend: Bool = true
	) -> DeviceContact {
		DeviceContact(
			endpointId: endpointId,
			localLabel: label,
			remoteDisplayName: remoteName,
			lastTransferAt: nil,
			createdAt: 0,
			canSend: canSend
		)
	}

	private func offer(_ offerId: String, from endpointId: String = "peer") -> IncomingOfferModel {
		IncomingOfferModel(
			offerId: offerId,
			fromEndpointId: endpointId,
			senderDisplayName: "Peer",
			transferName: "photos",
			fileCount: 2,
			totalBytes: 1_024,
			receivedAt: 0
		)
	}

	func testRefreshLoadsContactsBlocksAndPrompts() async {
		let gateway = FakeCoreGateway()
		gateway.contactsResult = .success([contact("a"), contact("b")])
		gateway.blockedResult = .success(["blocked-one"])
		gateway.pairings = [PendingPairingModel(endpointId: "c", displayName: "Laptop", receivedAt: 0)]
		gateway.offers = [offer("offer-1")]
		let (model, _) = makeModel(gateway)

		await model.refresh()

		XCTAssertEqual(model.state.contacts.count, 2)
		XCTAssertEqual(model.state.blocked, ["blocked-one"])
		XCTAssertEqual(model.state.currentPairing?.endpointId, "c")
		XCTAssertEqual(model.state.currentOffer?.offerId, "offer-1")
		XCTAssertFalse(model.state.isLoading)
	}

	/// Accepting an offer is the only path that yields a ticket; the caller needs
	/// it to run the receive with its own destination.
	func testAcceptingAnOfferReturnsTheTicket() async {
		let gateway = FakeCoreGateway()
		gateway.offers = [offer("offer-1")]
		gateway.offerTicket = "vnd1:abc"
		let (model, _) = makeModel(gateway)
		await model.refresh()

		let ticket = await model.respondToOffer(offerId: "offer-1", accepted: true)

		XCTAssertEqual(ticket, "vnd1:abc")
		XCTAssertTrue(model.state.pendingOffers.isEmpty)
		XCTAssertEqual(gateway.offerResponses.map(\.accepted), [true])
	}

	func testDecliningAnOfferYieldsNoTicketAndClearsThePrompt() async {
		let gateway = FakeCoreGateway()
		gateway.offers = [offer("offer-1")]
		let (model, _) = makeModel(gateway)
		await model.refresh()

		let ticket = await model.respondToOffer(offerId: "offer-1", accepted: false)

		XCTAssertNil(ticket, "a declined offer must not hand over a capability")
		XCTAssertTrue(model.state.pendingOffers.isEmpty)
	}

	/// Declining to be remembered must leave nothing behind for the peer.
	func testDecliningPairingClearsThePromptWithoutAddingAContact() async {
		let gateway = FakeCoreGateway()
		gateway.pairings = [PendingPairingModel(endpointId: "peer", displayName: nil, receivedAt: 0)]
		let (model, _) = makeModel(gateway)
		await model.refresh()

		await model.respondToPairing(endpointId: "peer", accepted: false)

		XCTAssertTrue(model.state.pendingPairings.isEmpty)
		XCTAssertTrue(model.state.contacts.isEmpty)
		XCTAssertEqual(gateway.pairingResponses.map(\.accepted), [false])
	}

	func testAcceptingPairingAddsTheContact() async {
		let gateway = FakeCoreGateway()
		gateway.pairings = [PendingPairingModel(endpointId: "peer", displayName: "Laptop", receivedAt: 0)]
		let (model, _) = makeModel(gateway)
		await model.refresh()
		gateway.contactsResult = .success([contact("peer", remoteName: "Laptop")])

		await model.respondToPairing(endpointId: "peer", accepted: true)

		XCTAssertTrue(model.state.pendingPairings.isEmpty)
		XCTAssertEqual(model.state.contacts.map(\.endpointId), ["peer"])
	}

	func testForgettingClearsTheSelectionAndReloads() async {
		let gateway = FakeCoreGateway()
		gateway.contactsResult = .success([contact("peer")])
		let (model, _) = makeModel(gateway)
		await model.refresh()
		model.select("peer")

		gateway.contactsResult = .success([])
		await model.forget(endpointId: "peer")

		XCTAssertEqual(gateway.forgottenContacts, ["peer"])
		XCTAssertNil(model.state.selectedEndpointId)
		XCTAssertTrue(model.state.contacts.isEmpty)
	}

	func testBlockingRemovesTheContactAndKeepsItListedAsBlocked() async {
		let gateway = FakeCoreGateway()
		gateway.contactsResult = .success([contact("peer")])
		let (model, _) = makeModel(gateway)
		await model.refresh()
		model.select("peer")

		gateway.contactsResult = .success([])
		gateway.blockedResult = .success(["peer"])
		await model.block(endpointId: "peer")

		XCTAssertEqual(gateway.blockedContactIds, ["peer"])
		XCTAssertNil(model.state.selectedEndpointId)
		XCTAssertEqual(model.state.blocked, ["peer"])
	}

	/// An empty label clears the override rather than storing whitespace, so the
	/// row falls back to the name the device reports.
	func testBlankLabelClearsTheLocalName() async {
		let gateway = FakeCoreGateway()
		let (model, _) = makeModel(gateway)

		await model.setLabel(endpointId: "peer", label: "   ")

		XCTAssertEqual(gateway.contactLabels.count, 1)
		XCTAssertNil(gateway.contactLabels[0].label)
	}

	func testLabelIsTrimmedBeforeStoring() async {
		let gateway = FakeCoreGateway()
		let (model, _) = makeModel(gateway)

		await model.setLabel(endpointId: "peer", label: "  Work Mac  ")

		XCTAssertEqual(gateway.contactLabels[0].label, "Work Mac")
	}

	/// The core holds the lifetime in memory only, so the stored preference is
	/// the durable copy and both have to move together.
	func testGrantLifetimeIsPersistedAndPushedToTheCore() async {
		let gateway = FakeCoreGateway()
		let (model, preferences) = makeModel(gateway)

		model.setGrantLifetime(.days365)
		await Task.yield()

		XCTAssertEqual(model.state.grantLifetime, .days365)
		XCTAssertEqual(preferences.preferences.grantLifetime, .days365)
		XCTAssertEqual(gateway.grantLifetimes.last, .days365)
	}

	func testDefaultGrantLifetimeIsNinetyDays() {
		let gateway = FakeCoreGateway()
		let (model, _) = makeModel(gateway)

		XCTAssertEqual(model.state.grantLifetime, .days90)
	}

	/// The local label wins over whatever the peer calls itself.
	func testDisplayNamePrefersTheLocalLabel() {
		let subject = contact("peer", label: "Work Mac", remoteName: "Totally Not Evil")

		XCTAssertEqual(subject.displayName, "Work Mac")
	}

	func testDisplayNameFallsBackToTheReportedName() {
		let subject = contact("peer", remoteName: "Laptop")

		XCTAssertEqual(subject.displayName, "Laptop")
	}

	func testUnreachableContactIsSurfacedForRepairing() async {
		let gateway = FakeCoreGateway()
		gateway.contactsResult = .success([contact("peer", canSend: false)])
		let (model, _) = makeModel(gateway)

		await model.refresh()

		XCTAssertEqual(model.state.contacts.first?.canSend, false)
	}
}
