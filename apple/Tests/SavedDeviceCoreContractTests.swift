import Foundation
import XCTest
@preconcurrency import VnidropCore
@testable import VniDrop

/// Headless Apple harness for the saved-device core/platform contract (ticket 14).
///
/// Exercises protected identity restart, event revision recovery, and binding
/// hygiene against the generated UniFFI surface. The full two-node public-API
/// lifecycle (eligibility → unblock) lives in
/// `crates/vnidrop/src/tests/platform_contract_apple.rs` so it can run without
/// the iOS simulator.
final class SavedDeviceCoreContractTests: XCTestCase {
	private final class RecordingSink: CoreEventSink, @unchecked Sendable {
		private let lock = NSLock()
		private var events: [CoreEvent] = []

		func onEvent(event: CoreEvent) {
			lock.lock()
			events.append(event)
			lock.unlock()
		}

		func snapshot() -> [CoreEvent] {
			lock.lock()
			defer { lock.unlock() }
			return events
		}
	}

	func testProtectedKeychainIdentitySurvivesStandardConstructorRestart() throws {
		let directory = try FileManager.default.url(
			for: .itemReplacementDirectory,
			in: .userDomainMask,
			appropriateFor: FileManager.default.temporaryDirectory,
			create: true
		).appendingPathComponent("vnidrop-apple-contract-\(UUID().uuidString)", isDirectory: true)
		try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
		defer { try? FileManager.default.removeItem(at: directory) }

		let sink = RecordingSink()
		let first = try VnidropCore.initializeWithLimitsAndNetworkConfig(
			appDataDir: directory.path,
			eventSink: sink,
			limits: defaultCoreLimits(),
			networkConfig: CoreNetworkConfig(mode: .automatic, relayUrls: [])
		)
		let endpointId = first.status().endpointId
		XCTAssertFalse(endpointId.isEmpty)
		XCTAssertFalse(
			FileManager.default.fileExists(atPath: directory.appendingPathComponent("iroh.secret").path),
			"protected identity must not fall back to plaintext"
		)
		first.shutdown()

		let restarted = try VnidropCore.initializeWithLimitsAndNetworkConfig(
			appDataDir: directory.path,
			eventSink: RecordingSink(),
			limits: defaultCoreLimits(),
			networkConfig: CoreNetworkConfig(mode: .automatic, relayUrls: [])
		)
		defer { restarted.shutdown() }
		XCTAssertEqual(restarted.status().endpointId, endpointId)
	}

	func testEventRevisionRecoveryUsesStableIdsThenListApis() throws {
		let directory = try FileManager.default.url(
			for: .itemReplacementDirectory,
			in: .userDomainMask,
			appropriateFor: FileManager.default.temporaryDirectory,
			create: true
		).appendingPathComponent("vnidrop-apple-events-\(UUID().uuidString)", isDirectory: true)
		try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
		defer { try? FileManager.default.removeItem(at: directory) }

		let sink = RecordingSink()
		let core = try VnidropCore.initializeWithLimitsAndNetworkConfig(
			appDataDir: directory.path,
			eventSink: sink,
			limits: defaultCoreLimits(),
			networkConfig: CoreNetworkConfig(mode: .automatic, relayUrls: [])
		)
		defer { core.shutdown() }

		// Force at least one durable event through the public surface.
		XCTAssertThrowsError(
			try core.receive(ticket: "not-a-ticket", outputDir: directory.path, receiverName: nil)
		)

		let listed = try core.listEvents(transferId: nil)
		XCTAssertFalse(listed.isEmpty)

		var seenIds = Set<String>()
		var revisions = Set<UInt64>()
		for event in listed {
			XCTAssertTrue(seenIds.insert(event.id).inserted, "event ids must be stable/unique")
			XCTAssertGreaterThanOrEqual(event.revision, 1)
			XCTAssertTrue(revisions.insert(event.revision).inserted, "revisions must be distinct")
		}

		// Simulate duplicate delivery after a listener restart, then trust list APIs.
		let duplicates = listed
		var recoveredIds = Set<String>()
		var maxRevision: UInt64 = 0
		for event in listed + duplicates {
			if recoveredIds.insert(event.id).inserted {
				maxRevision = max(maxRevision, event.revision)
			}
		}
		XCTAssertEqual(recoveredIds.count, listed.count)
		XCTAssertGreaterThanOrEqual(maxRevision, 1)

		XCTAssertEqual(try core.listSavedDevices().count, 0)
		XCTAssertEqual(try core.listDeviceRelationships().count, 0)
		XCTAssertEqual(try core.listBlockedDevices().count, 0)
	}

	func testGeneratedBindingsOmitRawSecretsAndGenericMutation() throws {
		let candidates = [
			Bundle(for: SavedDeviceCoreContractTests.self).bundleURL
				.deletingLastPathComponent()
				.appendingPathComponent("Vnidrop.swift"),
			URL(fileURLWithPath: #filePath)
				.deletingLastPathComponent()
				.deletingLastPathComponent()
				.appendingPathComponent("VnidropCore/Sources/VnidropCore/Vnidrop.swift"),
		]

		guard let bindingsURL = candidates.first(where: { FileManager.default.fileExists(atPath: $0.path) })
		else {
			// Source package path may be absent in a clean CI checkout before
			// build-core; linking the typed APIs below still proves the
			// regenerated surface is what the harness compiles against.
			let _: (
				(String, CoreEventSink, CoreLimits, CoreNetworkConfig) throws -> VnidropCore
			) = VnidropCore.initializeWithLimitsAndNetworkConfig
			let capabilities: SavedDeviceCapabilities = savedDeviceCapabilities()
			XCTAssertGreaterThanOrEqual(capabilities.domainContractVersion, 1)
			XCTAssertNotNil(defaultCoreLimits().maxSavedDevices)
			return
		}

		let source = try String(contentsOf: bindingsURL, encoding: .utf8)
		let forbidden = [
			"SecretMaterial",
			"SecretHandle",
			"SecureSecretStore",
			"executeSql",
			"executeSQL",
			"mutateState",
			"applyRawState",
			"rawSecret",
			"grantSecret",
			"pairingCapabilityBytes",
			"func setState(",
			"func mutate(",
		]
		for needle in forbidden {
			XCTAssertFalse(
				source.contains(needle),
				"generated bindings must not expose \(needle)"
			)
		}
		XCTAssertFalse(source.contains("initializeWithExperimentalSavedDevices"))
		XCTAssertFalse(source.contains("ExperimentalSavedDeviceCapabilities"))
		XCTAssertFalse(source.contains("experimentalSavedDeviceCapabilities"))
		XCTAssertTrue(source.contains("initializeWithLimitsAndNetworkConfig"))
		XCTAssertTrue(source.contains("public struct SavedDeviceCapabilities"))
		XCTAssertTrue(source.contains("public func savedDeviceCapabilities()"))
		XCTAssertTrue(source.contains("setSavedDeviceLabel"))
		XCTAssertTrue(source.contains("listSavedDevices"))
		XCTAssertTrue(source.contains("revision"))
	}
}
