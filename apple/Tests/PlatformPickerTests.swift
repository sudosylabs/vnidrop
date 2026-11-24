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
	func testPickingAnIosDirectoryRetainsAccessUntilTheSelectionIsReleased() {
		let counter = ScopeReleaseCounter()
		let folder = URL(fileURLWithPath: "/tmp/picked-folder", isDirectory: true)
		var picked = PickerSupport.pickedFile(from: folder, isDirectory: true) { url in
			XCTAssertEqual(url, folder)
			return { counter.release() }
		}

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
