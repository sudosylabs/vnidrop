import SwiftUI
import SFSafeSymbols

/// Transfer composer drawer, ported from `feature/send/TransferComposer.kt`.
/// Two steps: choose files/folder, then review + name + access policy + share.
struct TransferComposer: View {
	@ObservedObject var model: SendModel
	let windowClass: WindowClass

	private var state: SendState { model.state }

	var body: some View {
		VStack(alignment: .leading, spacing: 16) {
			if state.selectedFiles.isEmpty {
				chooseStep
			} else {
				reviewStep
			}
		}
		.padding(.horizontal, 20).padding(.vertical, 12)
		.frame(maxWidth: .infinity, alignment: .leading)
		.sendPickers(model: model)
	}

	private var chooseStep: some View {
		VStack(alignment: .leading, spacing: 16) {
			Text(String(localized: L10n.Send.chooseFileTitle)).font(.title2).fontWeight(.semibold)
			Text(String(localized: L10n.Send.chooseFileBody))
				.font(.subheadline).foregroundStyle(.secondary)
			VStack(spacing: 14) {
				Image(systemSymbol: .doc).font(.system(size: 30)).foregroundStyle(.tint)
				PrimaryButton(title: String(localized: L10n.Button.chooseFiles), action: model.selectFile).fixedSize()
				QuietButton(title: String(localized: L10n.Button.chooseFolder), action: model.selectFolder)
			}
			.frame(maxWidth: .infinity)
			.padding(28)
			.background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 16))
		}
	}

	private var reviewStep: some View {
		VStack(alignment: .leading, spacing: 16) {
			Text(String(localized: L10n.Send.reviewTitle)).font(.title2).fontWeight(.semibold)
			if state.selectedFiles.count > 1 {
				Text(L10n.Send.selectedFilesCount(count: state.selectedFiles.count))
					.font(.subheadline).foregroundStyle(.secondary)
			}
			ForEach(state.selectedFiles) { file in
				SelectedFileCard(
					file: file,
					canRemove: state.selectedFiles.count > 1 && !state.isSharing,
					onRemove: { model.removeSelectedFile(file.value) }
				)
			}
			Field(label: String(localized: L10n.Field.transferName),
				  value: Binding(get: { state.transferName }, set: { model.setTransferName($0) }))
			Field(label: String(localized: L10n.Field.senderName),
				  value: Binding(get: { state.senderName }, set: { model.setSenderName($0) }))
			Text(String(localized: L10n.Send.accessTitle)).font(.headline)
			PolicyOption(
				icon: .checkmarkShield, titleKey: L10n.Send.accessApproval, descKey: L10n.Send.accessApprovalDescription,
				selected: state.accessPolicy == .requireApproval,
				onTap: { model.setAccessPolicy(.requireApproval) }
			)
			PolicyOption(
				icon: .globe, titleKey: L10n.Send.accessAnyone, descKey: L10n.Send.accessAnyoneDescription,
				selected: state.accessPolicy == .anyoneWithTransfer,
				onTap: { model.setAccessPolicy(.anyoneWithTransfer) }
			)
			if state.accessPolicy == .anyoneWithTransfer {
				Label(String(localized: L10n.Send.accessAnyoneWarning), systemSymbol: .exclamationmarkTriangleFill)
					.font(.caption).foregroundStyle(.orange)
			}
			actions
		}
	}

	private var actions: some View {
		let shareTitle = state.isSharing
			? String(localized: L10n.Button.sharingFile) : String(localized: L10n.Button.shareFile)
		return VStack(spacing: 10) {
			PrimaryButton(
				title: shareTitle, action: model.createShare,
				enabled: state.canCreateShare(coreInitialized: model.coreState.isInitialized)
			)
			// Secondary source actions as an even row of bordered buttons rather than
			// bare text links, so they read as controls and align with the primary.
			HStack(spacing: 10) {
				sourceButton(title: L10n.Button.changeFiles, symbol: .docBadgeArrowUp, action: model.selectFile)
				sourceButton(title: L10n.Button.chooseFolder, symbol: .folder, action: model.selectFolder)
				if windowClass != .phone {
					sourceButton(title: L10n.Button.clear, symbol: .xmark, action: model.clearSelectedSource)
				}
			}
		}
	}

	private func sourceButton(
		title: String.LocalizationValue, symbol: SFSymbol, action: @escaping () -> Void
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
		.disabled(state.isSharing)
	}
}

private struct SelectedFileCard: View {
	let file: PickedShareFile
	let canRemove: Bool
	let onRemove: () -> Void

	var body: some View {
		HStack(spacing: 12) {
			FileArtwork(thumbnail: file.thumbnailData)
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

private struct PolicyOption: View {
	let icon: SFSymbol
	let titleKey: String.LocalizationValue
	let descKey: String.LocalizationValue
	let selected: Bool
	let onTap: () -> Void

	var body: some View {
		Button(action: onTap) {
			HStack(spacing: 12) {
				Image(systemSymbol: icon)
					.font(.system(size: 20))
					.foregroundStyle(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
					.frame(width: 22)
				VStack(alignment: .leading, spacing: 3) {
					Text(String(localized: titleKey))
					Text(String(localized: descKey)).font(.caption).foregroundStyle(.secondary)
				}
				Spacer()
				Image(systemSymbol: selected ? .checkmarkCircleFill : .circle)
					.foregroundStyle(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
			}
			.padding(14)
			.frame(maxWidth: .infinity)
			.background(.quaternary.opacity(selected ? 0.8 : 0.4), in: RoundedRectangle(cornerRadius: 14))
			.overlay(
				RoundedRectangle(cornerRadius: 14)
					.stroke(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), lineWidth: 1.5)
			)
		}
		.buttonStyle(.plain)
	}
}
