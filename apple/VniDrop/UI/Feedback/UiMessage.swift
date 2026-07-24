import Foundation
import Combine

/// A localizable UI string: either a catalog key or dynamic text, ported from
/// `UiText` in `ui/feedback/UiMessageController.kt`.
enum UiText: Equatable {
	case resource(String.LocalizationValue)   // Localizable.xcstrings key (use L10n.*)
	case dynamic(String)

	/// Resolves to display text. Keys go through the string catalog.
	func resolved() -> String {
		switch self {
		case .dynamic(let value): return value
		case .resource(let value): return String(localized: value)
		}
	}
}

enum UiMessageTone {
	case info
	case success
	case warning
	case error
}

struct UiMessage: Identifiable {
	let id = UUID()
	let text: UiText
	var tone: UiMessageTone = .info
	var actionLabel: UiText? = nil
	var onAction: (() -> Void)? = nil
}

/// Queues user-facing messages (snackbars) and dismissal requests. Ported from
/// `UiMessageController.kt`. Errors that are user cancellations are suppressed.
@MainActor
final class UiMessageController: ObservableObject {
	@Published private(set) var current: UiMessage?
	private var queue: [UiMessage] = []

	func show(_ message: UiMessage) {
		if current == nil {
			current = message
		} else {
			queue.append(message)
		}
	}

	@discardableResult
	func tryShow(_ message: UiMessage) -> Bool {
		show(message)
		return true
	}

	/// Called by the host when the current message is dismissed or times out.
	func advance() {
		if queue.isEmpty {
			current = nil
		} else {
			current = queue.removeFirst()
		}
	}

	/// Surfaces a user-facing error. Logs the technical detail; suppresses user
	/// cancellations. Mirrors `UiMessageController.error(Throwable)`.
	func error(_ error: Error) {
		if error.isUserCancellation {
			AppLogger.info("ui", "suppressed user cancellation", ["detail": error.technicalDetail])
			return
		}
		AppLogger.error("ui", "user-facing error", error)
		show(UiMessage(text: error.toUiText(), tone: .error))
	}

	func error(_ text: UiText) {
		show(UiMessage(text: text, tone: .error))
	}
}
