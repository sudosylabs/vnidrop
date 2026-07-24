import Foundation
import Combine

/// Which acquisition method produced an invitation, ported from `ReceiveMethod`.
enum ReceiveMethod {
	case invitationFile
	case qrCode
	case nfc
}

enum ReceiveHistoryDeleteTarget: Equatable {
	case transfer(transferId: UInt64)
	case all
}

/// Receive feature state, ported from `feature/receive/ReceiveViewModel.kt`.
struct ReceiveState: Equatable {
	var isAcquisitionOpen = false
	var ticket = ""
	var method: ReceiveMethod?
	var inspection: TicketInspectionModel?
	var receiverName = ""
	var receiveFolder: ReceiveFolder?
	var folderAccessStatus: FolderAccessStatus = .unavailable
	var isInspecting = false
	var isReceiving = false
	var activeReceiveTransferId: UInt64?
	var lastReceiveError: UiText?
	var isWaitingForNfc = false
	var historyDeleteTarget: ReceiveHistoryDeleteTarget?
	var isDeletingHistory = false

	func canReceive(coreInitialized: Bool) -> Bool {
		coreInitialized && !ticket.isEmpty && inspection != nil
			&& folderAccessStatus == .writable && !isReceiving && !isInspecting
	}

	static func == (lhs: ReceiveState, rhs: ReceiveState) -> Bool {
		lhs.isAcquisitionOpen == rhs.isAcquisitionOpen && lhs.ticket == rhs.ticket
			&& lhs.inspection == rhs.inspection && lhs.receiverName == rhs.receiverName
			&& lhs.receiveFolder == rhs.receiveFolder && lhs.folderAccessStatus == rhs.folderAccessStatus
			&& lhs.isInspecting == rhs.isInspecting && lhs.isReceiving == rhs.isReceiving
			&& lhs.activeReceiveTransferId == rhs.activeReceiveTransferId
			&& lhs.lastReceiveError == rhs.lastReceiveError && lhs.isWaitingForNfc == rhs.isWaitingForNfc
			&& lhs.historyDeleteTarget == rhs.historyDeleteTarget && lhs.isDeletingHistory == rhs.isDeletingHistory
	}
}

@MainActor
final class ReceiveModel: ObservableObject {
	@Published private(set) var state = ReceiveState()
	@Published private(set) var coreState = CoreState()

	private let repository: CoreGateway
	private let fileSystemService: FileSystemService
	private let messages: UiMessageController
	private var cancellables = Set<AnyCancellable>()

	init(
		repository: CoreGateway,
		fileSystemService: FileSystemService,
		preferences: AppPreferencesRepository,
		messages: UiMessageController
	) {
		self.repository = repository
		self.fileSystemService = fileSystemService
		self.messages = messages

		repository.statePublisher.sink { [weak self] in self?.coreState = $0 }.store(in: &cancellables)

		preferences.$preferences
			.sink { [weak self] prefs in
				guard let self else { return }
				Task {
					let folder = self.fileSystemService.effectiveReceiveFolder(prefs.receiveFolder)
					let status = await self.fileSystemService.validateReceiveFolder(folder)
					if self.state.receiverName.isEmpty { self.state.receiverName = prefs.username }
					self.state.receiveFolder = folder
					self.state.folderAccessStatus = status
				}
			}
			.store(in: &cancellables)

		repository.signals
			.sink { [weak self] signal in
				guard let self else { return }
				if case .transfersChanged(let id) = signal {
					Task { _ = await self.repository.refresh() }
					if self.state.isReceiving && id != 0 {
						self.state.activeReceiveTransferId = id
					}
				}
			}
			.store(in: &cancellables)
	}

	func openAcquisition() { state.isAcquisitionOpen = true }
	func dismissAcquisition() {
		if !state.isReceiving && !state.isInspecting { resetAcquisition() }
	}
	func setReceiverName(_ value: String) { state.receiverName = value }
	func setWaitingForNfc(_ waiting: Bool) { state.isWaitingForNfc = waiting }

	func requestDeleteHistoryItem(_ transferId: UInt64) {
		let canDelete = coreState.transfers.contains {
			$0.transferId == transferId && $0.direction == .receive && $0.status.isTerminalReceiveHistory
		}
		if canDelete { state.historyDeleteTarget = .transfer(transferId: transferId) }
	}

	func requestClearHistory() {
		if coreState.transfers.contains(where: { $0.direction == .receive && $0.status.isTerminalReceiveHistory }) {
			state.historyDeleteTarget = .all
		}
	}

	func dismissHistoryDelete() {
		if !state.isDeletingHistory { state.historyDeleteTarget = nil }
	}

	func confirmHistoryDelete() {
		guard let target = state.historyDeleteTarget, !state.isDeletingHistory else { return }
		state.isDeletingHistory = true
		// Close synchronously: the alert's dismiss binding runs async and no-ops while
		// `isDeletingHistory`, which would otherwise leave the target set and macOS
		// re-present it.
		state.historyDeleteTarget = nil
		Task {
			let result: Result<Void, Error>
			switch target {
			case .transfer(let id):
				result = await repository.delete(transferId: id)
			case .all:
				result = (await repository.clearReceiveHistory()).map { _ in () }
			}
			switch result {
			case .success:
				state.historyDeleteTarget = nil
				state.isDeletingHistory = false
				let key = target == .all ? L10n.Receive.historyCleared : L10n.Transfer.deleted
				messages.tryShow(UiMessage(text: .resource(key), tone: .success))
			case .failure(let error):
				state.isDeletingHistory = false
				messages.error(error)
			}
		}
	}

	func onInvitationResult(_ method: ReceiveMethod, _ result: Result<String, Error>) {
		state.isWaitingForNfc = false
		switch result {
		case .success(let raw): inspectInvitation(method, raw)
		case .failure(let error): messages.error(error)
		}
	}

	func receive() {
		let current = state
		guard let folder = current.receiveFolder else { return }
		if !current.canReceive(coreInitialized: coreState.isInitialized) { return }
		state.isReceiving = true
		state.lastReceiveError = nil
		state.activeReceiveTransferId = nil
		Task {
			let result: Result<Void, Error>
			if folder.kind == .iosSecurityScopedUrl {
				result = await repository.receiveIntoSecurityScopedDirectory(
					ticket: current.ticket, outputDirectoryUrl: folder.value, receiverName: current.receiverName
				)
			} else {
				result = await repository.receive(
					ticket: current.ticket, outputDir: folder.value, receiverName: current.receiverName
				)
			}
			switch result {
			case .success:
				resetAcquisition()
				let canReveal = fileSystemService.canRevealReceiveFolder(folder)
				messages.tryShow(UiMessage(
					text: .resource(L10n.Receive.completed),
					tone: .success,
					actionLabel: canReveal ? .resource(L10n.Button.showInFiles) : nil,
					onAction: canReveal ? { self.revealReceiveFolder(folder) } : nil
				))
			case .failure(let error):
				if error.isUserCancellation {
					state.isReceiving = false
					state.activeReceiveTransferId = nil
					state.lastReceiveError = nil
					return
				}
				let uiText = error.toUiText()
				state.isReceiving = false
				state.activeReceiveTransferId = nil
				state.lastReceiveError = uiText
				messages.tryShow(UiMessage(
					text: uiText,
					tone: .error,
					actionLabel: .resource(L10n.Button.retry),
					onAction: { self.receive() }
				))
			}
		}
	}

	func cancelActiveReceive() {
		let transferId = state.activeReceiveTransferId
			?? coreState.transfers.first { $0.direction == .receive && $0.status == .receiving }?.transferId
			?? coreState.events.first { $0.eventDirection == .receive && $0.transferId != nil }?.transferId
		guard let transferId else { return }
		Task {
			let result = await repository.cancel(transferId: transferId)
			switch result {
			case .success:
				state.isReceiving = false
				state.activeReceiveTransferId = nil
				state.lastReceiveError = nil
				_ = await repository.refresh()
			case .failure(let error):
				messages.error(error)
			}
		}
	}

	private func revealReceiveFolder(_ folder: ReceiveFolder) {
		Task {
			let result = await fileSystemService.revealReceiveFolder(folder)
			if case .failure = result {
				messages.show(UiMessage(text: .resource(L10n.Receive.openFilesFailed), tone: .error))
			}
		}
	}

	private func inspectInvitation(_ method: ReceiveMethod, _ raw: String) {
		let ticket = raw.trimmingCharacters(in: .whitespacesAndNewlines)
		if ticket.isEmpty { return messages.error(.resource(L10n.Error.invitationEmpty)) }
		state.isAcquisitionOpen = true
		state.ticket = ticket
		state.method = method
		state.inspection = nil
		state.isInspecting = true
		Task {
			let result = await repository.inspectTicket(ticket)
			switch result {
			case .success(let inspection):
				state.inspection = inspection
				state.isInspecting = false
			case .failure(let error):
				state.ticket = ""
				state.method = nil
				state.inspection = nil
				state.isInspecting = false
				messages.error(error)
			}
		}
	}

	private func resetAcquisition() {
		state.isAcquisitionOpen = false
		state.ticket = ""
		state.method = nil
		state.inspection = nil
		state.isInspecting = false
		state.isReceiving = false
		state.activeReceiveTransferId = nil
		state.lastReceiveError = nil
		state.isWaitingForNfc = false
	}
}
