import SwiftUI
import SFSafeSymbols

/// Source composition for a targeted send. Deliberately mirrors the invitation
/// composer's file/folder/rename/replace/clear affordances — the two domains stay
/// distinct after creation, but choosing what to send works the same way.
struct TargetedSendComposer: View {
	@ObservedObject var model: SavedDevicesModel
	let deviceName: String

	private var state: SavedDevicesState { model.state }

	var body: some View {
		VStack(alignment: .leading, spacing: 16) {
			if state.sendFiles.isEmpty {
				chooseStep
			} else {
				reviewStep
			}
		}
		.padding(.horizontal, 20)
		.padding(.vertical, 12)
		.frame(maxWidth: .infinity, alignment: .leading)
		.savedDeviceSendPickers(model: model)
	}

	private var chooseStep: some View {
		VStack(alignment: .leading, spacing: 16) {
			Text(String(localized: L10n.Send.chooseFileTitle))
				.font(.title2)
				.fontWeight(.semibold)
			Text(recipientLine)
				.font(.subheadline)
				.foregroundStyle(.secondary)
			VStack(spacing: 14) {
				Image(systemSymbol: .doc)
					.font(.system(size: 30))
					.foregroundStyle(.tint)
				PrimaryButton(
					title: String(localized: L10n.Button.chooseFiles),
					action: model.selectSendFiles
				)
				.fixedSize()
				QuietButton(
					title: String(localized: L10n.Button.chooseFolder),
					action: model.selectSendFolder
				)
			}
			.frame(maxWidth: .infinity)
			.padding(28)
			.background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 16))
		}
	}

	private var reviewStep: some View {
		VStack(alignment: .leading, spacing: 16) {
			Text(String(localized: L10n.Send.reviewTitle))
				.font(.title2)
				.fontWeight(.semibold)
			Text(recipientLine)
				.font(.subheadline)
				.foregroundStyle(.secondary)

			ForEach(state.sendFiles) { file in
				TargetedSourceCard(
					file: file,
					canRemove: state.sendFiles.count > 1 && !state.isCreatingSend,
					onRemove: { model.removeSendFile(file.value) }
				)
			}

			Field(
				label: String(localized: L10n.Field.transferName),
				value: Binding(
					get: { state.sendTransferName },
					set: { model.setSendTransferName($0) }
				),
				enabled: !state.isCreatingSend
			)

			// Every targeted transfer still needs the receiver to approve it;
			// saying so here sets the right expectation before sending.
			Label(
				String(localized: L10n.Send.accessApprovalDescription),
				systemSymbol: .checkmarkShield
			)
			.font(.caption)
			.foregroundStyle(.secondary)

			actions
		}
	}

	private var actions: some View {
		VStack(spacing: 10) {
			if state.isCreatingSend {
				// Reaching an unavailable device can take minutes, so the wait says
				// so. Abandoning it lives on the toolbar's cancel item alone — a
				// second button here would be the same action under a second name.
				HStack(spacing: 10) {
					ProgressView().controlSize(.small)
					Text(String(localized: L10n.Saved.devicesSendWaiting))
						.font(.subheadline)
						.foregroundStyle(.secondary)
				}
				.frame(maxWidth: .infinity)
				.padding(.vertical, 10)
			} else {
				PrimaryButton(
					title: String(localized: L10n.Saved.devicesSendAction),
					action: model.createTargetedTransfer,
					enabled: state.canCreateTargetedTransfer
				)
			}
			HStack(spacing: 10) {
				sourceButton(
					title: L10n.Button.changeFiles, symbol: .docBadgeArrowUp, action: model.selectSendFiles
				)
				sourceButton(
					title: L10n.Button.chooseFolder, symbol: .folder, action: model.selectSendFolder
				)
				sourceButton(title: L10n.Button.clear, symbol: .xmark, action: model.clearSendFiles)
			}
		}
	}

	private func sourceButton(
		title: String.LocalizationValue,
		symbol: SFSymbol,
		action: @escaping () -> Void
	) -> some View {
		Button(action: action) {
			Label(String(localized: title), systemSymbol: symbol)
				.lineLimit(1)
				.minimumScaleFactor(0.85)
				.frame(maxWidth: .infinity)
				.frame(minHeight: 20)
		}
		.buttonStyle(.bordered)
		.controlSize(.large)
		.tint(.secondary)
		.disabled(state.isCreatingSend)
	}

	private var recipientLine: String {
		L10n.Saved.devicesTransferDirectionOutgoing(device: deviceName)
	}
}

private struct TargetedSourceCard: View {
	let file: PickedShareFile
	let canRemove: Bool
	let onRemove: () -> Void

	var body: some View {
		HStack(spacing: 12) {
			Image(systemSymbol: file.isDirectory ? .folder : .doc)
				.foregroundStyle(.secondary)
				.frame(width: 44, height: 44)
				.background(.quaternary, in: RoundedRectangle(cornerRadius: 10))
			VStack(alignment: .leading, spacing: 3) {
				Text(file.displayName).lineLimit(1)
				Text(subtitle).font(.caption).foregroundStyle(.secondary)
			}
			Spacer()
			if canRemove {
				Button(role: .destructive, action: onRemove) {
					Image(systemSymbol: .trash)
				}
				.buttonStyle(.borderless)
				.tint(.red)
			}
		}
		.padding(14)
		.frame(maxWidth: .infinity)
		.background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 14))
	}

	private var subtitle: String {
		if file.isDirectory { return String(localized: L10n.Send.folderLabel) }
		if let size = file.sizeBytes { return formatBytes(size) }
		return String(localized: L10n.Send.fileSizeUnknown)
	}
}

/// File/folder picker for targeted send. Attached to the composer so it presents
/// from the composer's own sheet rather than the already-presenting root — the
/// same constraint `SendPickers` documents.
private struct SavedDeviceSendPickers: ViewModifier {
	@ObservedObject var model: SavedDevicesModel

	func body(content: Content) -> some View {
		// One .fileImporter switched between files and folders: stacking two on a
		// single view silently breaks, with the second shadowing the first.
		content.fileImporter(
			isPresented: Binding(
				get: { model.pendingFilePick || model.pendingFolderPick },
				set: { presented in
					if !presented {
						model.pendingFilePick = false
						model.pendingFolderPick = false
					}
				}
			),
			allowedContentTypes: model.pendingFolderPick ? [.folder] : [.item],
			allowsMultipleSelection: !model.pendingFolderPick
		) { result in
			let isDirectory = model.pendingFolderPick
			model.pendingFilePick = false
			model.pendingFolderPick = false
			switch result {
			case .success(let urls):
				let files = urls.compactMap { PickerSupport.pickedFile(from: $0, isDirectory: isDirectory) }
				if files.isEmpty {
					model.onSendFilePickFailed("")
				} else {
					model.onSendFilesPicked(files)
				}
			case .failure(let error):
				if !error.isUserCancellation { model.onSendFilePickFailed(error.technicalDetail) }
			}
		}
	}
}

extension View {
	func savedDeviceSendPickers(model: SavedDevicesModel) -> some View {
		modifier(SavedDeviceSendPickers(model: model))
	}
}
