import Foundation

/// Dispatch-queue labels for the core's serial and interrupt lanes.
enum QueueLabel {
	static let core = "com.vnidrop.core"
	static let interrupt = "com.vnidrop.core.interrupt"
}

/// Two-lane dispatcher for blocking core calls.
///
/// `run` serializes control and status calls. `runInterrupt` uses a separate
/// concurrent queue so streaming receives, cancellation and status reads can
/// reach the internally synchronized core at the same time.
final class CoreDispatcher: Sendable {
	private let serialQueue: DispatchQueue
	private let interruptQueue: DispatchQueue

	init(label: String = QueueLabel.core, interruptLabel: String = QueueLabel.interrupt) {
		serialQueue = DispatchQueue(label: label, qos: .userInitiated)
		interruptQueue = DispatchQueue(label: interruptLabel, qos: .userInitiated, attributes: .concurrent)
	}

	/// Runs a blocking core call on the serial lane and hops the result back.
	func run<T: Sendable>(_ block: @escaping @Sendable () throws -> T) async -> Result<T, Error> {
		await withCheckedContinuation { continuation in
			serialQueue.async { continuation.resume(returning: Result { try block() }) }
		}
	}

	/// Runs transfers and interrupts that the core can execute concurrently.
	func runInterrupt<T: Sendable>(_ block: @escaping @Sendable () throws -> T) async -> Result<T, Error> {
		await withCheckedContinuation { continuation in
			interruptQueue.async { continuation.resume(returning: Result { try block() }) }
		}
	}
}
