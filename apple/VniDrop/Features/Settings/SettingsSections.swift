import SwiftUI
import SFSafeSymbols

/// Settings section detail views, rebuilt as native `Form` content. Each view is
/// placed inside a parent `Form`, so it returns `Section`s / rows directly.

struct PreferencesSettings: View {
	@ObservedObject var model: SettingsModel

	var body: some View {
		Section(String(localized: L10n.Field.username)) {
			TextField(String(localized: L10n.Field.username),
					  text: Binding(get: { model.state.username }, set: { model.setUsername($0) }))
		}
		if model.state.supportsCustomReceiveFolders {
			Section(String(localized: L10n.Preferences.receiveFolderTitle)) {
				LabeledContent {
					Button(String(localized: L10n.Button.chooseFolder), action: model.chooseReceiveFolder)
				} label: {
					Label {
						Text(model.state.receiveFolder?.displayName ?? String(localized: L10n.Value.unavailable))
							.lineLimit(1)
							.truncationMode(.middle)
					} icon: {
						Image(systemSymbol: .folder)
					}
				}
				if !model.isUsingDefaultReceiveFolder {
					Button(String(localized: L10n.Button.resetDefault), role: .cancel, action: model.resetReceiveFolder)
				}
			}
		}
	}
}

struct AppearanceSettings: View {
	@ObservedObject var model: SettingsModel

	var body: some View {
		Section {
			Picker(String(localized: L10n.Appearance.title),
				   selection: Binding(get: { model.state.themeMode }, set: { model.setThemeMode($0) })) {
				ForEach(ThemeMode.allCases, id: \.self) { mode in
					Text(themeModeLabel(mode)).tag(mode)
				}
			}
			.pickerStyle(.inline)
			.labelsHidden()
		}
	}
}

struct NotificationSettings: View {
	@ObservedObject var model: SettingsModel

	var body: some View {
		Section {
			Text(String(localized: L10n.Notifications.description)).foregroundStyle(.secondary)
			switch model.state.notificationPermission {
			case .notDetermined:
				Button(String(localized: L10n.Notifications.localTitle), action: model.requestNotifications)
			case .granted:
				// Allowed — the OS Settings app is where you disable or fine-tune.
				Text(String(localized: L10n.Notifications.enabledMessage)).foregroundStyle(.secondary)
				Button(String(localized: L10n.Button.openSettings), action: model.openNotificationSettings)
			case .denied:
				Text(String(localized: L10n.Notifications.permissionDenied)).foregroundStyle(.secondary)
				Button(String(localized: L10n.Button.openSettings), action: model.openNotificationSettings)
			case .unsupported:
				Text(String(localized: L10n.Notifications.unsupported)).foregroundStyle(.secondary)
			}
		}
		.onAppear { model.refreshNotificationPermission() }
	}
}

struct NetworkSettings: View {
	@ObservedObject var model: SettingsModel

	var body: some View {
		Section {
			Picker(
				"",
				selection: Binding(get: { model.state.relayMode }, set: { model.setRelayMode($0) })
			) {
				ForEach(RelayPreferenceMode.allCases, id: \.self) { mode in
					Text(relayModeLabel(mode)).tag(mode)
				}
			}
			.pickerStyle(.inline)
			.labelsHidden()
			.disabled(model.state.isApplyingRelayConfiguration)
		} header: {
			Text(String(localized: L10n.Settings.networkTitle))
		} footer: {
			Text(String(localized: relayModeDescription(model.state.relayMode)))
		}

		Section {
			Label {
				Text(String(localized: L10n.Relay.privacyDescription))
					.fixedSize(horizontal: false, vertical: true)
			} icon: {
				Image(systemSymbol: .lockShield)
			}
			.foregroundStyle(.secondary)
		}

		if let endpointId = model.state.endpointId, !endpointId.isEmpty {
			Section {
				Text(L10n.Approval.endpointId(deviceId: endpointId))
					.font(.footnote.monospaced())
					.textSelection(.enabled)
			}
		}

		if model.state.relayMode.usesCustomRelayURLs {
			Section {
				if model.state.relayMode == .strictCustom {
					Label {
						Text(String(localized: L10n.Relay.strictWarning))
							.fixedSize(horizontal: false, vertical: true)
					} icon: {
						Image(systemSymbol: .exclamationmarkShieldFill)
					}
					.foregroundStyle(.orange)
				}

				ForEach(Array(model.state.relayURLs.indices), id: \.self) { index in
					VStack(alignment: .leading, spacing: 6) {
						HStack {
							TextField(
								"",
								text: Binding(
									get: {
										model.state.relayURLs.indices.contains(index)
											? model.state.relayURLs[index]
											: ""
									},
									set: { model.setRelayURL($0, at: index) }
								),
								// `Text(verbatim:)` avoids macOS markdown-linkifying the
								// URL-shaped placeholder into a purple link.
								prompt: Text(verbatim: "https://relay.example.com")
							)
							.labelsHidden()
							#if os(iOS)
							.keyboardType(.URL)
							.textInputAutocapitalization(.never)
							#endif
							.autocorrectionDisabled()
							.disabled(model.state.isApplyingRelayConfiguration)

							Button(role: .destructive) {
								model.removeRelayURL(at: index)
							} label: {
								Image(systemSymbol: .minusCircleFill)
							}
							.buttonStyle(.borderless)
							.accessibilityLabel(Text(String(localized: L10n.Relay.removeUrl)))
							.disabled(model.state.isApplyingRelayConfiguration)
						}

						if let error = model.state.relayValidationError, error.urlIndex == index {
							Text(relayValidationMessage(error))
								.font(.caption)
								.foregroundStyle(.red)
						}
					}
				}

				Button(action: model.addRelayURL) {
					Label(String(localized: L10n.Relay.addUrl), systemSymbol: .plusCircle)
				}
				.disabled(
					model.state.relayURLs.count >= RelayConfigurationValidator.maximumRelayCount
						|| model.state.isApplyingRelayConfiguration
				)
			} header: {
				Text(String(localized: L10n.Relay.customUrlsLabel))
			} footer: {
				Text(String(localized: L10n.Relay.customUrlsHelp))
			}
		}

		if let error = model.state.relayValidationError, error.urlIndex == nil {
			Section {
				Label {
					Text(relayValidationMessage(error))
				} icon: {
					Image(systemSymbol: .exclamationmarkTriangleFill)
				}
				.foregroundStyle(.red)
			}
		}

		if model.state.hasActiveNetworkWork || model.state.relayApplyErrorKey != nil {
			Section {
				Label {
					Text(String(localized: model.state.hasActiveNetworkWork ? L10n.Relay.applyActiveTransfers : (model.state.relayApplyErrorKey ?? L10n.Relay.applyFailed)))
				} icon: {
					Image(systemSymbol: .exclamationmarkTriangleFill)
				}
				.foregroundStyle(.red)
			}
		}

		Section {
			Button(action: model.applyRelayConfiguration) {
				HStack {
					Text(String(localized: model.state.isApplyingRelayConfiguration ? L10n.Relay.applying : L10n.Relay.apply))
					if model.state.isApplyingRelayConfiguration {
						Spacer()
						ProgressView()
					}
				}
			}
			.disabled(
				!model.state.relayConfigurationIsDirty
					|| model.state.isApplyingRelayConfiguration
					|| model.state.hasActiveNetworkWork
			)
		} footer: {
			Text(String(localized: L10n.Relay.applyRestartDescription))
		}
	}
}

private func relayValidationMessage(_ error: RelayConfigurationValidationError) -> String {
	switch error {
	case .missingURL:
		return String(localized: L10n.Relay.validationMissingUrl)
	case .tooManyURLs:
		return L10n.Relay.validationTooManyUrls(maximum: RelayConfigurationValidator.maximumRelayCount)
	case .httpsRequired(let index):
		return L10n.Relay.validationHttpsRequired(line: index + 1)
	case .invalidURL(let index):
		return L10n.Relay.validationInvalidUrl(line: index + 1)
	case .duplicateURL(let index):
		return L10n.Relay.validationDuplicateUrl(line: index + 1)
	}
}

struct StorageSettings: View {
	@ObservedObject var model: SettingsModel
	@State private var showDeleteConfirmation = false

	private var isBusy: Bool {
		model.state.isCalculatingStorage || model.state.isCleaningStorage || model.state.isDeletingTransfers
	}

	var body: some View {
		Section {
			usageContent
		} header: {
			HStack {
				Text(String(localized: L10n.Storage.usageHeader))
				Spacer()
				if model.state.isCalculatingStorage {
					ProgressView().controlSize(.small)
				} else {
					Button(action: model.loadStorageUsage) {
						Label(String(localized: L10n.Storage.refresh), systemSymbol: .arrowClockwise)
							.labelStyle(.iconOnly)
					}
					.buttonStyle(.borderless)
					.disabled(isBusy)
					.help(String(localized: L10n.Storage.refresh))
				}
			}
		} footer: {
			Text(String(localized: L10n.Storage.footer))
		}

		// Reclaim reversible junk (temp + trash) — non-destructive to history.
		Section {
			Button(action: model.freeUpSpace) {
				actionLabel(
					title: L10n.Storage.freeUpSpace,
					busyTitle: L10n.Storage.cleaning,
					isBusy: model.state.isCleaningStorage,
					symbol: .sparkles,
					tint: .accentColor
				)
			}
			// `.plain` so pressing the row dims the label instead of flipping it to
			// the white selection-highlight that the default form button style uses.
			.buttonStyle(.plain)
			.disabled(isBusy)
		} footer: {
			Text(String(localized: L10n.Storage.freeUpSpaceCaption))
		}

		// Destructive: clears transfer history + cached share content.
		Section {
			Button {
				showDeleteConfirmation = true
			} label: {
				actionLabel(
					title: L10n.Storage.deleteTransfers,
					busyTitle: L10n.Storage.deleting,
					isBusy: model.state.isDeletingTransfers,
					symbol: .trash,
					tint: .red
				)
			}
			.buttonStyle(.plain)
			.disabled(isBusy)
		} footer: {
			Text(String(localized: L10n.Storage.deleteTransfersCaption))
		}
		.task { model.loadStorageUsage() }
		.confirmationDialog(
			Text(String(localized: L10n.Storage.deleteTransfers)),
			isPresented: $showDeleteConfirmation,
			titleVisibility: .visible
		) {
			Button(String(localized: L10n.Storage.deleteTransfers), role: .destructive) {
				model.deleteAllTransfers()
			}
			Button(String(localized: L10n.Button.cancel), role: .cancel) {}
		} message: {
			Text(String(localized: L10n.Storage.deleteTransfersDescription))
		}
	}

	@ViewBuilder
	private var usageContent: some View {
		if let storage = model.state.storage {
			LabeledContent(String(localized: L10n.Storage.receivedFiles), value: formatBytes(storage.receivedFiles))
			LabeledContent(String(localized: L10n.Storage.transferData), value: formatBytes(storage.transferCache))
			LabeledContent(String(localized: L10n.Storage.appData), value: formatBytes(storage.appData))
			LabeledContent(String(localized: L10n.Storage.temporary), value: formatBytes(storage.temporary))
			LabeledContent(String(localized: L10n.Storage.total)) {
				Text(formatBytes(storage.total)).fontWeight(.semibold)
			}
		} else if model.state.storageLoadFailed {
			// Genuine failure (core reported an error) — offer a retry.
			Button(action: model.loadStorageUsage) {
				Label(String(localized: L10n.Storage.unavailable), systemSymbol: .arrowClockwise)
					.foregroundStyle(.secondary)
			}
			.buttonStyle(.plain)
		} else {
			// Loading, or waiting for the core to finish starting.
			HStack {
				Text(String(localized: L10n.Storage.calculating)).foregroundStyle(.secondary)
				Spacer()
				ProgressView().controlSize(.small)
			}
		}
	}

	/// A tinted, full-width button label with a leading symbol and a trailing
	/// spinner while busy. `.contentShape` keeps the whole row tappable.
	private func actionLabel(
		title: String.LocalizationValue,
		busyTitle: String.LocalizationValue,
		isBusy: Bool,
		symbol: SFSymbol,
		tint: Color
	) -> some View {
		HStack {
			Label(String(localized: isBusy ? busyTitle : title), systemSymbol: symbol)
			Spacer()
			if isBusy {
				ProgressView().controlSize(.small)
			}
		}
		.foregroundStyle(tint)
		.contentShape(Rectangle())
	}
}

struct AboutSettings: View {
	@ObservedObject var model: SettingsModel

	private static let privacyPolicyURL = AppConfig.privacyPolicyURL

	var body: some View {
		Section {
			Text(String(localized: L10n.About.tagline)).font(.headline)
			Text(String(localized: L10n.About.description)).foregroundStyle(.secondary)
		}

		Section(String(localized: L10n.About.isTitle)) {
			AboutPoint(L10n.About.isDirect, .paperplane)
			AboutPoint(L10n.About.isNoAccount, .personCropCircleBadgeXmark)
			AboutPoint(L10n.About.isInControl, .checkmarkShield)
			AboutPoint(L10n.About.isEncrypted, .lock)
			AboutPoint(L10n.About.isOpen, .chevronLeftForwardslashChevronRight)
		}

		Section(String(localized: L10n.About.isntTitle)) {
			AboutPoint(L10n.About.isntCloud, .icloudSlash)
			AboutPoint(L10n.About.isntSync, .arrowTriangle2Circlepath)
			AboutPoint(L10n.About.isntPublic, .megaphone)
		}

		Section(String(localized: L10n.About.privacyTitle)) {
			AboutPoint(L10n.About.privacyCapability, .qrcode)
			AboutPoint(L10n.About.privacyDeny, .handRaised)
			AboutPoint(L10n.About.privacyRelay, .antennaRadiowavesLeftAndRight)
			AboutPoint(L10n.About.privacyLocal, .internaldrive)
		}

		Section(String(localized: L10n.About.title)) {
			LabeledContent(String(localized: L10n.Version.title), value: model.state.appVersion)
			if let device = model.state.deviceInfo {
				LabeledContent(String(localized: L10n.Device.modelTitle), value: device.deviceModel ?? "—")
				LabeledContent(String(localized: L10n.Os.versionTitle), value: device.operatingSystem)
			}
			LabeledContent(String(localized: L10n.About.licenseLabel), value: "Apache 2.0")
			Link(destination: Self.privacyPolicyURL) {
				Label(String(localized: L10n.About.privacyPolicyLabel), systemSymbol: .handRaised)
			}
		}

		if DiagnosticsBuildConfig.included {
			Section {
				Toggle(isOn: Binding(
					get: { model.state.diagnosticsEnabled },
					set: { model.setDiagnosticsEnabled($0) }
				)) {
					Text(String(localized: L10n.Diagnostics.title))
				}
			}
		}
	}
}

/// Bug report presented as a sheet from About. Can be dismissed by swipe only
/// when empty; otherwise the Cancel button is required.
struct BugReportSheet: View {
	@ObservedObject var model: SettingsModel
	@Environment(\.dismiss) private var dismiss

	private var isEmpty: Bool {
		model.state.bugWhatHappened.isEmpty && model.state.bugExpected.isEmpty
			&& model.state.bugSteps.isEmpty && model.state.bugContact.isEmpty
	}

	var body: some View {
		NavigationStack {
			Form {
				BugReportSettings(model: model, onSubmitted: { dismiss() })
			}
			.formStyle(.grouped)
			.navigationTitle(Text(String(localized: L10n.About.bugReport)))
			#if os(iOS)
			.navigationBarTitleDisplayMode(.inline)
			#endif
			.toolbar {
				ToolbarItem(placement: .cancellationAction) {
					Button(String(localized: L10n.Button.cancel)) { dismiss() }
				}
			}
		}
		.interactiveDismissDisabled(!isEmpty)
	}
}

/// A bullet-style informational row with an SF Symbol and wrapping localized text.
private struct AboutPoint: View {
	let key: String.LocalizationValue
	let symbol: SFSymbol

	init(_ key: String.LocalizationValue, _ symbol: SFSymbol) {
		self.key = key
		self.symbol = symbol
	}

	var body: some View {
		Label {
			Text(String(localized: key))
				.font(.subheadline)
				.fixedSize(horizontal: false, vertical: true)
		} icon: {
			Image(systemSymbol: symbol).foregroundStyle(.tint)
		}
	}
}

struct BugReportSettings: View {
	@ObservedObject var model: SettingsModel
	var onSubmitted: () -> Void = {}

	var body: some View {
		Section(String(localized: L10n.Bug.reportWhatLabel)) {
			TextField("", text: Binding(get: { model.state.bugWhatHappened }, set: { model.setBugWhatHappened($0) }),
					  prompt: Text(String(localized: L10n.Bug.reportWhatHint)), axis: .vertical)
				.lineLimit(3, reservesSpace: true)
				.labelsHidden()
		}
		Section(String(localized: L10n.Bug.reportExpectedLabel)) {
			TextField("", text: Binding(get: { model.state.bugExpected }, set: { model.setBugExpected($0) }),
					  prompt: Text(String(localized: L10n.Bug.reportExpectedHint)), axis: .vertical)
				.lineLimit(3, reservesSpace: true)
				.labelsHidden()
		}
		Section(String(localized: L10n.Bug.reportStepsLabel)) {
			TextField("", text: Binding(get: { model.state.bugSteps }, set: { model.setBugSteps($0) }),
					  prompt: Text(String(localized: L10n.Bug.reportStepsHint)), axis: .vertical)
				.lineLimit(3, reservesSpace: true)
				.labelsHidden()
		}
		Section(String(localized: L10n.Bug.reportContactLabel)) {
			TextField("", text: Binding(get: { model.state.bugContact }, set: { model.setBugContact($0) }),
					  prompt: Text(String(localized: L10n.Bug.reportContactHint)))
				.labelsHidden()
		}
		Section {
			Toggle(isOn: Binding(get: { model.state.bugIncludeLogs }, set: { model.setBugIncludeLogs($0) })) {
				Text(String(localized: L10n.Bug.reportIncludeLogs))
			}
			Button(action: { model.submitBugReport(onSuccess: onSubmitted) }) {
				Text(model.state.isSubmittingBugReport
					 ? String(localized: L10n.Bug.reportSubmitting) : String(localized: L10n.Bug.reportSubmit))
			}
			.disabled(model.state.isSubmittingBugReport)
		}
	}
}
