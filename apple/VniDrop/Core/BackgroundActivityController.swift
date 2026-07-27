import Combine
import Foundation
#if os(iOS)
import UIKit
#endif

/// Keeps the Rust core alive across the app moving to the background, within the
/// bounds Apple actually allows for a serverless P2P transfer app.
///
/// iOS suspends the whole process (freezing the core's network threads) shortly
/// after the app leaves the foreground. When a transfer or share is active we
/// take a `UIApplication` background-task assertion so iOS grants its finite
/// grace window — long enough for an in-flight transfer to finish streaming and
/// for its completion/failure notification to fire. There is no App-Store-legal
/// mechanism to keep serving or receiving *indefinitely* while backgrounded, and
/// `BGTaskScheduler` wake-ups run only opportunistically and cannot detect an
/// incoming peer connection, so they are deliberately not used here.
///
/// macOS does not suspend the process on focus loss, so this is a no-op there and
/// the core keeps running normally.
@MainActor
final class BackgroundActivityController {
	private let repository: CoreRepository

	init(repository: CoreRepository) {
		self.repository = repository
	}

	#if os(iOS)
	private var assertionId: UIBackgroundTaskIdentifier = .invalid
	private var idleCancellable: AnyCancellable?

	/// The app moved to the background. Hold the process open while there is live
	/// work; release as soon as it drains, on return to foreground, or when iOS
	/// ends the grace window (whichever comes first).
	func didEnterBackground() {
		guard assertionId == .invalid, hasActiveWork else { return }
		assertionId = UIApplication.shared.beginBackgroundTask(withName: "vnidrop.transfer") { [weak self] in
			// Expiration handler: iOS is reclaiming the window; end cleanly to
			// avoid the watchdog terminating the app.
			self?.endAssertion()
		}
		// Release the assertion the moment work finishes instead of holding it for
		// the full window (battery, and it lets the process suspend sooner). Events
		// still deliver on the main actor while the window is open, so the core's
		// active counts drop here when a transfer completes.
		idleCancellable = repository.statePublisher
			.map { ($0.status?.activeTransfers ?? 0) == 0 && ($0.status?.activeShares ?? 0) == 0 }
			.removeDuplicates()
			.sink { [weak self] idle in
				if idle { self?.endAssertion() }
			}
	}

	/// The app returned to the foreground; the process is live again, so drop any
	/// held assertion.
	func didBecomeForeground() {
		endAssertion()
	}

	private var hasActiveWork: Bool {
		let status = repository.state.status
		return (status?.activeTransfers ?? 0) > 0 || (status?.activeShares ?? 0) > 0
	}

	private func endAssertion() {
		idleCancellable?.cancel()
		idleCancellable = nil
		guard assertionId != .invalid else { return }
		UIApplication.shared.endBackgroundTask(assertionId)
		assertionId = .invalid
	}
	#else
	func didEnterBackground() {}
	func didBecomeForeground() {}
	#endif
}
