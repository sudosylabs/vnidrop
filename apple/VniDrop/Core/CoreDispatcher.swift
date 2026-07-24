import Foundation

/// Dispatch-queue labels for the core's serial and interrupt lanes.
enum QueueLabel {
	static let core = "com.vnidrop.core"
	static let interrupt = "com.vnidrop.core.interrupt"
}

/// Two-lane dispatcher for blocking core calls.
///
/// `run` serializes calls on one queue so the core is driven from a single lane.
/// `runInterrupt` uses a *separate* concurrent lane, so an interrupt-style call
/// (cancel) can reach the core while a blocking call (`receive`) still occupies
/// the serial lane. The core is internally synchronized and explicitly supports
/// cancel arriving from another thread mid-receive (see VnidropCore.block_on);
/// a single shared queue would deadlock it.
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

	/// Like `run`, but off the serial lane so it can interrupt a blocking call in
	/// flight there (e.g. cancel a `receive`). Only use for core calls that are
	/// safe to run concurrently with another core call.
	func runInterrupt<T: Sendable>(_ block: @escaping @Sendable () throws -> T) async -> Result<T, Error> {
		await withCheckedContinuation { continuation in
			interruptQueue.async { continuation.resume(returning: Result { try block() }) }
		}
	}
}
