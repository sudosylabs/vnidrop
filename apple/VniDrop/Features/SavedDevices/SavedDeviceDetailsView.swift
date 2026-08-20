import SwiftUI
import SFSafeSymbols

/// Per-device details: Send, label/forget/block, and this device's targeted
/// transfers with their lifecycle actions. Presented as a sheet on compact
/// layouts and as a native inspector on macOS.
struct SavedDeviceDetailsView: View {
	@ObservedObject var model: SavedDevicesModel
	let peerEndpointId: String
	let windowClass: WindowClass
	let onClose: () -> Void

	/// Which destructive action is awaiting confirmation.
	@State private var confirming: DestructiveAction?

	private enum DestructiveAction: String, Identifiable {
		case forget
		case block
		var id: String { rawValue }
	}

	private var state: SavedDevicesState { model.state }
	private var device: SavedDeviceModel? { state.device(peerEndpointId) }
	private var busy: Bool { state.busyPeerIds.contains(peerEndpointId) }
	private var transfers: [SavedDeviceTransferItem] { state.transfers(for: peerEndpointId) }

	var body: some View {
		Group {
			if let device {
				content(device)
			} else {
				// The device can disappear underneath us — forget, block, or a peer
				// reinstall all remove it. Close rather than render a stale identity.
				Color.clear.onAppear(perform: onClose)
			}
		}
		.confirmationDialog(
			Text(String(localized: confirming == .block
				? L10n.Saved.devicesBlockConfirmTitle
				: L10n.Saved.devicesForgetConfirmTitle)),
			isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } }),
			titleVisibility: .visible
		) {
			Button(String(localized: L10n.Button.cancel), role: .cancel) { confirming = nil }
			Button(
				String(localized: confirming == .block
					? L10n.Saved.devicesBlockAction
					: L10n.Saved.devicesForgetAction),
				role: .destructive
			) {
				let action = confirming
				confirming = nil
				switch action {
				case .forget: model.forget(peerEndpointId)
				case .block: model.block(peerEndpointId)
				case nil: break
				}
			}
		} message: {
			let name = device?.displayName ?? String(localized: L10n.Saved.devicesUnnamed)
			Text(confirming == .block
				? L10n.Saved.devicesBlockConfirmBody(device: name)
				: L10n.Saved.devicesForgetConfirmBody(device: name))
		}
		.sheet(isPresented: Binding(
			get: { state.labelingPeerId == peerEndpointId },
			set: { if !$0 { model.dismissLabelEditor() } }
		)) {
			LabelEditorSheet(model: model, windowClass: windowClass)
		}
		.sheet(isPresented: Binding(
			get: { state.sendTargetPeerId == peerEndpointId },
			set: { if !$0 { model.cancelSend() } }
		)) {
			TargetedSendSheet(
				model: model,
				windowClass: windowClass,
				deviceName: device?.displayName ?? String(localized: L10n.Saved.devicesUnnamed)
			)
		}
	}

	@ViewBuilder
	private func content(_ device: SavedDeviceModel) -> some View {
		List {
			Section { DeviceHeader(device: device) }

			Section {
				Button {
					model.beginSend(to: peerEndpointId)
				} label: {
					ActionRow(
						symbol: .paperplaneFill,
						title: String(localized: L10n.Saved.devicesSendAction),
						tint: VniDropColors.brandPurple
					)
				}
				.disabled(busy)

				Button {
					model.openLabelEditor(peerEndpointId)
				} label: {
					ActionRow(
						symbol: .pencil,
						title: String(localized: L10n.Saved.devicesLabelAction),
						// Showing the current label makes it clear this renames
						// locally rather than changing what the peer calls itself.
						detail: device.localLabel?.isEmpty == false ? device.localLabel : nil
					)
				}
				.disabled(busy)
			}

			Section {
				if transfers.isEmpty {
					Text(String(localized: L10n.Saved.devicesTransferEmpty))
						.font(.subheadline)
						.foregroundStyle(.secondary)
						.padding(.vertical, 2)
				} else {
					ForEach(transfers) { transfer in
						TargetedTransferRow(
							transfer: transfer,
							busy: state.busyTransferIds.contains(transfer.id),
							onReceive: { model.receiveTargetedTransfer(transfer.id) },
							onResume: { model.resumeTargetedTransfer(transfer.id) },
							onCancel: { model.cancelTargetedTransfer(transfer.id) },
							onDelete: { model.deleteTargetedTransfer(transfer.id) }
						)
					}
				}
			} header: {
				Text(String(localized: L10n.Saved.devicesTransfersTitle))
			}

			Section {
				Button(role: .destructive) {
					confirming = .forget
				} label: {
					ActionRow(
						symbol: .trash,
						title: String(localized: L10n.Saved.devicesForgetAction),
						tint: .red
					)
				}
				.disabled(busy)

				Button(role: .destructive) {
					confirming = .block
				} label: {
					ActionRow(
						symbol: .nosign,
						title: String(localized: L10n.Saved.devicesBlockAction),
						tint: .red
					)
				}
				.disabled(busy)
			} header: {
				Text(L10n.Saved.devicesMoreActions(device: device.displayName))
			}
		}
		#if os(macOS)
		.listStyle(.inset)
		#else
		.listStyle(.insetGrouped)
		#endif
		.buttonStyle(.plain)
		.overlay(alignment: .top) {
			if busy { ProgressView().controlSize(.small).padding(.top, 6) }
		}
	}
}

// MARK: - Pieces

private struct DeviceHeader: View {
	let device: SavedDeviceModel

	var body: some View {
		VStack(alignment: .leading, spacing: 10) {
			HStack(spacing: 14) {
				DeviceAvatar(size: 52)
				VStack(alignment: .leading, spacing: 3) {
					Text(device.displayName)
						.font(.title3)
						.fontWeight(.semibold)
						.lineLimit(2)
					// The authenticated peer-supplied name, shown only when a local
					// label overrides it, so the user can tell the two apart.
					if device.localLabel?.isEmpty == false, let remote = device.remoteDisplayName {
						Text(L10n.Saved.devicesAuthenticatedName(name: remote))
							.font(.caption)
							.foregroundStyle(.secondary)
							.lineLimit(1)
					}
				}
				Spacer(minLength: 0)
			}
			EndpointIdLabel(endpointId: device.endpointId)
		}
		.padding(.vertical, 6)
	}
}

/// A tappable row that reads as a native list action rather than bare tinted text.
private struct ActionRow: View {
	let symbol: SFSymbol
	let title: String
	var detail: String? = nil
	var tint: Color = .primary

	var body: some View {
		HStack(spacing: 12) {
			Image(systemSymbol: symbol)
				.font(.system(size: 15))
				.foregroundStyle(tint == .primary ? AnyShapeStyle(.secondary) : AnyShapeStyle(tint))
				.frame(width: 22)
			Text(title).foregroundStyle(tint)
			Spacer(minLength: 8)
			if let detail {
				Text(detail)
					.font(.callout)
					.foregroundStyle(.secondary)
					.lineLimit(1)
					.truncationMode(.tail)
			}
		}
		.contentShape(Rectangle())
		.padding(.vertical, 2)
	}
}

/// One targeted transfer. The action its state actually calls for is a visible
/// button; everything else lives behind an overflow menu so a list of transfers
/// does not turn into a wall of buttons.
private struct TargetedTransferRow: View {
	let transfer: SavedDeviceTransferItem
	let busy: Bool
	let onReceive: () -> Void
	let onResume: () -> Void
	let onCancel: () -> Void
	let onDelete: () -> Void

	var body: some View {
		VStack(alignment: .leading, spacing: 6) {
			HStack(spacing: 8) {
				Image(systemSymbol: transfer.direction == .outgoing ? .arrowUpCircle : .arrowDownCircle)
					.font(.callout)
					.foregroundStyle(.secondary)
				Text(name)
					.font(.body)
					.lineLimit(1)
					.truncationMode(.middle)
				Spacer(minLength: 8)
				if busy {
					ProgressView().controlSize(.small)
				} else {
					StatusPill(label: transfer.state.label, tone: transfer.state.tone)
				}
			}

			HStack(spacing: 8) {
				Text(L10n.Saved.devicesTransferFiles(
					count: "\(transfer.fileCount)",
					size: formatBytes(transfer.totalSize)
				))
				.font(.caption)
				.foregroundStyle(.secondary)

				Spacer(minLength: 0)

				if let primary = primaryAction {
					Button(String(localized: primary.title), action: primary.run)
						.buttonStyle(.borderedProminent)
						.controlSize(.small)
						.disabled(busy)
				}
				if !overflowActions.isEmpty {
					Menu {
						ForEach(overflowActions, id: \.id) { action in
							Button(String(localized: action.title), role: .destructive, action: action.run)
						}
					} label: {
						Image(systemSymbol: .ellipsisCircle)
					}
					.menuStyle(.borderlessButton)
					.menuIndicator(.hidden)
					.fixedSize()
					.disabled(busy)
				}
			}

			if let progress = transfer.progressFraction {
				ProgressView(value: progress)
				Text(L10n.Saved.devicesTransferProgress(
					verified: formatBytes(transfer.verifiedBytes),
					total: formatBytes(transfer.totalSize)
				))
				.font(.caption2)
				.foregroundStyle(.secondary)
			}
		}
		.padding(.vertical, 4)
	}

	private var name: String {
		transfer.transferName.isEmpty
			? String(localized: L10n.Saved.devicesUnnamed)
			: transfer.transferName
	}

	private struct TransferAction {
		let id: String
		let title: String.LocalizationValue
		let run: () -> Void
	}

	/// At most one of receive/resume applies: one state each, receiving side only.
	private var primaryAction: TransferAction? {
		if transfer.availableActions.contains(.receive) {
			return TransferAction(id: "receive", title: L10n.Saved.devicesTransferReceive, run: onReceive)
		}
		if transfer.availableActions.contains(.resume) {
			return TransferAction(id: "resume", title: L10n.Saved.devicesTransferResume, run: onResume)
		}
		return nil
	}

	private var overflowActions: [TransferAction] {
		var actions: [TransferAction] = []
		if transfer.availableActions.contains(.cancel) {
			actions.append(TransferAction(id: "cancel", title: L10n.Saved.devicesTransferCancel, run: onCancel))
		}
		if transfer.availableActions.contains(.delete) {
			actions.append(TransferAction(id: "delete", title: L10n.Saved.devicesTransferDelete, run: onDelete))
		}
		return actions
	}
}

// MARK: - Sheets

/// Label editor. Stays open and keeps its draft when the write fails, and blocks
/// its own dismissal while saving, so the retry path always survives.
///
/// Split per platform on purpose: macOS gets a compact dialog with one trailing
/// button row, iOS gets the native Cancel/Save toolbar. A single shared layout
/// produced two competing button rows and a lot of dead space.
private struct LabelEditorSheet: View {
	@ObservedObject var model: SavedDevicesModel
	let windowClass: WindowClass

	private var isSaving: Bool { model.state.isSavingLabel }

	/// The peer's own authenticated name, shown so it is obvious the label is a
	/// local override rather than a rename on the other device.
	private var remoteName: String? {
		guard let peerId = model.state.labelingPeerId else { return nil }
		guard let remote = model.state.device(peerId)?.remoteDisplayName, !remote.isEmpty else {
			return nil
		}
		return remote
	}

	private var field: some View {
		TextField(
			String(localized: L10n.Saved.devicesLabelPlaceholder),
			text: Binding(
				get: { model.state.labelDraft },
				set: { model.setLabelDraft($0) }
			)
		)
		.textFieldStyle(.roundedBorder)
		.labelsHidden()
		.disabled(isSaving)
		.onSubmit { if model.state.canSaveLabel { model.saveLabel() } }
	}

	var body: some View {
		#if os(macOS)
		VStack(alignment: .leading, spacing: 14) {
			VStack(alignment: .leading, spacing: 4) {
				Text(String(localized: L10n.Saved.devicesLabelTitle))
					.font(.headline)
				if let remoteName {
					Text(L10n.Saved.devicesAuthenticatedName(name: remoteName))
						.font(.subheadline)
						.foregroundStyle(.secondary)
						.lineLimit(1)
						.truncationMode(.middle)
				}
			}
			field
			HStack(spacing: 10) {
				Button(String(localized: L10n.Saved.devicesLabelClear), role: .destructive) {
					model.clearLabel()
				}
				.buttonStyle(.link)
				// Nothing to clear when the device has no label yet.
				.disabled(isSaving || !model.state.hasExistingLabel)
				Spacer(minLength: 12)
				if isSaving { ProgressView().controlSize(.small) }
				Button(String(localized: L10n.Button.cancel), action: model.dismissLabelEditor)
					.keyboardShortcut(.cancelAction)
					.disabled(isSaving)
				Button(String(localized: L10n.Saved.devicesLabelSave), action: model.saveLabel)
					.buttonStyle(.borderedProminent)
					.keyboardShortcut(.defaultAction)
					.disabled(isSaving || !model.state.canSaveLabel)
			}
		}
		.padding(20)
		.frame(width: 420)
		.interactiveDismissDisabled(isSaving)
		#else
		NavigationStack {
			Form {
				Section {
					field
				} footer: {
					if let remoteName {
						Text(L10n.Saved.devicesAuthenticatedName(name: remoteName))
					}
				}
				Section {
					Button(String(localized: L10n.Saved.devicesLabelClear), role: .destructive) {
						model.clearLabel()
					}
					.disabled(isSaving || !model.state.hasExistingLabel)
				}
			}
			.navigationTitle(Text(String(localized: L10n.Saved.devicesLabelTitle)))
			.navigationBarTitleDisplayMode(.inline)
			.toolbar {
				ToolbarItem(placement: .cancellationAction) {
					Button(String(localized: L10n.Button.cancel), action: model.dismissLabelEditor)
						.disabled(isSaving)
				}
				ToolbarItem(placement: .confirmationAction) {
					if isSaving {
						ProgressView().controlSize(.small)
					} else {
						Button(String(localized: L10n.Saved.devicesLabelSave), action: model.saveLabel)
							.disabled(!model.state.canSaveLabel)
					}
				}
			}
		}
		.sheetSize(windowClass: windowClass, minWidth: 380, minHeight: 240)
		.interactiveDismissDisabled(isSaving)
		#endif
	}
}

/// Hosts the targeted-send composer.
private struct TargetedSendSheet: View {
	@ObservedObject var model: SavedDevicesModel
	let windowClass: WindowClass
	let deviceName: String

	var body: some View {
		NavigationStack {
			ScrollView {
				TargetedSendComposer(model: model, deviceName: deviceName)
			}
			#if os(iOS)
			.navigationBarTitleDisplayMode(.inline)
			#endif
			.toolbar {
				ToolbarItem(placement: .cancellationAction) {
					// Never disabled: a create in flight can take minutes against an
					// unavailable device, and a dead Close left no way out at all.
					// It reads "Cancel" while waiting, because that is what leaving
					// now does — abandoning keeps the sources alive for the core's
					// import and cleans them up once the call lands.
					Button(String(localized: model.state.isCreatingSend
						? L10n.Button.cancel
						: L10n.Button.close)) {
						if model.state.isCreatingSend {
							model.abandonSend()
						} else {
							model.cancelSend()
						}
					}
				}
			}
		}
		.sheetSize(windowClass: windowClass, minWidth: 460, minHeight: 480)
	}
}

private extension View {
	/// Sizes a sheet for its host. A phone must never get a `minWidth` — forcing
	/// one wider than the screen pushes the content out of bounds instead of
	/// growing the sheet, which is exactly what a fixed 460pt did.
	@ViewBuilder
	func sheetSize(windowClass: WindowClass, minWidth: CGFloat, minHeight: CGFloat) -> some View {
		if windowClass == .phone {
			self
				.presentationDetents([.medium, .large])
				.presentationDragIndicator(.visible)
		} else {
			frame(minWidth: minWidth, minHeight: minHeight)
		}
	}
}

// MARK: - Adaptive presentation

extension View {
	/// Presents the details surface the way each platform expects: a sheet with
	/// detents on compact layouts, a native inspector on macOS.
	func savedDeviceDetails(
		model: SavedDevicesModel,
		windowClass: WindowClass,
		selectedPeerId: Binding<String?>
	) -> some View {
		modifier(SavedDeviceDetailsPresentation(
			model: model, windowClass: windowClass, selectedPeerId: selectedPeerId
		))
	}
}

private struct SavedDeviceDetailsPresentation: ViewModifier {
	@ObservedObject var model: SavedDevicesModel
	let windowClass: WindowClass
	@Binding var selectedPeerId: String?

	func body(content: Content) -> some View {
		#if os(macOS)
		content.inspector(isPresented: Binding(
			get: { selectedPeerId != nil },
			set: { if !$0 { selectedPeerId = nil } }
		)) {
			detail
				.inspectorColumnWidth(min: 300, ideal: 360, max: 480)
				.toolbar {
					ToolbarItem(placement: .primaryAction) {
						Button {
							selectedPeerId = nil
						} label: {
							Label(String(localized: L10n.Button.close), systemSymbol: .sidebarRight)
						}
					}
				}
		}
		#else
		content.sheet(isPresented: Binding(
			get: { selectedPeerId != nil },
			set: { if !$0 { selectedPeerId = nil } }
		)) {
			NavigationStack {
				detail
					.navigationBarTitleDisplayMode(.inline)
					.toolbar {
						ToolbarItem(placement: .cancellationAction) {
							Button(String(localized: L10n.Button.close)) { selectedPeerId = nil }
						}
					}
			}
			.sheetSize(windowClass: windowClass, minWidth: 460, minHeight: 520)
		}
		#endif
	}

	@ViewBuilder
	private var detail: some View {
		if let peerId = selectedPeerId {
			SavedDeviceDetailsView(
				model: model,
				peerEndpointId: peerId,
				windowClass: windowClass,
				onClose: { selectedPeerId = nil }
			)
		}
	}
}
