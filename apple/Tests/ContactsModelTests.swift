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
			preferences: preferences,
			fileSystemService: FakeFileSystemService()
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

	/// Files picked for a device go out as an offer, never as an invitation
	/// anyone holding the ticket could use.
	func testSendingToAContactUsesTheContactDestination() async {
		let gateway = FakeCoreGateway()
		let files = FakeFileSystemService()
		let defaults = UserDefaults(suiteName: "contacts-send-\(UUID().uuidString)")!
		let preferences = AppPreferencesRepository(
			defaults: defaults,
			fallback: AppPreferencesDefaults(
				username: "tester",
				receiveFolder: ReceiveFolder(kind: .fileSystemPath, value: "/tmp", displayName: "Downloads"),
				themeMode: .system
			)
		)
		let model = ContactsModel(
			repository: gateway,
			messages: UiMessageController(),
			preferences: preferences,
			fileSystemService: files
		)
		gateway.sendToContactResult = .success(
			ContactSendOutcome(
				share: Share(
					transferId: 1, ticket: "vnd1:x", transferName: "doc",
					contentHash: "h", fileCount: 1, totalSize: 2
				),
				delivered: true
			)
		)

		model.chooseFilesToSend(to: "peer")
		XCTAssertTrue(model.pendingFilePick)
		await model.onFilesPicked([
			PickedShareFile(value: "/tmp/doc.txt", displayName: "doc.txt", isDirectory: false)
		])

		XCTAssertEqual(files.shareDestinations, [.contact(endpointId: "peer")])
		XCTAssertEqual(gateway.sentToContacts, ["peer"])
	}

	/// A pick that arrives with no target must not be sent anywhere.
	func testPickedFilesWithoutATargetAreIgnored() async {
		let gateway = FakeCoreGateway()
		let files = FakeFileSystemService()
		let defaults = UserDefaults(suiteName: "contacts-send-\(UUID().uuidString)")!
		let preferences = AppPreferencesRepository(
			defaults: defaults,
			fallback: AppPreferencesDefaults(
				username: "tester",
				receiveFolder: ReceiveFolder(kind: .fileSystemPath, value: "/tmp", displayName: "Downloads"),
				themeMode: .system
			)
		)
		let model = ContactsModel(
			repository: gateway,
			messages: UiMessageController(),
			preferences: preferences,
			fileSystemService: files
		)

		await model.onFilesPicked([
			PickedShareFile(value: "/tmp/doc.txt", displayName: "doc.txt", isDirectory: false)
		])

		XCTAssertTrue(files.shareDestinations.isEmpty)
		XCTAssertTrue(gateway.sentToContacts.isEmpty)
	}

	/// Polling is opt-in: it tells every contact the app was opened.
	func testForegroundCheckIsSkippedUnlessEnabled() async {
		let gateway = FakeCoreGateway()
		let (model, _) = makeModel(gateway)

		await model.checkForOffersOnForeground()

		XCTAssertEqual(gateway.pollCount, 0)
	}

	func testForegroundCheckRunsOnceEnabled() async {
		let gateway = FakeCoreGateway()
		let (model, preferences) = makeModel(gateway)

		model.setCheckForOffersOnOpen(true)
		await model.checkForOffersOnForeground()

		XCTAssertEqual(gateway.pollCount, 1)
		XCTAssertTrue(preferences.preferences.checkForOffersOnOpen)
	}

	/// The explicit "check now" ignores the setting: the user just asked.
	func testExplicitCheckRunsEvenWhenTheSettingIsOff() async {
		let gateway = FakeCoreGateway()
		gateway.pollResult = .success(2)
		let (model, _) = makeModel(gateway)

		let collected = await model.collectWaitingOffers()

		XCTAssertEqual(collected, 2)
		XCTAssertEqual(gateway.pollCount, 1)
	}

	/// A transfer that could not be delivered is reported as waiting, not as a
	/// success nobody has received.
	func testAnUndeliveredSendIsReportedAsWaiting() async {
		let gateway = FakeCoreGateway()
		let files = FakeFileSystemService()
		let defaults = UserDefaults(suiteName: "contacts-held-\(UUID().uuidString)")!
		let preferences = AppPreferencesRepository(
			defaults: defaults,
			fallback: AppPreferencesDefaults(
				username: "tester",
				receiveFolder: ReceiveFolder(kind: .fileSystemPath, value: "/tmp", displayName: "Downloads"),
				themeMode: .system
			)
		)
		let messages = UiMessageController()
		let model = ContactsModel(
			repository: gateway,
			messages: messages,
			preferences: preferences,
			fileSystemService: files
		)
		gateway.sendToContactResult = .success(
			ContactSendOutcome(
				share: Share(
					transferId: 1, ticket: "vnd1:x", transferName: "doc",
					contentHash: "h", fileCount: 1, totalSize: 2
				),
				delivered: false
			)
		)

		model.chooseFilesToSend(to: "peer")
		await model.onFilesPicked([
			PickedShareFile(value: "/tmp/doc.txt", displayName: "doc.txt", isDirectory: false)
		])

		XCTAssertEqual(messages.current?.tone, .info)
	}

	func testHeldOffersAreLoadedForDisplay() async {
		let gateway = FakeCoreGateway()
		gateway.heldOffersResult = .success([
			HeldOfferModel(
				offerId: "held-1",
				endpointId: "peer",
				transferId: 1,
				transferName: "doc",
				fileCount: 1,
				totalBytes: 2,
				createdAt: 0
			)
		])
		let (model, _) = makeModel(gateway)

		await model.refresh()

		XCTAssertEqual(model.state.heldOffers.map(\.offerId), ["held-1"])
	}

	/// Offering an existing transfer reuses it rather than creating another.
	func testOfferingAnExistingTransferReportsAcceptance() async {
		let gateway = FakeCoreGateway()
		gateway.offerTransferResult = .success(
			ContactSendOutcome(
				share: Share(
					transferId: 7, ticket: "vnd1:x", transferName: "doc",
					contentHash: "h", fileCount: 1, totalSize: 2
				),
				delivered: true
			)
		)
		let (model, _) = makeModel(gateway)

		let delivered = await model.offerTransfer(transferId: 7, to: contact("peer"))

		XCTAssertTrue(delivered)
		XCTAssertEqual(gateway.offeredTransfers.map(\.transferId), [7])
		XCTAssertEqual(gateway.offeredTransfers.map(\.endpointId), ["peer"])
	}

	/// An offer to a closed device is reported as waiting, not accepted.
	func testOfferingToAClosedDeviceReportsItAsWaiting() async {
		let gateway = FakeCoreGateway()
		gateway.offerTransferResult = .success(
			ContactSendOutcome(
				share: Share(
					transferId: 7, ticket: "vnd1:x", transferName: "doc",
					contentHash: "h", fileCount: 1, totalSize: 2
				),
				delivered: false
			)
		)
		let (model, _) = makeModel(gateway)

		let delivered = await model.offerTransfer(transferId: 7, to: contact("peer"))

		XCTAssertFalse(delivered)
	}

	/// A refusal by the person on the other device is information, not an error.
	func testADeclinedOfferIsReportedWithoutAnErrorTone() async {
		let gateway = FakeCoreGateway()
		gateway.offerTransferResult = .failure(
			InvitationError.raw("permission error: device did not accept the transfer: receiver-declined")
		)
		let defaults = UserDefaults(suiteName: "contacts-declined-\(UUID().uuidString)")!
		let preferences = AppPreferencesRepository(
			defaults: defaults,
			fallback: AppPreferencesDefaults(
				username: "tester",
				receiveFolder: ReceiveFolder(kind: .fileSystemPath, value: "/tmp", displayName: "Downloads"),
				themeMode: .system
			)
		)
		let messages = UiMessageController()
		let model = ContactsModel(
			repository: gateway,
			messages: messages,
			preferences: preferences,
			fileSystemService: FakeFileSystemService()
		)

		let delivered = await model.offerTransfer(transferId: 7, to: contact("peer"))

		XCTAssertFalse(delivered)
		XCTAssertEqual(messages.current?.tone, .info)
	}

	func testUnreachableContactIsSurfacedForRepairing() async {
		let gateway = FakeCoreGateway()
		gateway.contactsResult = .success([contact("peer", canSend: false)])
		let (model, _) = makeModel(gateway)

		await model.refresh()

		XCTAssertEqual(model.state.contacts.first?.canSend, false)
	}
}

// MARK: - Post-transfer suggestions

@MainActor
final class PairingSuggestionTests: XCTestCase {
	private func makeModel(
		_ gateway: FakeCoreGateway,
		defaults: UserDefaults
	) -> (ContactsModel, AppPreferencesRepository) {
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
			preferences: preferences,
			fileSystemService: FakeFileSystemService()
		)
		return (model, preferences)
	}

	private func newDefaults() -> UserDefaults {
		UserDefaults(suiteName: "suggestion-tests-\(UUID().uuidString)")!
	}

	private func completedReceive(from peerId: String?) -> Transfer {
		Transfer(
			localId: "local-1",
			transferId: 1,
			direction: .receive,
			status: .done,
			peerId: peerId,
			transferName: "photos",
			contentHash: nil,
			fileCount: 1,
			totalSize: 10,
			ticket: nil,
			accessPolicy: .requireApproval,
			createdAt: 0,
			updatedAt: 0
		)
	}

	private func state(with transfers: [Transfer]) -> CoreState {
		var core = CoreState()
		core.isInitialized = true
		core.transfers = transfers
		return core
	}

	func testCompletedReceiveSuggestsItsSender() async {
		let gateway = FakeCoreGateway()
		let (model, _) = makeModel(gateway, defaults: newDefaults())
		await model.refresh()

		gateway.setState(state(with: [completedReceive(from: "sender-endpoint")]))
		await Task.yield()

		XCTAssertEqual(model.state.currentSuggestion?.endpointId, "sender-endpoint")
	}

	/// A transfer that never recorded a peer cannot be turned into a suggestion.
	func testReceiveWithoutAPeerIsNotSuggested() async {
		let gateway = FakeCoreGateway()
		let (model, _) = makeModel(gateway, defaults: newDefaults())
		await model.refresh()

		gateway.setState(state(with: [completedReceive(from: nil)]))
		await Task.yield()

		XCTAssertNil(model.state.currentSuggestion)
	}

	func testAlreadyRememberedDeviceIsNotSuggested() async {
		let gateway = FakeCoreGateway()
		gateway.contactsResult = .success([
			DeviceContact(
				endpointId: "sender-endpoint",
				localLabel: nil,
				remoteDisplayName: nil,
				lastTransferAt: nil,
				createdAt: 0,
				canSend: true
			)
		])
		let (model, _) = makeModel(gateway, defaults: newDefaults())
		await model.refresh()

		gateway.setState(state(with: [completedReceive(from: "sender-endpoint")]))
		await Task.yield()

		XCTAssertNil(model.state.currentSuggestion)
	}

	func testBlockedDeviceIsNotSuggested() async {
		let gateway = FakeCoreGateway()
		gateway.blockedResult = .success(["sender-endpoint"])
		let (model, _) = makeModel(gateway, defaults: newDefaults())
		await model.refresh()

		gateway.setState(state(with: [completedReceive(from: "sender-endpoint")]))
		await Task.yield()

		XCTAssertNil(model.state.currentSuggestion)
	}

	/// Declining has to stick, or every later transfer with the same device
	/// re-asks the question the user already answered.
	func testDecliningIsRememberedAcrossLaterTransfers() async {
		let defaults = newDefaults()
		let gateway = FakeCoreGateway()
		let (model, preferences) = makeModel(gateway, defaults: defaults)
		await model.refresh()
		gateway.setState(state(with: [completedReceive(from: "sender-endpoint")]))
		await Task.yield()
		let suggestion = try? XCTUnwrap(model.state.currentSuggestion)

		model.declineSuggestion(suggestion!)

		XCTAssertNil(model.state.currentSuggestion)
		XCTAssertTrue(preferences.preferences.declinedPairingSuggestions.contains("sender-endpoint"))

		// A second transfer with the same device must stay silent.
		gateway.setState(CoreState())
		gateway.setState(state(with: [completedReceive(from: "sender-endpoint")]))
		await Task.yield()
		XCTAssertNil(model.state.currentSuggestion)
	}

	func testAcceptingASuggestionIssuesAGrantUnderTheLocalUsername() async {
		let gateway = FakeCoreGateway()
		let (model, _) = makeModel(gateway, defaults: newDefaults())
		await model.refresh()
		gateway.setState(state(with: [completedReceive(from: "sender-endpoint")]))
		await Task.yield()
		let suggestion = try? XCTUnwrap(model.state.currentSuggestion)

		await model.acceptSuggestion(suggestion!)

		XCTAssertEqual(gateway.allowedDevices.map(\.endpointId), ["sender-endpoint"])
		XCTAssertEqual(gateway.allowedDevices.first?.displayName, "tester")
		XCTAssertNil(model.state.currentSuggestion)
	}

	/// Pairing deliberately after declining should work, so the decline is
	/// cleared rather than blocking the device forever.
	func testAcceptingClearsAnEarlierDecline() async {
		let defaults = newDefaults()
		let gateway = FakeCoreGateway()
		let (model, preferences) = makeModel(gateway, defaults: defaults)
		let suggestion = PairingSuggestion(
			endpointId: "sender-endpoint",
			displayName: nil,
			transferName: nil
		)
		model.declineSuggestion(suggestion)
		XCTAssertTrue(preferences.preferences.declinedPairingSuggestions.contains("sender-endpoint"))

		await model.acceptSuggestion(suggestion)

		XCTAssertFalse(preferences.preferences.declinedPairingSuggestions.contains("sender-endpoint"))
	}

	func testTheSameDeviceIsOnlySuggestedOnce() async {
		let gateway = FakeCoreGateway()
		let (model, _) = makeModel(gateway, defaults: newDefaults())
		await model.refresh()

		gateway.setState(state(with: [completedReceive(from: "sender-endpoint")]))
		await Task.yield()
		gateway.setState(state(with: [completedReceive(from: "sender-endpoint")]))
		await Task.yield()

		XCTAssertEqual(model.state.suggestions.count, 1)
	}
}
