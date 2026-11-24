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

	func resetUnrecoverableIdentity(
		appDataDir: String,
		eventSink: CoreEventSink,
		networkConfiguration: RelayConfiguration
	) throws -> VnidropCore {
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

private final class BlockingReceiveCore: VnidropCore, @unchecked Sendable {
	private let started = DispatchSemaphore(value: 0)
	private let release = DispatchSemaphore(value: 0)

	init() {
		super.init(noPointer: .init())
	}

	required init(unsafeFromRawPointer pointer: UnsafeMutableRawPointer) {
		super.init(unsafeFromRawPointer: pointer)
	}

	override func status() -> RuntimeStatus {
		RuntimeStatus(endpointId: "test", addr: "", activeTransfers: 1, activeShares: 0)
	}

	override func listTransfers() throws -> [StoredTransfer] { [] }
	override func listEvents(transferId: UInt64?) throws -> [CoreEvent] { [] }
	override func shutdown() {}

	override func runtimeObligationFacts() throws -> RuntimeObligationFacts {
		RuntimeObligationFacts(
			activeInvitationTransfers: 1, invitationProviderAvailability: 0,
			targetedPreparations: 0, activeTargetedTransfers: 0, targetedProviderAvailability: 0
		)
	}

	override func receive(ticket: String, outputDir: String, receiverName: String?) throws {
		blockReceive()
	}

	override func receiveTargetedTransfer(transferId: String, outputDir: String) throws {
		blockReceive()
	}

	override func resumeTargetedTransfer(id: String, outputDir: String) throws {
		blockReceive()
	}

	func waitForReceive() -> Bool {
		started.wait(timeout: .now() + 2) == .success
	}

	func unblockReceive() {
		release.signal()
	}

	private func blockReceive() {
		started.signal()
		release.wait()
	}
}

private struct FixedCoreBindingFactory: CoreBindingFactory {
	let core: VnidropCore

	func initialize(appDataDir: String, eventSink: CoreEventSink, networkConfiguration: RelayConfiguration) throws -> VnidropCore {
		core
	}

	func resetUnrecoverableIdentity(
		appDataDir: String, eventSink: CoreEventSink, networkConfiguration: RelayConfiguration
	) throws -> VnidropCore {
		core
	}
}

private final class ThreadCheckedCore: VnidropCore, @unchecked Sendable {
	init() {
		super.init(noPointer: .init())
	}

	required init(unsafeFromRawPointer pointer: UnsafeMutableRawPointer) {
		super.init(unsafeFromRawPointer: pointer)
	}

	override func status() -> RuntimeStatus {
		XCTAssertFalse(Thread.isMainThread, "Reading native status blocked the main thread")
		return RuntimeStatus(endpointId: "test", addr: "", activeTransfers: 0, activeShares: 0)
	}

	override func listTransfers() throws -> [StoredTransfer] {
		XCTAssertFalse(Thread.isMainThread, "Reading native transfers blocked the main thread")
		return []
	}

	override func listEvents(transferId: UInt64?) throws -> [CoreEvent] {
		XCTAssertFalse(Thread.isMainThread, "Reading native events blocked the main thread")
		return []
	}

	override func shutdown() {
		XCTAssertFalse(Thread.isMainThread, "Stopping the native core blocked the main thread")
	}
}

@MainActor
final class CoreRepositoryLifecycleTests: XCTestCase {
	func testInitializationAndShutdownKeepNativeCallsOffTheMainThread() async throws {
		let core = ThreadCheckedCore()
		let repository = CoreRepository(coreFactory: FixedCoreBindingFactory(core: core))

		let initialized = await repository.initialize(appDataDir: "/tmp/test", networkConfiguration: .automatic)
		try initialized.get()
		XCTAssertTrue(repository.state.isInitialized)
		XCTAssertEqual(repository.state.status?.endpointId, "test")

		await repository.shutdown()
		XCTAssertFalse(repository.state.isInitialized)
		XCTAssertNil(repository.state.status)
	}

	func testIdleRequirementRejectsInvitationAndTargetedObligations() {
		let idle = RuntimeObligationFactsModel(
			activeInvitationTransfers: 0, invitationProviderAvailability: 0,
			targetedPreparations: 0, activeTargetedTransfers: 0, targetedProviderAvailability: 0
		)
		XCTAssertNoThrow(try CoreNetworkLifecycle.requireIdle(idle))
		let active = RuntimeObligationFactsModel(
			activeInvitationTransfers: 0, invitationProviderAvailability: 0,
			targetedPreparations: 0, activeTargetedTransfers: 0, targetedProviderAvailability: 1
		)
		XCTAssertThrowsError(try CoreNetworkLifecycle.requireIdle(active)) { error in
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

	func testRuntimeFactsRemainAvailableWhileReceiving() async throws {
		let core = BlockingReceiveCore()
		let repository = CoreRepository(coreFactory: FixedCoreBindingFactory(core: core))
		let initialized = await repository.initialize(appDataDir: "/tmp/test", networkConfiguration: .automatic)
		try initialized.get()

		let receives: [(String, @MainActor () async -> Result<Void, Error>)] = [
			("invitation", { await repository.receive(ticket: "ticket", outputDir: "/tmp", receiverName: "Tester") }),
			("security-scoped invitation", {
				await repository.receiveIntoSecurityScopedDirectory(ticket: "ticket", outputDirectoryUrl: "file:///tmp", receiverName: "Tester")
			}),
			("targeted", { await repository.receiveTargetedTransfer(transferId: "transfer", outputDirectoryUrl: "file:///tmp") }),
			("targeted resume", { await repository.resumeTargetedTransfer(id: "transfer", outputDirectoryUrl: "file:///tmp") }),
		]
		for (name, receive) in receives {
			let active = Task { await receive() }
			let started = await Task.detached { core.waitForReceive() }.value
			let finished = DispatchSemaphore(value: 0)
			let factsCall = Task {
				let facts = await repository.runtimeObligationFacts()
				finished.signal()
				return facts
			}
			let factsArrived = await Task.detached { Self.didSignal(finished) }.value
			if factsArrived {
				let restart = await repository.initialize(appDataDir: "/tmp/restart", networkConfiguration: .automatic)
				assertLifecycleFailure(restart, equals: .activeNetworkWork)
			}
			core.unblockReceive()
			let received = await active.value
			let facts = await factsCall.value
			XCTAssertTrue(started, "\(name) never entered the core")
			XCTAssertTrue(factsArrived, "\(name) blocked the runtime facts call")
			try received.get()
			XCTAssertTrue(try facts.get().requiresRuntime)
		}
		await repository.shutdown()
	}

	private nonisolated static func didSignal(_ semaphore: DispatchSemaphore) -> Bool {
		semaphore.wait(timeout: .now() + 2) == .success
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
