import SwiftUI
import SFSafeSymbols

/// Receive screen, rebuilt on native SwiftUI. A grouped `List` of received
/// transfers with swipe-to-delete, and the acquisition flow as a native sheet.
struct ReceiveScreen: View {
	@ObservedObject var model: ReceiveModel
	let windowClass: WindowClass

	private var transfers: [Transfer] {
		model.coreState.transfers.filter { $0.direction == .receive }
	}
	private var deletable: [Transfer] {
		transfers.filter { $0.status.isTerminalReceiveHistory }
	}

	var body: some View {
		NavigationStack {
			Group {
				if transfers.isEmpty {
					emptyState
				} else {
					history
				}
			}
			.navigationTitle(Text(String(localized: L10n.Receive.title)))
			.toolbar {
				ToolbarItem(placement: .primaryAction) {
					Button(action: model.openAcquisition) {
						Label(String(localized: L10n.Button.receiveFiles), systemSymbol: .plus)
					}
				}
				if !deletable.isEmpty {
					ToolbarItem(placement: .primaryAction) {
						Button(role: .destructive, action: model.requestClearHistory) {
							Label(String(localized: L10n.Receive.clearHistory), systemSymbol: .trash)
						}
					}
				}
			}
		}
		.adaptiveDrawer(
			isPresented: Binding(get: { model.state.isAcquisitionOpen }, set: { _ in }),
			windowClass: windowClass,
			onDismiss: model.dismissAcquisition
		) {
			if model.state.ticket.isEmpty {
				ReceiveMethodPanel(model: model)
			} else {
				InvitationReviewPanel(model: model)
			}
		}
		.alert(
			Text(String(localized: clearAllPending ? L10n.Receive.clearHistoryTitle : L10n.Receive.deleteHistoryTitle)),
			isPresented: Binding(get: { model.state.historyDeleteTarget != nil }, set: { if !$0 { Task { @MainActor in model.dismissHistoryDelete() } } })
		) {
			Button(String(localized: L10n.Button.cancel), role: .cancel, action: model.dismissHistoryDelete)
			Button(String(localized: clearAllPending ? L10n.Receive.clearHistory : L10n.Button.deleteTransfer),
				   role: .destructive, action: model.confirmHistoryDelete)
		} message: {
			historyDeleteMessage
		}
	}

	private var history: some View {
		List {
			Section {
				ForEach(transfers) { transfer in
					ReceiveTransferRow(
						transfer: transfer,
						progress: progressForTransfer(events: model.coreState.events, transferId: transfer.transferId)
					)
					.swipeActions(edge: .trailing, allowsFullSwipe: true) {
						if transfer.status.isTerminalReceiveHistory {
							Button(role: .destructive) {
								model.requestDeleteHistoryItem(transfer.transferId)
							} label: {
								Label(String(localized: L10n.Button.deleteTransfer), systemSymbol: .trash)
							}
						}
					}
					.contextMenu {
						if transfer.status.isTerminalReceiveHistory {
							Button(role: .destructive) {
								model.requestDeleteHistoryItem(transfer.transferId)
							} label: {
								Label(String(localized: L10n.Button.deleteTransfer), systemSymbol: .trash)
							}
						}
					}
				}
			} header: {
				Text(String(localized: L10n.Receive.historyTitle))
			} footer: {
				Text(String(localized: L10n.Receive.newSubtitle))
			}
		}
	}

	private var emptyState: some View {
		ContentUnavailableView {
			Label(String(localized: L10n.Receive.emptyTitle), systemSymbol: .trayAndArrowDown)
		} description: {
			Text(String(localized: L10n.Receive.emptyBody))
		} actions: {
			Button(action: model.openAcquisition) {
				Label(String(localized: L10n.Button.receiveFiles), systemSymbol: .plus)
			}
			.buttonStyle(.borderedProminent)
			.controlSize(.large)
		}
	}

	private var clearAllPending: Bool { model.state.historyDeleteTarget == .all }

	@ViewBuilder
	private var historyDeleteMessage: some View {
		if let target = model.state.historyDeleteTarget {
			if target == .all {
				Text(String(localized: L10n.Receive.clearHistoryDescription))
			} else {
				Text(L10n.Receive.deleteHistoryDescription(
					transferName: transferName(for: target) ?? String(localized: L10n.Receive.unknownTransfer)))
			}
		}
	}

	private func transferName(for target: ReceiveHistoryDeleteTarget) -> String? {
		if case .transfer(let id) = target {
			return transfers.first { $0.transferId == id }?.transferName
		}
		return nil
	}
}

private struct ReceiveTransferRow: View {
	let transfer: Transfer
	let progress: TransferProgress?

	var body: some View {
		HStack(spacing: 12) {
			Image(systemSymbol: .doc)
				.foregroundStyle(.secondary)
				.frame(width: 40, height: 40)
				.background(.quaternary, in: RoundedRectangle(cornerRadius: 9))
			VStack(alignment: .leading, spacing: 3) {
				Text(transfer.transferName ?? String(localized: L10n.Receive.unknownTransfer))
					.font(.body).lineLimit(1)
				Text(L10n.Format.separatedPair(first: formatBytes(transfer.totalSize), second: statusLabel(transfer.status)))
					.font(.caption).foregroundStyle(.secondary)
				if transfer.status == .receiving, let progress {
					ProgressRow(labelKey: progress.labelKey, progress: progress.progress, detail: progress.detail)
						.padding(.top, 2)
				}
			}
			Spacer(minLength: 0)
		}
	}
}
