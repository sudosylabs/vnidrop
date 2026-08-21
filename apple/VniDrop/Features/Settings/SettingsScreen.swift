import SwiftUI
import SFSafeSymbols

/// Settings screen, rebuilt on a native `Form` with `NavigationStack` push
/// navigation. The model stays the source of truth via a derived path binding.
struct SettingsScreen: View {
	@ObservedObject var model: SettingsModel
	let windowClass: WindowClass
	@State private var showBugReport = false

	private var path: Binding<[SettingsSection]> {
		Binding(
			get: {
				switch model.state.selectedSection {
				case .overview: return []
				case .bugReport: return [.about, .bugReport]
				case let section: return [section]
				}
			},
			set: { newPath in model.selectSection(newPath.last ?? .overview) }
		)
	}

	var body: some View {
		NavigationStack(path: path) {
			Form {
				#if os(iOS)
				// iOS suspends the app in the background, so serving/receiving
				// can't run indefinitely there (unlike macOS). Tell users up
				// front so the platform limit doesn't read as a bug.
				Section {
					VStack(alignment: .leading, spacing: 6) {
						Label(String(localized: L10n.Settings.iosBackgroundNoticeTitle), systemSymbol: .moonZzz)
							.font(.subheadline.weight(.semibold))
						Text(String(localized: L10n.Settings.iosBackgroundNoticeBody))
							.font(.footnote)
							.foregroundStyle(.secondary)
					}
					.padding(.vertical, 2)
				}
				#endif
				Section {
					NavigationLink(value: SettingsSection.preferences) {
						SettingsRow(icon: .personCropCircle, title: String(localized: L10n.Preferences.title), value: model.state.username)
					}
					NavigationLink(value: SettingsSection.appearance) {
						SettingsRow(icon: .sunMax, title: String(localized: L10n.Appearance.title), value: themeModeLabel(model.state.themeMode))
					}
				}
				Section {
					NavigationLink(value: SettingsSection.notifications) {
						SettingsRow(icon: .bell, title: String(localized: L10n.Notifications.title), value: nil)
					}
					NavigationLink(value: SettingsSection.storage) {
						SettingsRow(icon: .internaldrive, title: String(localized: L10n.Storage.title), value: nil)
					}
				}
				Section(String(localized: L10n.Settings.advancedTitle)) {
					NavigationLink(value: SettingsSection.network) {
						SettingsRow(
							icon: .network,
							title: String(localized: L10n.Settings.networkTitle),
							value: relayModeLabel(model.state.relayMode)
						)
					}
				}
				Section {
					NavigationLink(value: SettingsSection.about) {
						SettingsRow(icon: .infoCircle, title: String(localized: L10n.About.title), value: nil)
					}
				}
			}
			.formStyle(.grouped)
			.navigationTitle(Text(String(localized: L10n.Settings.title)))
			.navigationDestination(for: SettingsSection.self) { section in
				sectionForm(section)
			}
		}
	}

	@ViewBuilder
	private func sectionForm(_ section: SettingsSection) -> some View {
		settingsSectionForm(section)
	}

	@ViewBuilder
	private func settingsSectionForm(_ section: SettingsSection) -> some View {
		let content = Form {
			SettingsSectionContent(model: model, section: section)
		}
		.formStyle(.grouped)
		.navigationTitle(Text(String(localized: section.titleKey)))

		if section == .about {
			content
				#if os(iOS)
				.navigationBarTitleDisplayMode(.inline)
				#endif
				.toolbar {
					ToolbarItem(placement: .primaryAction) {
						Button {
							showBugReport = true
						} label: {
							Label(String(localized: L10n.About.bugReport), systemSymbol: .ladybug)
						}
					}
				}
				.sheet(isPresented: $showBugReport) {
					BugReportSheet(model: model)
				}
		} else {
			content
				#if os(iOS)
				.navigationBarTitleDisplayMode(.inline)
				#endif
		}
	}
}

private struct SettingsSectionContent: View {
	@ObservedObject var model: SettingsModel
	let section: SettingsSection

	var body: some View {
		switch section {
		case .overview:
			EmptyView()
		case .preferences:
			PreferencesSettings(model: model)
		case .appearance:
			AppearanceSettings(model: model)
		case .notifications:
			NotificationSettings(model: model)
		case .network:
			NetworkSettings(model: model)
		case .storage:
			StorageSettings(model: model)
		case .about:
			AboutSettings(model: model)
		case .bugReport:
			BugReportSettings(model: model)
		}
	}
}

func relayModeLabel(_ mode: RelayPreferenceMode) -> String {
	switch mode {
	case .automatic: return String(localized: L10n.Relay.modeAutomatic)
	case .strictCustom: return String(localized: L10n.Relay.modeCustom)
	case .customWithDirectFallback: return String(localized: L10n.Relay.modeCustomDirectFallback)
	case .localOnly: return String(localized: L10n.Relay.modeLocalOnly)
	}
}

func relayModeDescription(_ mode: RelayPreferenceMode) -> String.LocalizationValue {
	switch mode {
	case .automatic: return L10n.Relay.modeAutomaticDescription
	case .strictCustom: return L10n.Relay.modeCustomDescription
	case .customWithDirectFallback: return L10n.Relay.modeCustomDirectFallbackDescription
	case .localOnly: return L10n.Relay.modeLocalOnlyDescription
	}
}

struct SettingsRow: View {
	let icon: SFSymbol
	let title: String
	let value: String?

	var body: some View {
		HStack(spacing: 12) {
			Image(systemSymbol: icon)
				.foregroundStyle(.tint)
				.frame(width: 26)
			Text(title).foregroundStyle(.primary)
			Spacer()
			if let value {
				Text(value).foregroundStyle(.secondary).lineLimit(1)
			}
		}
	}
}

func themeModeLabel(_ mode: ThemeMode) -> String {
	switch mode {
	case .system: return String(localized: L10n.Appearance.systemMode)
	case .light: return String(localized: L10n.Appearance.lightMode)
	case .dark: return String(localized: L10n.Appearance.darkMode)
	}
}
