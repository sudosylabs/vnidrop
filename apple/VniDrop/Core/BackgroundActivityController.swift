import Combine
import Foundation
#if os(iOS)
import UIKit
#endif

enum BackgroundActivityPolicy {
	static func requiresAssertion(_ facts: RuntimeObligationFactsModel) -> Bool {
		facts.requiresRuntime
	}
}

/// Maps neutral core runtime obligations to Apple's finite background grace window.
/// Events are wake-ups only; every decision comes from a fresh fact snapshot.
@MainActor
final class BackgroundActivityController {
	private let repository: any CoreGateway

	init(repository: any CoreGateway) {
		self.repository = repository
	}

	#if os(iOS)
	private var assertionId: UIBackgroundTaskIdentifier = .invalid
	private var signalCancellable: AnyCancellable?
	private var factsTask: Task<Void, Never>?
	private var isBackgrounded = false

	func didEnterBackground() {
		guard !isBackgrounded else { return }
		isBackgrounded = true
		signalCancellable = repository.signals.sink { [weak self] signal in
			guard Self.affectsRuntimeObligations(signal) else { return }
			self?.scheduleRefresh()
		}
		scheduleRefresh()
	}

	func didBecomeForeground() {
		isBackgrounded = false
		factsTask?.cancel()
		factsTask = nil
		signalCancellable?.cancel()
		signalCancellable = nil
		releaseAssertion()
	}

	private static func affectsRuntimeObligations(_ signal: CoreSignal) -> Bool {
		switch signal {
		case .runtimeObligationChanged, .targetedTransferChanged, .transfersChanged:
			return true
		case .approvalChanged, .receiverHistoryChanged, .pairingChanged:
			return false
		}
	}

	private func scheduleRefresh() {
		factsTask?.cancel()
		factsTask = Task { [weak self] in
			guard let self else { return }
			let result = await repository.runtimeObligationFacts()
			guard !Task.isCancelled, isBackgrounded, case .success(let facts) = result else { return }
			if BackgroundActivityPolicy.requiresAssertion(facts) {
				beginAssertionIfNeeded()
			} else {
				releaseAssertion()
			}
		}
	}

	private func beginAssertionIfNeeded() {
		guard assertionId == .invalid else { return }
		assertionId = UIApplication.shared.beginBackgroundTask(withName: "vnidrop.transfer") { [weak self] in
			self?.didBecomeForeground()
		}
	}

	private func releaseAssertion() {
		guard assertionId != .invalid else { return }
		UIApplication.shared.endBackgroundTask(assertionId)
		assertionId = .invalid
	}
	#else
	func didEnterBackground() {}
	func didBecomeForeground() {}
	#endif
}
