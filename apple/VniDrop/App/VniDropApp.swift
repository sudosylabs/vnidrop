import SwiftUI

/// Scene identifier for the single main window.
private let mainWindowId = "main"

/// Native app entry point for iOS, iPadOS, and macOS.
/// Opens `.vnd` invitations via `onOpenURL` and routes them to the receive flow.
@main
struct VniDropApp: App {
	@StateObject private var externalInvitations = ExternalInvitationController()

	var body: some Scene {
		#if os(macOS)
		// A single-instance `Window` (not `WindowGroup`): the app must never open a
		// second window. `Window` also drops the ⌘N "New Window" command.
		Window(Text(verbatim: "VniDrop"), id: mainWindowId) {
			RootView(dependencies: makeAppDependencies(externalInvitations: externalInvitations))
				.ignoresSafeArea()
				.onOpenURL(perform: openInvitation)
		}
		#else
		WindowGroup(id: mainWindowId) {
			RootView(dependencies: makeAppDependencies(externalInvitations: externalInvitations))
				.ignoresSafeArea()
				.onOpenURL(perform: openInvitation)
		}
		#endif
	}

	/// Reads a `.vnd` invitation document under a security scope, enforcing the
	/// 64 KiB / strict-UTF-8 rules from `ContentView.swift`.
	private func openInvitation(_ url: URL) {
		guard url.pathExtension.caseInsensitiveCompare(vniDropInvitationExtension) == .orderedSame else {
			externalInvitations.reportOpenFailure(message: "This is not a VniDrop invitation")
			return
		}
		let started = url.startAccessingSecurityScopedResource()
		defer { if started { url.stopAccessingSecurityScopedResource() } }
		do {
			let values = try url.resourceValues(forKeys: [.fileSizeKey])
			if let size = values.fileSize, size > maxVniDropInvitationBytes {
				throw InvitationError.tooLarge
			}
			let data = try Data(contentsOf: url, options: .mappedIfSafe)
			let raw = try decodeInvitationBytes(data)
			externalInvitations.openInvitation(raw: raw)
		} catch {
			externalInvitations.reportOpenFailure(
				message: (error as? LocalizedError)?.errorDescription ?? "The invitation could not be opened"
			)
		}
	}
}
