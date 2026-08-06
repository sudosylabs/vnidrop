import Foundation
import Combine

enum TransferDetailPanel: Equatable {
	case activity
	case receivers
	case share
}

/// Which invitation delivery action produced a result (for success copy).
enum InvitationAction {
	case export
	case share
	case nfc
}

/// Send feature state, ported from `feature/send/SendViewModel.kt` (`SendState`).
struct SendState: Equatable {
	var isComposerOpen = false
	var selectedFiles: [PickedShareFile] = []
	var transferName = ""
	var senderName = ""
	var accessPolicy: ShareAccessPolicy = .requireApproval
	var isSharing = false
	var selectedTransferId: UInt64?
	var transferThumbnails: [UInt64: Data] = [:]
	var detailPanel: TransferDetailPanel?
	/// Transfer whose share panel is presented inline from the list context menu
	/// (distinct from `detailPanel == .share`, which shows it from the detail view).
	/// Held in the model — not `SendScreen` @State — so the approval flow can dismiss
	/// it centrally before presenting its modal.
	var shareTargetId: UInt64?
	var receiverHistory: [ReceiverRequestModel] = []
	var isLoadingReceivers = false
	var isDeleteConfirmationOpen = false
	var isDeleting = false

	func canCreateShare(coreInitialized: Bool) -> Bool {
		coreInitialized && !selectedFiles.isEmpty && !transferName.isEmpty && !isSharing
	}
}

@MainActor
final class SendModel: ObservableObject {
	@Published private(set) var state = SendState()
	@Published private(set) var coreState = CoreState()

	/// Requests a file/folder pick or clipboard copy, consumed by the view layer.
	@Published var pendingFilePick = false
	@Published var pendingFolderPick = false

	/// Receiver delivery records per active (sharing) transfer, used to decide
	/// which transfers still have an in-flight receiver. Delivery status is the
	/// authoritative signal; byte-transfer events alone don't reliably mark a
	/// small transfer complete.
	@Published private(set) var receiversByTransfer: [UInt64: [ReceiverRequestModel]] = [:]

	private let repository: CoreGateway
	private let fileSystemService: FileSystemService
	private let filePreviewRepository: FilePreviewRepository
	private let messages: UiMessageController
	private var cancellables = Set<AnyCancellable>()

	/// Fires *after* a share/QR sheet (the detail-view panel or the list-level share
	/// sheet) has finished animating out. The approval flow waits on this to present
	/// its modal on macOS, where a sheet shown while another is still dismissing is
	/// dropped — using the real completion instead of a guessed delay.
	private let shareSheetsDismissedSubject = PassthroughSubject<Void, Never>()
	var shareSheetsDismissed: AnyPublisher<Void, Never> {
		shareSheetsDismissedSubject.eraseToAnyPublisher()
	}

	/// Invoked by a share sheet's `onDismiss` completion.
	func shareSheetDidDismiss() { shareSheetsDismissedSubject.send(()) }

	init(
		repository: CoreGateway,
		fileSystemService: FileSystemService,
		preferences: AppPreferencesRepository,
		filePreviewRepository: FilePreviewRepository,
		messages: UiMessageController
	) {
		self.repository = repository
		self.fileSystemService = fileSystemService
		self.filePreviewRepository = filePreviewRepository
		self.messages = messages

		repository.statePublisher.sink { [weak self] in self?.coreState = $0 }.store(in: &cancellables)

		repository.signals
			.sink { [weak self] signal in
				guard let self else { return }
				switch signal {
				case .transfersChanged:
					Task { _ = await self.repository.refresh() }
				case .receiverHistoryChanged(let id), .approvalChanged(let id):
					if id == self.state.selectedTransferId { self.refreshReceivers(id) }
					self.refreshReceiverStatuses(for: id)
				case .contactsChanged, .offersChanged:
					break
				}
			}
			.store(in: &cancellables)

		// Keep receiver delivery records current for every sharing/importing
		// outgoing transfer (new shares appear here; status transitions arrive via
		// the receiverHistoryChanged signal above).
		repository.statePublisher
			.map { core -> Set<UInt64> in
				Set(core.transfers
					.filter { $0.direction == .send && ($0.status == .sharing || $0.status == .importing) }
					.map(\.transferId))
			}
			.removeDuplicates()
			.sink { [weak self] ids in self?.syncSharingReceivers(ids) }
			.store(in: &cancellables)

		filePreviewRepository.$previews
			.sink { [weak self] previews in self?.state.transferThumbnails = previews }
			.store(in: &cancellables)

		repository.statePublisher
			.map { core -> Set<UInt64>? in
				core.isInitialized ? Set(core.transfers.map(\.transferId)) : nil
			}
			.removeDuplicates()
			.sink { [weak self] ids in
				if let ids { self?.filePreviewRepository.restore(activeTransferIds: ids) }
			}
			.store(in: &cancellables)

		preferences.$preferences
			.sink { [weak self] prefs in
				guard let self else { return }
				if self.state.senderName.isEmpty { self.state.senderName = prefs.username }
			}
			.store(in: &cancellables)
	}

	// MARK: - Composer

	func openComposer() {
		if state.isSharing { return }
		let discarded = state.selectedFiles
		state.isComposerOpen = true
		state.selectedFiles = []
		state.transferName = ""
		state.accessPolicy = .requireApproval
		discardPickedFiles(discarded)
	}

	func dismissComposer() {
		if state.isSharing { return }
		let discarded = state.selectedFiles
		state.isComposerOpen = false
		state.selectedFiles = []
		state.transferName = ""
		state.accessPolicy = .requireApproval
		discardPickedFiles(discarded)
	}

	func selectFile() { pendingFilePick = true }
	func selectFolder() { pendingFolderPick = true }

	func onFilesPicked(_ files: [PickedShareFile]) {
		if files.isEmpty { return }
		let selectedValues = Set(files.map(\.value))
		let discarded = state.selectedFiles.filter { !selectedValues.contains($0.value) }
		state.isComposerOpen = true
		state.selectedFiles = files
		state.transferName = defaultTransferName(files)
		discardPickedFiles(discarded)
	}

	func onFilePickFailed(_ reason: String) {
		messages.error(reason.isEmpty ? InvitationError.selectionFailed : InvitationError.raw(reason))
	}

	func clearSelectedSource() {
		let discarded = state.selectedFiles
		state.selectedFiles = []
		state.transferName = ""
		discardPickedFiles(discarded)
	}

	func removeSelectedFile(_ value: String) {
		let discarded = state.selectedFiles.filter { $0.value == value }
		let remaining = state.selectedFiles.filter { $0.value != value }
		let wasDefault = state.transferName == defaultTransferName(state.selectedFiles)
		state.selectedFiles = remaining
		state.transferName = remaining.isEmpty ? "" : (wasDefault ? defaultTransferName(remaining) : state.transferName)
		discardPickedFiles(discarded)
	}

	func setTransferName(_ value: String) { state.transferName = value }
	func setSenderName(_ value: String) { state.senderName = value }
	func setAccessPolicy(_ value: ShareAccessPolicy) { state.accessPolicy = value }

	// MARK: - Transfer detail

	func openTransfer(_ transferId: UInt64) {
		state.selectedTransferId = transferId
		state.detailPanel = nil
		refreshReceivers(transferId)
	}

	func closeTransferDetails() {
		state.selectedTransferId = nil
		state.detailPanel = nil
		state.receiverHistory = []
		state.isDeleteConfirmationOpen = false
	}

	func openActivity() { state.detailPanel = .activity }
	func openShare() { state.detailPanel = .share }
	func openReceivers() {
		guard let id = state.selectedTransferId else { return }
		state.detailPanel = .receivers
		refreshReceivers(id)
	}
	func closeDetailPanel() { state.detailPanel = nil }

	func openShareTarget(_ transferId: UInt64) { state.shareTargetId = transferId }
	func closeShareTarget() { state.shareTargetId = nil }

	/// Dismisses every share/QR surface at once — the detail-view share panel and the
	/// list-level share sheet. Used before presenting the receiver-approval modal, so
	/// no competing sheet is left open (macOS drops a sheet shown over another).
	func dismissShareSheets() {
		state.detailPanel = nil
		state.shareTargetId = nil
	}

	func requestDeleteTransfer() { state.isDeleteConfirmationOpen = true }
	func dismissDeleteTransfer() { if !state.isDeleting { state.isDeleteConfirmationOpen = false } }

	func confirmDeleteTransfer() {
		guard let transferId = state.selectedTransferId, !state.isDeleting else { return }
		state.isDeleting = true
		// Close synchronously: the alert's dismiss binding runs async and no-ops while
		// `isDeleting`, which would otherwise leave the flag true and macOS re-present it.
		state.isDeleteConfirmationOpen = false
		Task {
			let result = await repository.delete(transferId: transferId)
			switch result {
			case .success:
				filePreviewRepository.remove(transferId: transferId)
				state.selectedTransferId = nil
				state.detailPanel = nil
				state.receiverHistory = []
				state.isDeleteConfirmationOpen = false
				state.isDeleting = false
				messages.tryShow(UiMessage(text: .resource(L10n.Transfer.deleted), tone: .success))
			case .failure(let error):
				state.isDeleting = false
				messages.error(error)
			}
		}
	}

	/// Deletes a transfer by id, independent of the detail selection — used by the
	/// list context menu so it can act inline without navigating into the detail.
	func deleteTransfer(id: UInt64) {
		if state.isDeleting { return }
		state.isDeleting = true
		Task {
			let result = await repository.delete(transferId: id)
			switch result {
			case .success:
				filePreviewRepository.remove(transferId: id)
				if state.selectedTransferId == id {
					state.selectedTransferId = nil
					state.detailPanel = nil
					state.receiverHistory = []
				}
				state.isDeleting = false
				_ = await repository.refresh()
				messages.tryShow(UiMessage(text: .resource(L10n.Transfer.deleted), tone: .success))
			case .failure(let error):
				state.isDeleting = false
				messages.error(error)
			}
		}
	}

	/// Cancels/refuses a single receiver by responding to its request negatively.
	/// Uses the core's `respondReceiverRequest` (no backend change); applies to
	/// receivers that are still pending or accepted.
	func cancelReceiver(requestId: String) {
		respondToReceiver(requestId: requestId, accepted: false)
	}

	/// Approves a single pending receiver by responding to its request positively.
	/// A fallback for when the approval modal didn't surface — the pending receiver
	/// can still be accepted from its row in the transfer's receivers panel.
	func acceptReceiver(requestId: String) {
		respondToReceiver(requestId: requestId, accepted: true)
	}

	private func respondToReceiver(requestId: String, accepted: Bool) {
		Task {
			let result = await repository.respondReceiverRequest(requestId: requestId, accepted: accepted, reason: nil)
			switch result {
			case .success:
				if let transferId = state.selectedTransferId { refreshReceivers(transferId) }
				_ = await repository.refresh()
			case .failure(let error):
				messages.error(error)
			}
		}
	}

	/// Stops an active outgoing share (interrupts any in-flight receivers). The
	/// transfer stays in history as "Stopped". Uses the core's `cancelTransfer`.
	func stopSharing(transferId: UInt64) {
		Task {
			let result = await repository.cancel(transferId: transferId)
			switch result {
			case .success:
				_ = await repository.refresh()
				messages.tryShow(UiMessage(text: .resource(L10n.Transfer.eventStopped), tone: .info))
			case .failure(let error):
				messages.error(error)
			}
		}
	}

	// MARK: - Invitation results / share

	func onInvitationResult(_ action: InvitationAction, _ result: Result<Void, Error>) {
		switch result {
		case .success:
			let key: String.LocalizationValue?
			switch action {
			case .export: key = L10n.Transfer.invitationSaved
			case .nfc: key = L10n.Transfer.nfcWritten
			case .share: key = nil  // system share sheet already confirms
			}
			if let key { messages.tryShow(UiMessage(text: .resource(key), tone: .success)) }
		case .failure(let error):
			messages.error(error)
		}
	}

	func createShare() {
		let current = state
		if current.selectedFiles.isEmpty { return }
		if !current.canCreateShare(coreInitialized: coreState.isInitialized) { return }
		state.isSharing = true
		Task {
			let result = await fileSystemService.sharePickedFiles(
				repository: repository,
				files: current.selectedFiles,
				transferName: current.transferName.trimmingCharacters(in: .whitespacesAndNewlines),
				senderName: current.senderName.trimmingCharacters(in: .whitespacesAndNewlines),
				accessPolicy: current.accessPolicy
			)
			switch result {
			case .success(let share):
				await fileSystemService.discardPickedFiles(current.selectedFiles)
				if let thumb = current.selectedFiles.compactMap(\.thumbnailData).first {
					filePreviewRepository.save(transferId: share.transferId, bytes: thumb)
				}
				state.isComposerOpen = false
				state.selectedFiles = []
				state.transferName = ""
				state.accessPolicy = .requireApproval
				state.isSharing = false
				// Jump straight to the new transfer's share panel (QR + delivery) rather
				// than dropping the user on the list to drill in manually. Refresh first
				// so the transfer exists in state before it's selected.
				_ = await repository.refresh()
				state.selectedTransferId = share.transferId
				state.detailPanel = .share
				refreshReceivers(share.transferId)
				messages.show(UiMessage(text: .resource(L10n.Send.transferCreated), tone: .success))
			case .failure(let error):
				state.isSharing = false
				messages.error(error)
			}
		}
	}

	// MARK: - Internals

	private func defaultTransferName(_ files: [PickedShareFile]) -> String {
		if files.isEmpty { return "" }
		if files.count == 1 { return files[0].displayName }
		if files.allSatisfy(\.isDirectory) { return "\(files.count) folders" }
		return "\(files.count) files"
	}

	private func discardPickedFiles(_ files: [PickedShareFile]) {
		if files.isEmpty { return }
		Task { await fileSystemService.discardPickedFiles(files) }
	}

	/// Refresh the receiver records for the sharing set, pruning transfers that are
	/// no longer active.
	private func syncSharingReceivers(_ ids: Set<UInt64>) {
		receiversByTransfer = receiversByTransfer.filter { ids.contains($0.key) }
		for id in ids { refreshReceiverStatuses(for: id) }
	}

	private func refreshReceiverStatuses(for transferId: UInt64) {
		Task {
			if case .success(let requests) = await repository.receiverRequests(transferId: transferId) {
				receiversByTransfer[transferId] = requests
			}
		}
	}

	private func refreshReceivers(_ transferId: UInt64) {
		state.isLoadingReceivers = true
		Task {
			let result = await repository.receiverRequests(transferId: transferId)
			switch result {
			case .success(let requests):
				state.receiverHistory = requests
				state.isLoadingReceivers = false
			case .failure(let error):
				state.isLoadingReceivers = false
				messages.error(error)
			}
		}
	}
}
