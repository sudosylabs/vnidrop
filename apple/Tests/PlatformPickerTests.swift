import Foundation
import XCTest
@testable import VniDrop

#if os(iOS)
private final class ScopeReleaseCounter: @unchecked Sendable {
	private let lock = NSLock()
	private var releases = 0

	var count: Int { lock.withLock { releases } }

	func release() {
		lock.withLock { releases += 1 }
	}
}

@MainActor
final class PlatformPickerTests: XCTestCase {
	func testShareSelectionCopiesFilesOffTheMainThreadWhileHoldingAccess() async throws {
		let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
		try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
		defer { try? FileManager.default.removeItem(at: root) }
		let source = root.appendingPathComponent("report.txt")
		let contents = Data("report".utf8)
		try contents.write(to: source)
		let counter = ScopeReleaseCounter()

		let worker = PickerSupport.stageShareSelection(
			[source], isDirectory: false,
			acquireAccess: { _ in { counter.release() } },
			copyFile: { original, copy in
				XCTAssertFalse(Thread.isMainThread)
				XCTAssertEqual(counter.count, 0)
				try FileManager.default.copyItem(at: original, to: copy)
			}
		)
		let files = await worker.value
		let staged = try XCTUnwrap(files.first)
		let stagedURL = URL(fileURLWithPath: staged.value)
		defer { try? FileManager.default.removeItem(at: stagedURL.deletingLastPathComponent()) }
		XCTAssertTrue(staged.isTemporaryCopy)
		XCTAssertEqual(try Data(contentsOf: stagedURL), contents)
		XCTAssertEqual(counter.count, 1)
	}

	func testPickingAnIosDirectoryRetainsAccessUntilTheSelectionIsReleased() async {
		let counter = ScopeReleaseCounter()
		let folder = URL(fileURLWithPath: "/tmp/picked-folder", isDirectory: true)
		var picked = await PickerSupport.stageShareSelection(
			[folder], isDirectory: true,
			acquireAccess: { url in
				XCTAssertEqual(url, folder)
				return { counter.release() }
			}
		).value.first

		XCTAssertEqual(picked?.value, folder.path)
		XCTAssertEqual(picked?.isDirectory, true)
		withExtendedLifetime(picked) {
			XCTAssertEqual(counter.count, 0)
		}
		picked = nil
		XCTAssertEqual(counter.count, 1)
	}
}
#endif
