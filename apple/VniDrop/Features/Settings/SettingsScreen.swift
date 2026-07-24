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
	case .automatic: return String(localized: "relay_mode_automatic")
	case .strictCustom: return String(localized: "relay_mode_custom")
	case .customWithDirectFallback: return String(localized: "relay_mode_custom_direct_fallback")
	case .localOnly: return String(localized: "relay_mode_local_only")
	}
}

func relayModeDescriptionKey(_ mode: RelayPreferenceMode) -> String {
	switch mode {
	case .automatic: return "relay_mode_automatic_description"
	case .strictCustom: return "relay_mode_custom_description"
	case .customWithDirectFallback: return "relay_mode_custom_direct_fallback_description"
	case .localOnly: return "relay_mode_local_only_description"
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
