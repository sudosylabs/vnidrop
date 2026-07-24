import Foundation
import XCTest
@preconcurrency import VnidropCore
@testable import VniDrop

private enum BlockingCoreFactoryError: Error {
	case stopped
}

private final class BlockingCoreBindingFactory: CoreBindingFactory, @unchecked Sendable {
	private let release = DispatchSemaphore(value: 0)
	private let lock = NSLock()
	private var initializeCallCount = 0
	private var initializationStarted = false
	private var startWaiters: [CheckedContinuation<Void, Never>] = []

	var callCount: Int {
		lock.lock()
		defer { lock.unlock() }
		return initializeCallCount
	}

	func initialize(
		appDataDir: String,
		eventSink: CoreEventSink,
		networkConfiguration: RelayConfiguration
	) throws -> VnidropCore {
		lock.lock()
		initializeCallCount += 1
		let call = initializeCallCount
		initializationStarted = true
		let waiters = startWaiters
		startWaiters.removeAll()
		lock.unlock()
		waiters.forEach { $0.resume() }

		if call == 1 {
			release.wait()
		}
		throw BlockingCoreFactoryError.stopped
	}

	func waitUntilInitializationStarts() async {
		await withCheckedContinuation { continuation in
			lock.lock()
			if initializationStarted {
				lock.unlock()
				continuation.resume()
			} else {
				startWaiters.append(continuation)
				lock.unlock()
			}
		}
	}

	func unblockInitialization() {
		release.signal()
	}
}

@MainActor
final class CoreRepositoryLifecycleTests: XCTestCase {
	func testIdleRequirementRejectsTransfersAndShares() throws {
		XCTAssertNoThrow(try CoreNetworkLifecycle.requireIdle(activeTransfers: 0, activeShares: 0))
		XCTAssertThrowsError(
			try CoreNetworkLifecycle.requireIdle(activeTransfers: 1, activeShares: 0)
		) { error in
			XCTAssertEqual(error as? CoreNetworkLifecycleError, .activeNetworkWork)
		}
		XCTAssertThrowsError(
			try CoreNetworkLifecycle.requireIdle(activeTransfers: 0, activeShares: 1)
		) { error in
			XCTAssertEqual(error as? CoreNetworkLifecycleError, .activeNetworkWork)
		}
	}

	func testRestartSerializesInitializationAndRejectsNewNetworkWork() async {
		let factory = BlockingCoreBindingFactory()
		let repository = CoreRepository(coreFactory: factory)
		let firstInitialization = Task {
			await repository.initialize(appDataDir: "/tmp/first", networkConfiguration: .automatic)
		}
		await factory.waitUntilInitializationStarts()

		let safetyRelease = Task.detached {
			try? await Task.sleep(nanoseconds: 1_000_000_000)
			guard !Task.isCancelled else { return }
			factory.unblockInitialization()
		}
		defer {
			safetyRelease.cancel()
			factory.unblockInitialization()
		}

		let concurrentInitialization = await repository.initialize(
			appDataDir: "/tmp/second",
			networkConfiguration: RelayConfiguration(mode: .strictCustom, relayURLs: ["https://relay.example"])
		)
		assertLifecycleFailure(concurrentInitialization, equals: .transitionInProgress)

		let share = await repository.shareSources(
			[],
			transferName: "Blocked",
			senderName: "Tester",
			accessPolicy: .requireApproval
		)
		assertLifecycleFailure(share, equals: .transitionInProgress)

		let receive = await repository.receive(ticket: "ticket", outputDir: "/tmp", receiverName: "Tester")
		assertLifecycleFailure(receive, equals: .transitionInProgress)
		XCTAssertEqual(factory.callCount, 1)

		factory.unblockInitialization()
		guard case .failure(let error) = await firstInitialization.value else {
			return XCTFail("The blocking factory should fail the first initialization")
		}
		XCTAssertTrue(error is BlockingCoreFactoryError)
	}

	private func assertLifecycleFailure<T>(
		_ result: Result<T, Error>,
		equals expected: CoreNetworkLifecycleError,
		file: StaticString = #filePath,
		line: UInt = #line
	) {
		guard case .failure(let error) = result else {
			return XCTFail("Expected lifecycle failure \(expected)", file: file, line: line)
		}
		XCTAssertEqual(error as? CoreNetworkLifecycleError, expected, file: file, line: line)
	}
}
