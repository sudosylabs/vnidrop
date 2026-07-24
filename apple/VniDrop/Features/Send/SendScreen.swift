import SwiftUI
import SFSafeSymbols

/// Send screen, rebuilt on native SwiftUI. A grouped `List` of outgoing transfers,
/// with the composer and detail panels as native sheets and delete as an alert.
struct SendScreen: View {
	@ObservedObject var model: SendModel
	let windowClass: WindowClass

	/// Transfer whose share panel is presented inline from the list context menu.
	@State private var shareTarget: Transfer?
	/// Transfer pending an inline (list-level) delete confirmation.
	@State private var deleteTarget: Transfer?

	private var outgoing: [Transfer] {
		model.coreState.transfers.filter { $0.direction == .send }
	}
	private var selectedTransfer: Transfer? {
		guard let id = model.state.selectedTransferId else { return nil }
		return outgoing.first { $0.transferId == id }
	}

	private var detailsBinding: Binding<Bool> {
		Binding(get: { model.state.selectedTransferId != nil }, set: { if !$0 { model.closeTransferDetails() } })
	}

	var body: some View {
		NavigationStack {
			Group {
				if outgoing.isEmpty {
					emptyState
				} else {
					catalog
				}
			}
			.navigationTitle(Text(String(localized: L10n.Send.title)))
			.toolbar {
				ToolbarItem(placement: .primaryAction) {
					Button(action: model.openComposer) {
						Label(String(localized: L10n.Button.createNewTransfer), systemSymbol: .plus)
					}
				}
			}
			.navigationDestination(isPresented: detailsBinding) {
				if let transfer = selectedTransfer {
					detailView(for: transfer)
				}
			}
			// Attached inside the NavigationStack (a different sheet host than the
			// composer drawer on the outer body, so the two don't clash). Opens the
			// share panel over the list without navigating into the transfer detail.
			.adaptiveDrawer(
				isPresented: Binding(get: { shareTarget != nil }, set: { if !$0 { shareTarget = nil } }),
				windowClass: windowClass,
				onDismiss: { shareTarget = nil }
			) {
				if let shareTarget {
					TransferSharePanel(model: model, transfer: shareTarget)
				}
			}
		}
		.adaptiveDrawer(
			isPresented: Binding(get: { model.state.isComposerOpen }, set: { _ in }),
			windowClass: windowClass,
			onDismiss: model.dismissComposer
		) {
			TransferComposer(model: model, windowClass: windowClass)
		}
		.alert(
			Text(String(localized: L10n.Transfer.deleteTitle)),
			isPresented: Binding(get: { deleteTarget != nil }, set: { if !$0 { deleteTarget = nil } })
		) {
			Button(String(localized: L10n.Button.cancel), role: .cancel) { deleteTarget = nil }
			Button(String(localized: L10n.Button.deleteTransfer), role: .destructive) {
				if let target = deleteTarget { model.deleteTransfer(id: target.transferId) }
				deleteTarget = nil
			}
		} message: {
			if let target = deleteTarget {
				Text(L10n.Transfer.deleteDescription(
					transferName: target.transferName ?? String(localized: L10n.Send.newTransferTitle)))
			}
		}
	}


	/// The pushed transfer details view, with its detail-panel sheet and delete
	/// alert attached here so they present from the detail's own context (presenting
	/// modals from the parent stack while a detail is pushed is unreliable on macOS).
	private func detailView(for transfer: Transfer) -> some View {
		TransferDetailsView(model: model, transfer: transfer, events: model.coreState.events)
			.adaptiveDrawer(
				isPresented: Binding(get: { model.state.detailPanel != nil }, set: { _ in }),
				windowClass: windowClass,
				onDismiss: model.closeDetailPanel
			) {
				if let panel = model.state.detailPanel {
					DetailPanelContent(model: model, transfer: transfer, panel: panel)
				}
			}
			.alert(
				Text(String(localized: L10n.Transfer.deleteTitle)),
				isPresented: Binding(get: { model.state.isDeleteConfirmationOpen }, set: { if !$0 { Task { @MainActor in model.dismissDeleteTransfer() } } })
			) {
				Button(String(localized: L10n.Button.cancel), role: .cancel, action: model.dismissDeleteTransfer)
				Button(String(localized: L10n.Button.deleteTransfer), role: .destructive, action: model.confirmDeleteTransfer)
			} message: {
				Text(L10n.Transfer.deleteDescription(
					transferName: transfer.transferName ?? String(localized: L10n.Send.newTransferTitle)))
			}
	}

	private var catalog: some View {
		List {
			Section {
				ForEach(outgoing) { transfer in
					Button {
						model.openTransfer(transfer.transferId)
					} label: {
						TransferListItem(
							transfer: transfer,
							thumbnail: model.state.transferThumbnails[transfer.transferId],
							progress: progress(for: transfer)
						)
					}
					.buttonStyle(.plain)
					.contextMenu {
						if transfer.ticket != nil {
							Button {
								shareTarget = transfer
							} label: {
								Label(String(localized: L10n.Transfer.shareTitle), systemSymbol: .squareAndArrowUp)
							}
						}
						if transfer.status == .sharing {
							Button(role: .destructive) {
								model.stopSharing(transferId: transfer.transferId)
							} label: {
								Label(String(localized: L10n.Send.stopSharing), systemSymbol: .stopCircle)
							}
						}
						Divider()
						Button(role: .destructive) {
							deleteTarget = transfer
						} label: {
							Label(String(localized: L10n.Button.deleteTransfer), systemSymbol: .trash)
						}
					}
				}
			} header: {
				Text(String(localized: L10n.Send.transfersTitle))
			} footer: {
				Text(String(localized: L10n.Send.subtitle))
			}
		}
	}

	private var emptyState: some View {
		ContentUnavailableView {
			Label(String(localized: L10n.Send.emptyTitle), systemSymbol: .paperplane)
		} description: {
			Text(String(localized: L10n.Send.emptyBody))
		} actions: {
			Button(action: model.openComposer) {
				Label(String(localized: L10n.Button.createNewTransfer), systemSymbol: .plus)
			}
			.buttonStyle(.borderedProminent)
			.controlSize(.large)
		}
	}

	private func progress(for transfer: Transfer) -> TransferProgress? {
		switch transfer.status {
		case .importing: return progressForTransfer(events: model.coreState.events, transferId: transfer.transferId)
		case .sharing: return sharingProgress(for: transfer)
		default: return nil
		}
	}

	/// Progress for an active share, driven by receivers whose delivery is still
	/// in flight (`.accepted`). Returns nil when none are downloading, so the bar
	/// clears once every receiver has completed even if byte events lag.
	private func sharingProgress(for transfer: Transfer) -> TransferProgress? {
		let active = (model.receiversByTransfer[transfer.transferId] ?? []).filter { $0.status == .accepted }
		if active.isEmpty { return nil }
		let fractions = active.compactMap {
			progressForReceiver(events: model.coreState.events, transferId: transfer.transferId,
								 remoteEndpointId: $0.remoteEndpointId, totalSizeHint: transfer.totalSize)?.progress
		}
		let combined = fractions.isEmpty ? nil : fractions.reduce(0, +) / Double(fractions.count)
		if active.count == 1 {
			return TransferProgress(transferId: transfer.transferId, phase: .transfer, kind: .progress,
									labelKey: L10n.Progress.sending, progress: combined)
		}
		return TransferProgress(transferId: transfer.transferId, phase: .transfer, kind: .progress,
								labelKey: L10n.Progress.sending, progress: combined,
								label: L10n.Progress.sendingToCount(count: active.count))
	}
}

private struct TransferListItem: View {
	let transfer: Transfer
	let thumbnail: Data?
	let progress: TransferProgress?

	var body: some View {
		HStack(spacing: 12) {
			FileArtwork(thumbnail: thumbnail)
				.frame(width: 40, height: 40)
				.background(.quaternary, in: RoundedRectangle(cornerRadius: 9))
			VStack(alignment: .leading, spacing: 3) {
				HStack {
					Text(transfer.transferName ?? String(localized: L10n.Send.newTransferTitle))
						.font(.body).lineLimit(1)
					Spacer()
					StatusPill(label: statusLabel(transfer.status), tone: transfer.status.pillTone)
				}
				Text(L10n.Format.separatedPair(first: formatBytes(transfer.totalSize), second: accessPolicyLabel(transfer.accessPolicy)))
					.font(.caption).foregroundStyle(.secondary).lineLimit(1)
				if let progress, transfer.status == .importing || transfer.status == .sharing {
					ProgressRow(labelKey: progress.labelKey, progress: progress.progress, detail: progress.detail, labelText: progress.label)
						.padding(.top, 2)
				}
			}
			Image(systemSymbol: .chevronForward)
				.font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
		}
		.contentShape(Rectangle())
	}
}

struct FileArtwork: View {
	let thumbnail: Data?

	var body: some View {
		if let thumbnail, let image = PlatformImage.from(data: thumbnail) {
			image.resizable().aspectRatio(contentMode: .fill)
				.clipShape(RoundedRectangle(cornerRadius: 8))
		} else {
			Image(systemSymbol: .doc)
				.font(.system(size: 18))
				.foregroundStyle(.secondary)
		}
	}
}

func statusLabel(_ status: TransferStatus) -> String {
	String(localized: statusLabelKey(status))
}

func accessPolicyLabel(_ policy: ShareAccessPolicy) -> String {
	switch policy {
	case .requireApproval: return String(localized: L10n.Send.accessApproval)
	case .anyoneWithTransfer: return String(localized: L10n.Send.accessAnyone)
	}
}

extension TransferStatus {
	var pillTone: PillTone {
		switch self {
		case .sharing, .done: return .brand
		case .importing, .receiving: return .warning
		case .failed, .cancelled: return .destructive
		case .stopped: return .neutral
		}
	}
}
