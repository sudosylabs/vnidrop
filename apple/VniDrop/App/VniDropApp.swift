import SwiftUI

/// Scene identifier for the single main window.
private let mainWindowId = "main"

/// Native app entry point for iOS, iPadOS, and macOS.
/// Opens `.vnd` invitations via `onOpenURL` and routes them to the receive flow.
@main
struct VniDropApp: App {
	@StateObject private var externalInvitations = ExternalInvitationController()
	#if DIRECT_DISTRIBUTION && os(macOS)
	// Sparkle auto-updater, present only in the direct-download (.dmg) build.
	@StateObject private var updater = SparkleUpdaterController()
	#endif

	var body: some Scene {
		#if os(macOS)
		// A single-instance `Window` (not `WindowGroup`): the app must never open a
		// second window. `Window` also drops the ⌘N "New Window" command.
		Window(Text(verbatim: "VniDrop"), id: mainWindowId) {
			RootView(dependencies: makeAppDependencies(externalInvitations: externalInvitations))
				.ignoresSafeArea()
				.onOpenURL(perform: openInvitation)
		}
		#if DIRECT_DISTRIBUTION
		.commands {
			UpdatesCommands(controller: updater)
		}
		#endif
		#else
		WindowGroup(id: mainWindowId) {
			RootView(dependencies: makeAppDependencies(externalInvitations: externalInvitations))
				.ignoresSafeArea()
				.onOpenURL(perform: openInvitation)
		}
		#endif
	}

	private func openInvitation(_ url: URL) {
		Task { await externalInvitations.openInvitation(from: url) }
	}
}
