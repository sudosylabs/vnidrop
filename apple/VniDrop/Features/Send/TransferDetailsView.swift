import SwiftUI
import SFSafeSymbols
import CoreImage.CIFilterBuiltins

/// Transfer details + drawer panels, ported from `feature/send/TransferDetails.kt`.

struct TransferDetailsView: View {
	@ObservedObject var model: SendModel
	let transfer: Transfer
	let events: [CoreEventModel]
	@State private var showStopConfirmation = false

	private var isActiveShare: Bool {
		transfer.status == .sharing || transfer.status == .importing
	}

	private var pendingReceivers: Int {
		model.state.receiverHistory.filter { $0.status == .requested || $0.status == .accepted }.count
	}
	private var completedReceivers: Int {
		model.state.receiverHistory.filter { $0.status == .completed }.count
	}

	var body: some View {
		Form {
			Section {
				LabeledContent(String(localized: L10n.Metadata.status), value: statusLabel(transfer.status))
				LabeledContent(String(localized: L10n.Metadata.size), value: formatBytes(transfer.totalSize))
				LabeledContent(String(localized: L10n.Send.accessTitle), value: accessPolicyLabel(transfer.accessPolicy))
			} header: {
				Text(transfer.transferName ?? String(localized: L10n.Send.newTransferTitle))
			}

			Section {
				DetailDestination(
					title: String(localized: L10n.Transfer.activityTitle),
					description: String(localized: L10n.Transfer.activityDescription),
					count: events.filter { $0.transferId == transfer.transferId && $0.isMeaningfulActivity }.count,
					onTap: model.openActivity
				)
				DetailDestination(
					title: String(localized: L10n.Transfer.receiversTitle),
					description: receiversDescription(pendingReceivers, completedReceivers),
					count: pendingReceivers + completedReceivers,
					onTap: model.openReceivers
				)
			}

			Section {
				if isActiveShare {
					Button(role: .destructive) {
						showStopConfirmation = true
					} label: {
						Label(String(localized: L10n.Send.stopSharing), systemSymbol: .stopCircle)
					}
				}
				Button(role: .destructive, action: model.requestDeleteTransfer) {
					Label(String(localized: L10n.Button.deleteTransfer), systemSymbol: .trash)
				}
			}
		}
		.formStyle(.grouped)
		.navigationTitle(Text(String(localized: L10n.Send.transferDetailsTitle)))
		#if os(iOS)
		.navigationBarTitleDisplayMode(.inline)
		#endif
		.toolbar {
			ToolbarItem(placement: .primaryAction) {
				Button(action: model.openShare) {
					Label(String(localized: L10n.Transfer.shareTitle), systemSymbol: .squareAndArrowUp)
				}
				.help(String(localized: L10n.Transfer.shareTitle))
			}
		}
		.confirmationDialog(
			Text(String(localized: L10n.Send.stopSharing)),
			isPresented: $showStopConfirmation,
			titleVisibility: .visible
		) {
			Button(String(localized: L10n.Send.stopSharing), role: .destructive) {
				model.stopSharing(transferId: transfer.transferId)
			}
			Button(String(localized: L10n.Button.cancel), role: .cancel) {}
		} message: {
			Text(String(localized: L10n.Send.stopSharingDescription))
		}
	}
}

private func receiversDescription(_ pending: Int, _ completed: Int) -> String {
	if pending > 0 && completed > 0 {
		return L10n.Format.separatedPair(
			first: L10n.Transfer.receiversPending(count: pending),
			second: L10n.Transfer.receiversCompletedCount(count: completed))
	}
	if pending > 0 { return L10n.Transfer.receiversPending(count: pending) }
	if completed > 0 { return L10n.Transfer.receiversCompletedCount(count: completed) }
	return String(localized: L10n.Transfer.receiversDescription)
}

private struct DetailDestination: View {
	let title: String
	let description: String
	let count: Int
	let onTap: () -> Void
	var body: some View {
		Button(action: onTap) {
			HStack {
				VStack(alignment: .leading, spacing: 3) {
					Text(title).foregroundStyle(.primary)
					Text(description).font(.caption).foregroundStyle(.secondary)
				}
				Spacer()
				if count > 0 {
					Text(verbatim: "\(count)")
						.font(.footnote)
						.foregroundStyle(.secondary)
				}
				Image(systemSymbol: .chevronForward)
					.font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
			}
			.contentShape(Rectangle())
		}
		.buttonStyle(.plain)
	}
}

// MARK: - Detail panels

struct DetailPanelContent: View {
	@ObservedObject var model: SendModel
	let transfer: Transfer
	let panel: TransferDetailPanel

	var body: some View {
		switch panel {
		case .activity:
			TransferActivityPanel(events: model.coreState.events, transferId: transfer.transferId)
		case .receivers:
			ReceiverHistoryPanel(
				receivers: model.state.receiverHistory,
				loading: model.state.isLoadingReceivers,
				events: model.coreState.events,
				transferTotalSize: transfer.totalSize,
				onCancel: model.cancelReceiver,
				onAccept: model.acceptReceiver
			)
		case .share:
			TransferSharePanel(model: model, transfer: transfer)
		}
	}
}

private struct PanelContainer<Content: View>: View {
	let title: String
	@ViewBuilder let content: () -> Content
	var body: some View {
		VStack(alignment: .leading, spacing: 14) {
			Text(title).font(VniType.titleLarge)
			content()
		}
		.padding(.horizontal, 20).padding(.vertical, 14)
		.frame(maxWidth: .infinity, alignment: .leading)
	}
}

struct TransferActivityPanel: View {
	@Environment(\.vniColors) private var colors
	let events: [CoreEventModel]
	let transferId: UInt64

	var body: some View {
		let visible = events
			.filter { $0.transferId == transferId && $0.isMeaningfulActivity }
			.sorted { $0.timestamp > $1.timestamp }
		PanelContainer(title: String(localized: L10n.Transfer.activityTitle)) {
			if visible.isEmpty {
				Text(String(localized: L10n.Transfer.noActivity)).foregroundStyle(colors.foregroundLighter)
			} else {
				ForEach(Array(visible.enumerated()), id: \.offset) { index, event in
					if index > 0 { Divider().overlay(colors.borderDefault) }
					Text(String(localized: event.activityTitleKey))
						.fontWeight(.medium).padding(.vertical, 14)
				}
			}
		}
	}
}

struct ReceiverHistoryPanel: View {
	@Environment(\.vniColors) private var colors
	let receivers: [ReceiverRequestModel]
	let loading: Bool
	let events: [CoreEventModel]
	let transferTotalSize: UInt64
	let onCancel: (String) -> Void
	let onAccept: (String) -> Void

	var body: some View {
		PanelContainer(title: String(localized: L10n.Transfer.receiversTitle)) {
			if loading {
				ProgressView().frame(maxWidth: .infinity).padding(40)
			} else if receivers.isEmpty {
				Text(String(localized: L10n.Transfer.noReceivers)).foregroundStyle(colors.foregroundLighter)
			} else {
				ForEach(Array(receivers.enumerated()), id: \.element.id) { index, receiver in
					if index > 0 { Divider().overlay(colors.borderDefault) }
					ReceiverRow(
						receiver: receiver,
						sendProgress: sendProgress(for: receiver),
						onCancel: onCancel,
						onAccept: onAccept
					)
				}
			}
		}
	}

	private func sendProgress(for receiver: ReceiverRequestModel) -> TransferProgress? {
		switch receiver.status {
		case .accepted, .requested:
			return progressForReceiver(events: events, transferId: receiver.transferId,
									   remoteEndpointId: receiver.remoteEndpointId, totalSizeHint: transferTotalSize)
		default: return nil
		}
	}
}

private struct ReceiverRow: View {
	@Environment(\.vniColors) private var colors
	let receiver: ReceiverRequestModel
	let sendProgress: TransferProgress?
	let onCancel: (String) -> Void
	let onAccept: (String) -> Void

	/// Only pending requests can be cancelled per-receiver: the core rejects a
	/// negative response to an already-accepted request ("...not approved, or it
	/// was refused"). Interrupting an in-flight receiver needs Stop sharing.
	private var isCancelable: Bool {
		receiver.status == .requested
	}

	var body: some View {
		let name = receiver.receiverName ?? receiver.receiverDeviceName ?? String(localized: L10n.Transfer.nearbyDevice)
		let showLive = sendProgress != nil && receiver.status != .completed
			&& receiver.status != .refused && receiver.status != .expired
			&& receiver.status != .failed
		HStack(alignment: .top, spacing: 12) {
			VStack(alignment: .leading, spacing: 6) {
				Text(name).font(VniType.bodyLarge).lineLimit(1)
				if let deviceName = receiver.receiverDeviceName, deviceName != name {
					Text(deviceName).font(VniType.bodySmall).foregroundStyle(colors.foregroundLighter)
				}
				if showLive, let sendProgress {
					ProgressRow(labelKey: sendProgress.labelKey, progress: sendProgress.progress, detail: sendProgress.detail, labelText: sendProgress.label)
				} else {
					Text(String(localized: receiver.status.statusTextKey))
						.font(VniType.bodySmall).fontWeight(.medium)
						.foregroundStyle(receiver.status.statusColor(colors))
				}
				if let reason = receiver.reason, !reason.isEmpty {
					Text(receiverReasonUiText(reason).resolved())
						.font(VniType.bodySmall).foregroundStyle(colors.foregroundLighter)
				}
			}
			.frame(maxWidth: .infinity, alignment: .leading)
			if isCancelable {
				VStack(alignment: .trailing, spacing: 8) {
					Button(role: .destructive) {
						onCancel(receiver.id)
					} label: {
						Text(String(localized: L10n.Button.refuse))
							.font(VniType.bodySmall)
					}
					.buttonStyle(.borderless)
					.tint(.red)
					// Fallback approve action, in case the approval modal didn't surface.
					Button {
						onAccept(receiver.id)
					} label: {
						Text(String(localized: L10n.Button.approve))
							.font(VniType.bodySmall).fontWeight(.medium)
							.foregroundStyle(.white)
							.padding(.horizontal, 16).padding(.vertical, 7)
							.background(Color.green, in: Capsule())
					}
					.buttonStyle(.plain)
				}
			}
		}
		.frame(maxWidth: .infinity, alignment: .leading)
		.padding(.vertical, 13)
	}
}

struct TransferSharePanel: View {
	@Environment(\.vniColors) private var colors
	@ObservedObject var model: SendModel
	let transfer: Transfer

	var body: some View {
		PanelContainer(title: String(localized: L10n.Transfer.shareTitle)) {
			switch transfer.invitationPresentation {
			case .ready(let ticket):
				let qrImage = QRCode.generate(from: ticket)
				qrCard(image: qrImage)
				if qrImage != nil {
					Text(String(localized: L10n.Transfer.scanQr))
						.font(VniType.bodySmall).foregroundStyle(colors.foregroundLighter)
						.frame(maxWidth: .infinity)
				}
				ShareActionsView(model: model, transfer: transfer, ticket: ticket)
			case .preparing:
				Text(String(localized: L10n.Transfer.eventPreparing)).foregroundStyle(colors.foregroundLighter)
			case .unavailable:
				Text(String(localized: transfer.status == .failed ? L10n.Transfer.eventFailed : L10n.Transfer.eventStopped))
					.foregroundStyle(colors.foregroundLighter)
			}
		}
	}

	private func qrCard(image: Image?) -> some View {
		ZStack {
			if let image {
				image.interpolation(.none).resizable().scaledToFit().padding(14)
			} else {
				VStack(spacing: 10) {
					Image(systemSymbol: .qrcode)
						.font(.system(size: 36, weight: .medium))
					Text(String(localized: L10n.Transfer.qrUnavailable))
						.font(VniType.bodySmall)
						.multilineTextAlignment(.center)
				}
				.foregroundStyle(.black.opacity(0.72))
				.padding(22)
			}
		}
		.frame(width: 268, height: 268)
		.background(Color.white, in: RoundedRectangle(cornerRadius: 18))
		.frame(maxWidth: .infinity)
	}
}

enum TransferInvitationPresentation: Equatable {
	case preparing
	case ready(String)
	case unavailable
}

extension Transfer {
	var invitationPresentation: TransferInvitationPresentation {
		switch status {
		case .importing:
			return .preparing
		case .sharing:
			guard let ticket, !ticket.isEmpty else { return .preparing }
			return .ready(ticket)
		case .receiving, .done, .failed, .cancelled, .stopped:
			return .unavailable
		}
	}
}

// MARK: - QR generation (CoreImage)

enum QRCode {
	static func generate(from string: String) -> Image? {
		let context = CIContext()
		let filter = CIFilter.qrCodeGenerator()
		filter.message = Data(string.utf8)
		filter.correctionLevel = "M"
		guard let output = filter.outputImage else { return nil }
		let scaled = output.transformed(by: CGAffineTransform(scaleX: 10, y: 10))
		guard let cgImage = context.createCGImage(scaled, from: scaled.extent) else { return nil }
		#if os(iOS)
		return Image(uiImage: UIImage(cgImage: cgImage))
		#else
		return Image(nsImage: NSImage(cgImage: cgImage, size: .zero))
		#endif
	}
}

// MARK: - Event helpers (ported from TransferDetails.kt)

extension CoreEventModel {
	var isMeaningfulActivity: Bool {
		(phase == "import" && kind == "started")
			|| (phase == "ticket" && kind == "created")
			|| (phase == "network" && (kind == "connecting" || kind == "connected"))
			|| (phase == "download" && kind == "found-collection")
			|| (phase == "lifecycle" && ["done", "cancelled", "share-stopped"].contains(kind))
			|| ["receiver-requested", "receiver-accepted", "receiver-auto-approved",
				"receiver-refused", "receiver-completed", "share-stopped", "failed"].contains(kind)
	}

	var activityTitleKey: String.LocalizationValue {
		if phase == "import" && kind == "started" { return L10n.Transfer.eventPreparing }
		if phase == "ticket" && kind == "created" { return L10n.Transfer.eventReady }
		if phase == "network" { return L10n.Transfer.eventConnecting }
		if phase == "download" { return L10n.Transfer.eventDownloading }
		if phase == "export" { return L10n.Transfer.eventSaving }
		if kind == "receiver-requested" { return L10n.Transfer.eventRequested }
		if kind == "receiver-accepted" || kind == "receiver-auto-approved" { return L10n.Transfer.eventApproved }
		if kind == "receiver-refused" { return L10n.Transfer.eventRefused }
		if kind == "receiver-completed" { return L10n.Transfer.eventCompleted }
		if kind == "share-stopped" || (phase == "lifecycle" && kind == "cancelled") { return L10n.Transfer.eventStopped }
		if kind == "failed" { return L10n.Transfer.eventFailed }
		return L10n.Transfer.eventUpdated
	}
}

extension ReceiverDeliveryStatus {
	var statusTextKey: String.LocalizationValue {
		switch self {
		case .requested: return L10n.Transfer.receiverRequested
		case .accepted: return L10n.Transfer.receiverAccepted
		case .refused: return L10n.Transfer.receiverRefused
		case .expired: return L10n.Transfer.receiverExpired
		case .completed: return L10n.Transfer.receiverCompleted
		case .failed: return L10n.Transfer.receiverFailed
		case .unknown: return L10n.Transfer.receiverUnknown
		}
	}

	func statusColor(_ colors: VniDropColors) -> Color {
		switch self {
		case .completed: return colors.brandDefault
		case .refused, .expired, .failed: return colors.destructiveDefault
		default: return colors.foregroundLighter
		}
	}
}

#if os(iOS)
import UIKit
#else
import AppKit
#endif
