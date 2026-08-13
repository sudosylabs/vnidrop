import Foundation
import Combine

/// Saved-device feature state, ported from `SavedDevicesViewModel.kt`
/// (`SavedDevicesState`). One snapshot drives the list, the details surface, the
/// pairing prompt, and the targeted-offer modal.
struct SavedDevicesState: Equatable {
	var isLoading = true
	var loadFailed = false
	var eligibilities: [PairingEligibilityModel] = []
	/// Relationships still awaiting consent on one side or the other. Rendered on
	/// the main screen alongside saved devices; never usable for a transfer.
	var pendingRelationships: [DeviceRelationshipModel] = []
	var savedDevices: [SavedDeviceModel] = []
	var targetedTransfers: [SavedDeviceTransferItem] = []
	var pairingPrompt = PairingPromptState()
	var targetedOffers = TargetedOfferState()
	/// Peers with a mutation in flight; their row actions are disabled.
	var busyPeerIds: Set<String> = []
	var busyTransferIds: Set<String> = []
	/// Peer whose label editor is open, or nil when closed.
	var labelingPeerId: String?
	var labelDraft = ""
	var isSavingLabel = false
	/// Peer the user chose to send to, or nil when no composition is open.
	var sendTargetPeerId: String?
	/// Sources chosen for the pending targeted send.
	var sendFiles: [PickedShareFile] = []
	var sendTransferName = ""
	var isCreatingSend = false

	/// True when the device being labelled already has one, so "Clear" has
	/// something to clear.
	var hasExistingLabel: Bool {
		guard let peerId = labelingPeerId else { return false }
		return device(peerId)?.localLabel?.isEmpty == false
	}

	/// Saving is pointless when the draft matches what is already stored.
	var canSaveLabel: Bool {
		guard let peerId = labelingPeerId, !isSavingLabel else { return false }
		let draft = labelDraft.trimmingCharacters(in: .whitespacesAndNewlines)
		let current = device(peerId)?.localLabel ?? ""
		return draft != current
	}

	/// A targeted transfer needs a destination, at least one source, and a name —
	/// the same composition rules as an invitation share.
	var canCreateTargetedTransfer: Bool {
		sendTargetPeerId != nil
			&& !sendFiles.isEmpty
			&& !sendTransferName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
			&& !isCreatingSend
	}

	var isEmpty: Bool {
		savedDevices.isEmpty && pendingRelationships.isEmpty && eligibilities.isEmpty
	}

	func transfers(for peerEndpointId: String) -> [SavedDeviceTransferItem] {
		targetedTransfers.filter { $0.peerEndpointId == peerEndpointId }
	}

	func device(_ peerEndpointId: String) -> SavedDeviceModel? {
		savedDevices.first { $0.endpointId == peerEndpointId }
	}
}

/// Product-level Saved-device experience, ported from `SavedDevicesViewModel.kt`.
/// Views observe one snapshot and issue named commands; pairing, targeted offers,
/// transfer history, and receive destinations stay internal.
@MainActor
final class SavedDevicesModel: ObservableObject {
	@Published private(set) var state = SavedDevicesState()

	/// Requests a file/folder pick, consumed by the view layer (mirrors SendModel).
	@Published var pendingFilePick = false
	@Published var pendingFolderPick = false

	private let repository: CoreGateway
	private let fileSystemService: FileSystemService
	private let messages: UiMessageController
	private var cancellables = Set<AnyCancellable>()

	/// Serializes `refresh` so overlapping signals cannot interleave their reads
	/// and publish a torn snapshot (the `refreshMutex` in the KMP model).
	private var refreshTask: Task<Void, Never>?
	/// Eligibilities the user dismissed this session. Dismissal is not a decline:
	/// it suppresses the prompt locally without consuming the core's single-use
	/// capability, so the device stays actionable from the list.
	private var dismissedEligibility: Set<String> = []
	private var receiveFolder: ReceiveFolder?

	init(
		repository: CoreGateway,
		fileSystemService: FileSystemService,
		preferences: AppPreferencesRepository,
		messages: UiMessageController
	) {
		self.repository = repository
		self.fileSystemService = fileSystemService
		self.messages = messages

		// Re-resolve the destination whenever the configured folder changes, and
		// load once the core is up. Both inputs gate the first refresh: a receive
		// has nowhere to land until the folder is known.
		preferences.$preferences
			.map(\.receiveFolder)
			.combineLatest(repository.statePublisher.map(\.isInitialized))
			.removeDuplicates { $0 == $1 }
			.sink { [weak self] folder, isInitialized in
				guard let self else { return }
				self.receiveFolder = self.fileSystemService.effectiveReceiveFolder(folder)
				if isInitialized { self.scheduleRefresh() }
			}
			.store(in: &cancellables)

		repository.signals
			.sink { [weak self] signal in
				guard let self else { return }
				switch signal {
				case .pairingChanged, .targetedTransferChanged:
					// Wake-up only: re-read durable state rather than trusting the
					// event payload (see DESIGN-DEVICE-HISTORY.md §13).
					if self.repository.state.isInitialized { self.scheduleRefresh() }
				case .approvalChanged, .receiverHistoryChanged, .transfersChanged:
					// Invitation-share domain; owned by SendModel.
					break
				}
			}
			.store(in: &cancellables)
	}

	// MARK: - Loading

	func retry() {
		guard repository.state.isInitialized, !state.isLoading else { return }
		scheduleRefresh()
	}

	// MARK: - Pairing prompt

	func acceptPairingPrompt() {
		respondToPrompt(accepted: true)
	}

	func declinePairingPrompt() {
		respondToPrompt(accepted: false)
	}

	/// Hides the prompt without answering it. Only meaningful for an eligibility —
	/// an incoming request stays until it is explicitly answered.
	func dismissPairingPrompt() {
		guard let prompt = state.pairingPrompt.prompt else { return }
		if case .eligibility(let peerId, _) = prompt { dismissedEligibility.insert(peerId) }
		state.pairingPrompt.prompt = nil
	}

	private func respondToPrompt(accepted: Bool) {
		guard let prompt = state.pairingPrompt.prompt, !state.pairingPrompt.busy else { return }
		state.pairingPrompt.busy = true
		Task {
			let result: Result<Void, Error>
			switch (prompt, accepted) {
			case (.eligibility(let peerId, _), true):
				result = await repository.requestSavedDevicePairing(peerEndpointId: peerId).map { _ in () }
			case (.eligibility(let peerId, _), false):
				result = await repository.declinePairingEligibility(peerEndpointId: peerId)
			case (.incomingRequest(let peerId, _), _):
				result = await repository
					.respondToDevicePairing(peerEndpointId: peerId, accepted: accepted)
					.map { _ in () }
			}
			state.pairingPrompt.busy = false
			switch result {
			case .success: await refresh()
			case .failure(let error): messages.error(error)
			}
		}
	}

	// MARK: - Per-device consent actions

	func rememberEligible(_ peerEndpointId: String) {
		mutatePeer(peerEndpointId) {
			await self.repository.requestSavedDevicePairing(peerEndpointId: peerEndpointId).map { _ in () }
		}
	}

	func declineEligible(_ peerEndpointId: String) {
		mutatePeer(peerEndpointId) {
			await self.repository.declinePairingEligibility(peerEndpointId: peerEndpointId)
		}
	}

	func acceptIncoming(_ peerEndpointId: String) {
		mutatePeer(peerEndpointId) {
			await self.repository
				.respondToDevicePairing(peerEndpointId: peerEndpointId, accepted: true)
				.map { _ in () }
		}
	}

	func declineIncoming(_ peerEndpointId: String) {
		mutatePeer(peerEndpointId) {
			await self.repository
				.respondToDevicePairing(peerEndpointId: peerEndpointId, accepted: false)
				.map { _ in () }
		}
	}

	// MARK: - Targeted offers

	func acceptTargetedOffer(_ transferId: String) {
		respondToTargetedOffer(transferId, accepted: true)
	}

	func declineTargetedOffer(_ transferId: String) {
		respondToTargetedOffer(transferId, accepted: false)
	}

	private func respondToTargetedOffer(_ transferId: String, accepted: Bool) {
		guard !state.targetedOffers.respondingIds.contains(transferId) else { return }
		state.targetedOffers.respondingIds.insert(transferId)
		Task {
			let response = await repository.respondToTargetedOffer(
				transferId: transferId, accepted: accepted
			)
			switch response {
			case .success(let outcome):
				// Approval only authorizes the pull; the receiver still has to run
				// it. `alreadySettled` is the idempotent replay path and must not
				// start a second one.
				if accepted, case .approved(let approvedId) = outcome {
					if case .failure(let error) = await pullTargetedTransfer(approvedId, resume: false) {
						messages.error(error)
					}
				}
				await refresh()
			case .failure(let error):
				messages.error(error)
			}
			state.targetedOffers.respondingIds.remove(transferId)
		}
	}

	// MARK: - Targeted transfer lifecycle

	func receiveTargetedTransfer(_ transferId: String) {
		mutateTransfer(transferId) { await self.pullTargetedTransfer(transferId, resume: false) }
	}

	func resumeTargetedTransfer(_ transferId: String) {
		mutateTransfer(transferId) { await self.pullTargetedTransfer(transferId, resume: true) }
	}

	func cancelTargetedTransfer(_ transferId: String) {
		mutateTransfer(transferId) { await self.repository.cancelTargetedTransfer(id: transferId) }
	}

	func deleteTargetedTransfer(_ transferId: String) {
		mutateTransfer(transferId) { await self.repository.deleteTargetedTransfer(id: transferId) }
	}

	private func pullTargetedTransfer(_ transferId: String, resume: Bool) async -> Result<Void, Error> {
		guard let folder = receiveFolder else {
			// Preferences have not resolved a usable destination yet; the pull has
			// nowhere to land. Surfaces as the generic filesystem error.
			return .failure(InvitationError.filesystemUnavailable)
		}
		let result = resume
			? await repository.resumeTargetedTransfer(id: transferId, outputDirectoryUrl: folder.value)
			: await repository.receiveTargetedTransfer(
				transferId: transferId, outputDirectoryUrl: folder.value
			)
		if case .success = result {
			messages.tryShow(UiMessage(text: .resource(L10n.Receive.completed), tone: .success))
		}
		return result
	}

	// MARK: - Send

	/// Opens targeted-send composition for a saved device.
	func beginSend(to peerEndpointId: String) {
		guard !state.busyPeerIds.contains(peerEndpointId) else { return }
		let discarded = state.sendFiles
		state.sendTargetPeerId = peerEndpointId
		state.sendFiles = []
		state.sendTransferName = ""
		discardPickedFiles(discarded)
	}

	func cancelSend() {
		guard !state.isCreatingSend else { return }
		let discarded = state.sendFiles
		state.sendTargetPeerId = nil
		state.sendFiles = []
		state.sendTransferName = ""
		discardPickedFiles(discarded)
	}

	func selectSendFiles() { pendingFilePick = true }
	func selectSendFolder() { pendingFolderPick = true }

	func onSendFilesPicked(_ files: [PickedShareFile]) {
		guard !files.isEmpty else { return }
		// Replacing a selection discards the picker copies the previous one owned.
		let selectedValues = Set(files.map(\.value))
		let discarded = state.sendFiles.filter { !selectedValues.contains($0.value) }
		state.sendFiles = files
		state.sendTransferName = defaultTransferName(files)
		discardPickedFiles(discarded)
	}

	func onSendFilePickFailed(_ reason: String) {
		messages.error(reason.isEmpty ? InvitationError.selectionFailed : InvitationError.raw(reason))
	}

	func removeSendFile(_ value: String) {
		let discarded = state.sendFiles.filter { $0.value == value }
		let remaining = state.sendFiles.filter { $0.value != value }
		// Keep a name the user typed; only re-derive one we generated.
		let wasDefault = state.sendTransferName == defaultTransferName(state.sendFiles)
		state.sendFiles = remaining
		state.sendTransferName = remaining.isEmpty
			? ""
			: (wasDefault ? defaultTransferName(remaining) : state.sendTransferName)
		discardPickedFiles(discarded)
	}

	func clearSendFiles() {
		let discarded = state.sendFiles
		state.sendFiles = []
		state.sendTransferName = ""
		discardPickedFiles(discarded)
	}

	func setSendTransferName(_ value: String) {
		guard !state.isCreatingSend else { return }
		state.sendTransferName = value
	}

	/// Creates the targeted transfer. The receiver still has to approve it — a
	/// saved device never grants automatic receipt.
	func createTargetedTransfer() {
		guard state.canCreateTargetedTransfer, let peerId = state.sendTargetPeerId else { return }
		let files = state.sendFiles
		let name = state.sendTransferName.trimmingCharacters(in: .whitespacesAndNewlines)
		state.isCreatingSend = true
		Task {
			let result = await fileSystemService.sendPickedFilesToSavedDevice(
				repository: repository,
				files: files,
				transferName: name,
				receiverEndpointId: peerId
			)
			state.isCreatingSend = false
			switch result {
			case .success:
				messages.tryShow(
					UiMessage(text: .resource(L10n.Saved.devicesSendStarted), tone: .success)
				)
				state.sendTargetPeerId = nil
				state.sendFiles = []
				state.sendTransferName = ""
				// The core owns the bytes now; release any picker copies.
				discardPickedFiles(files)
				await refresh()
			case .failure(let error):
				// Keep the composition intact so the user can retry without
				// re-picking, mirroring the label editor's failure behavior.
				messages.error(error)
			}
		}
	}

	private func defaultTransferName(_ files: [PickedShareFile]) -> String {
		guard let first = files.first else { return "" }
		return files.count == 1
			? first.displayName
			: L10n.Send.selectedFilesCount(count: files.count)
	}

	private func discardPickedFiles(_ files: [PickedShareFile]) {
		guard !files.isEmpty else { return }
		Task { await fileSystemService.discardPickedFiles(files) }
	}

	// MARK: - Label editing

	func openLabelEditor(_ peerEndpointId: String) {
		guard !state.isSavingLabel, !state.busyPeerIds.contains(peerEndpointId) else { return }
		state.labelingPeerId = peerEndpointId
		state.labelDraft = state.device(peerEndpointId)?.localLabel ?? ""
	}

	func setLabelDraft(_ value: String) {
		// Frozen while saving so the committed value cannot drift from the draft
		// the user is looking at.
		guard !state.isSavingLabel else { return }
		state.labelDraft = value
	}

	func dismissLabelEditor() {
		// Refuse to close mid-save: the draft must survive to be retried.
		guard !state.isSavingLabel else { return }
		state.labelingPeerId = nil
		state.labelDraft = ""
	}

	func saveLabel() {
		let trimmed = state.labelDraft.trimmingCharacters(in: .whitespacesAndNewlines)
		commitLabel(trimmed.isEmpty ? nil : trimmed)
	}

	func clearLabel() {
		commitLabel(nil)
	}

	/// Transactional from the UI's perspective: on failure the draft and the
	/// editor survive so the user can retry; the editor closes only after the
	/// core confirms the write.
	private func commitLabel(_ label: String?) {
		guard let peerId = state.labelingPeerId else { return }
		guard !state.isSavingLabel, !state.busyPeerIds.contains(peerId) else { return }
		state.isSavingLabel = true
		state.busyPeerIds.insert(peerId)
		Task {
			let result = await repository.setSavedDeviceLabel(peerEndpointId: peerId, label: label)
			switch result {
			case .success:
				await refresh()
				messages.tryShow(UiMessage(text: .resource(L10n.Saved.devicesLabeled), tone: .success))
			case .failure(let error):
				messages.error(error)
			}
			state.busyPeerIds.remove(peerId)
			state.isSavingLabel = false
			// Only close the editor the user still has open on this peer — they may
			// have switched to another device while the write was in flight.
			if case .success = result, state.labelingPeerId == peerId {
				state.labelingPeerId = nil
				state.labelDraft = ""
			}
		}
	}

	// MARK: - Destructive actions

	func forget(_ peerEndpointId: String) {
		mutatePeer(peerEndpointId) {
			let result = await self.repository.forgetSavedDevice(peerEndpointId: peerEndpointId)
			if case .success = result {
				self.messages.tryShow(
					UiMessage(text: .resource(L10n.Saved.devicesForgotten), tone: .success)
				)
			}
			return result
		}
	}

	func block(_ peerEndpointId: String) {
		mutatePeer(peerEndpointId) {
			let result = await self.repository.blockDevice(peerEndpointId: peerEndpointId)
			if case .success = result {
				self.messages.tryShow(
					UiMessage(text: .resource(L10n.Saved.devicesBlocked), tone: .success)
				)
			}
			return result
		}
	}

	// MARK: - Mutation helpers

	private func mutatePeer(
		_ peerEndpointId: String,
		_ block: @escaping () async -> Result<Void, Error>
	) {
		guard !state.busyPeerIds.contains(peerEndpointId) else { return }
		state.busyPeerIds.insert(peerEndpointId)
		Task {
			switch await block() {
			case .success: await refresh()
			case .failure(let error): messages.error(error)
			}
			state.busyPeerIds.remove(peerEndpointId)
		}
	}

	private func mutateTransfer(
		_ transferId: String,
		_ block: @escaping () async -> Result<Void, Error>
	) {
		guard !state.busyTransferIds.contains(transferId) else { return }
		state.busyTransferIds.insert(transferId)
		Task {
			switch await block() {
			case .success: await refresh()
			case .failure(let error): messages.error(error)
			}
			state.busyTransferIds.remove(transferId)
		}
	}

	// MARK: - Refresh

	/// Coalesces refreshes onto one serial chain. Signals can arrive in bursts;
	/// without this their reads interleave and publish a torn snapshot.
	private func scheduleRefresh() {
		let previous = refreshTask
		refreshTask = Task {
			await previous?.value
			await refresh()
		}
	}

	private func refresh() async {
		state.isLoading = true
		state.loadFailed = false

		// Five reads make one snapshot; if any fails the snapshot is incomplete, so
		// surface the failure rather than render a partial list. Sequential by
		// design: the gateway funnels core calls through one serial dispatcher, so
		// issuing these concurrently would queue behind each other anyway.
		do {
			let eligibilities = try await repository.listPairingEligibilities().get()
			let relationships = try await repository.listDeviceRelationships().get()
			let savedDevices = try await repository.listSavedDevices().get()
			let pendingOffers = try await repository.listPendingTargetedOffers().get()
			let transfers = try await repository.listTargetedTransfers().get()

			let savedNames = savedDevices.reduce(into: [String: String]()) { names, device in
				if let name = device.displayNameOrNil { names[device.endpointId] = name }
			}
			let localEndpointId = repository.state.status?.endpointId
			let pendingRelationships = relationships
				.filter { $0.state == .pendingIncoming || $0.state == .pendingOutgoing }
				.sorted { $0.updatedAt > $1.updatedAt }

			state.isLoading = false
			state.loadFailed = false
			state.eligibilities = eligibilities.sorted { $0.createdAt > $1.createdAt }
			state.pendingRelationships = pendingRelationships
			state.savedDevices = savedDevices.sorted { $0.createdAt > $1.createdAt }
			state.targetedTransfers = transfers
				.filter { $0.state != .deleted }
				.sorted { $0.updatedAt > $1.updatedAt }
				.map { $0.toExperienceItem(localEndpointId: localEndpointId, savedNames: savedNames) }
			// Leave a prompt mid-answer alone; replacing it would strand the
			// in-flight request behind a prompt the user never saw.
			if !state.pairingPrompt.busy {
				state.pairingPrompt = PairingPromptState(
					prompt: nextPairingPrompt(
						relationships: pendingRelationships,
						eligibilities: eligibilities,
						savedNames: savedNames
					)
				)
			}
			state.targetedOffers.pending = pendingOffers.sorted { $0.receivedAt < $1.receivedAt }
			state.targetedOffers.senderDisplayNames = savedNames
		} catch {
			state.isLoading = false
			state.loadFailed = true
			messages.error(error)
		}
	}

	/// An incoming request outranks an eligibility: the peer is waiting on us,
	/// and answering it is the only action that unblocks them.
	private func nextPairingPrompt(
		relationships: [DeviceRelationshipModel],
		eligibilities: [PairingEligibilityModel],
		savedNames: [String: String]
	) -> PairingPrompt? {
		if let incoming = relationships.first(where: { $0.state == .pendingIncoming }) {
			let name = eligibilities
				.first { $0.peerEndpointId == incoming.remoteEndpointId }?
				.remoteDisplayName
				?? savedNames[incoming.remoteEndpointId]
			return .incomingRequest(peerEndpointId: incoming.remoteEndpointId, remoteDisplayName: name)
		}
		guard let eligibility = eligibilities.first(where: {
			!dismissedEligibility.contains($0.peerEndpointId)
		}) else { return nil }
		return .eligibility(
			peerEndpointId: eligibility.peerEndpointId,
			remoteDisplayName: eligibility.remoteDisplayName
		)
	}
}

private extension TargetedTransferModel {
	/// Resolves the transfer against the local endpoint so the UI can speak in
	/// terms of "the peer" and a direction.
	func toExperienceItem(
		localEndpointId: String?,
		savedNames: [String: String]
	) -> SavedDeviceTransferItem {
		let outgoing = senderEndpointId == localEndpointId
		let peerEndpointId = outgoing ? receiverEndpointId : senderEndpointId
		return SavedDeviceTransferItem(
			id: id,
			peerEndpointId: peerEndpointId,
			peerDisplayName: savedNames[peerEndpointId],
			direction: outgoing ? .outgoing : .incoming,
			transferName: transferName,
			fileCount: fileCount,
			totalSize: totalSize,
			verifiedBytes: verifiedBytes,
			state: state,
			createdAt: createdAt,
			updatedAt: updatedAt
		)
	}
}
