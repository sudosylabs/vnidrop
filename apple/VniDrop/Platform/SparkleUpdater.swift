#if DIRECT_DISTRIBUTION && os(macOS)
import Combine
import Sparkle
import SwiftUI

/// Owns the Sparkle updater for the direct-download (.dmg) build.
///
/// Compiled only under `DIRECT_DISTRIBUTION`, so the App Store / TestFlight target
/// (which must not ship a self-updater) never compiles or links Sparkle. The feed
/// URL and public EdDSA key are read from Info.plist (`SUFeedURL`, `SUPublicEDKey`).
@MainActor
final class SparkleUpdaterController: ObservableObject {
	private let updaterController: SPUStandardUpdaterController
	/// Mirrors `SPUUpdater.canCheckForUpdates` so the menu item can disable itself
	/// while a check is already in flight.
	@Published private(set) var canCheckForUpdates = false

	init() {
		// `startingUpdater: true` begins the automatic background check schedule.
		updaterController = SPUStandardUpdaterController(
			startingUpdater: true,
			updaterDelegate: nil,
			userDriverDelegate: nil
		)
		updaterController.updater
			.publisher(for: \.canCheckForUpdates)
			.assign(to: &$canCheckForUpdates)
	}

	func checkForUpdates() {
		updaterController.checkForUpdates(nil)
	}
}

/// Adds a "Check for Updates…" item to the application menu (right after the
/// standard "About VniDrop" item), matching the macOS convention.
struct UpdatesCommands: Commands {
	@ObservedObject var controller: SparkleUpdaterController

	var body: some Commands {
		CommandGroup(after: .appInfo) {
			Button(String(localized: L10n.Updates.check)) {
				controller.checkForUpdates()
			}
			.disabled(!controller.canCheckForUpdates)
		}
	}
}
#endif
