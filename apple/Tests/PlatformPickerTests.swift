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

	func testSecondStagingFailureReleasesBothScopesAndDiscardsFirstCopy() async throws {
		let counter = ScopeReleaseCounter()
		let first = URL(fileURLWithPath: "/first")
		let second = URL(fileURLWithPath: "/second")
		let marker = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		defer { try? FileManager.default.removeItem(at: marker) }
		let result = await PickerSupport.stageShareSelection([first, second], isDirectory: false,
			acquireAccess: { _ in { counter.release() } }, copyFile: { source, destination in
				if source == second { throw CocoaError(.fileReadNoPermission) }
				try Data("selected".utf8).write(to: destination)
				try Data(destination.path.utf8).write(to: marker)
			}).value
		XCTAssertTrue(result.isEmpty)
		XCTAssertEqual(counter.count, 2)
		XCTAssertFalse(FileManager.default.fileExists(atPath: try String(contentsOf: marker, encoding: .utf8)))
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

#if os(macOS)
@MainActor
final class MacShareBookmarkTests: XCTestCase {
	func testRenamedSelectionUsesResolvedFile() async throws {
		let fm = FileManager.default
		let dir = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
		try fm.createDirectory(at: dir, withIntermediateDirectories: true)
		defer { try? fm.removeItem(at: dir) }
		let original = dir.appendingPathComponent("original")
		try Data("chosen".utf8).write(to: original)
		let bookmark = try original.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: [.fileResourceIdentifierKey], relativeTo: nil)
		try fm.moveItem(at: original, to: dir.appendingPathComponent("renamed"))
		let selection = PickedShareFile(value: original.path, displayName: "original", securityScopeBookmark: bookmark)
		let result = await MacFileSystemService().withScopedSources([selection]) { sources in
			Result { try Data(contentsOf: URL(fileURLWithPath: XCTUnwrap(sources.first).value)) }
		}
		XCTAssertEqual(try result.get(), Data("chosen".utf8))

		try Data("replacement".utf8).write(to: original)
		let retried = await MacFileSystemService().withScopedSources([selection]) { sources in
			Result { try Data(contentsOf: URL(fileURLWithPath: XCTUnwrap(sources.first).value)) }
		}
		// Different filesystems can still resolve the original after path reuse.
		if case .success(let bytes) = retried { XCTAssertEqual(bytes, Data("chosen".utf8)) }
	}
}
#endif
