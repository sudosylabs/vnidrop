import XCTest
@testable import VniDrop

final class CoreDispatcherTests: XCTestCase {

	/// Regression guard for the receive-cancel deadlock: an interrupt-lane call
	/// must complete even while the serial lane is occupied by a blocking call.
	/// With a single shared queue (the old design) the interrupt would be stuck
	/// behind the blocked `receive`, and this would time out.
	func testInterruptCompletesWhileSerialLaneIsBlocked() async {
		let dispatcher = CoreDispatcher()
		let serialEntered = DispatchSemaphore(value: 0)
		let releaseSerial = DispatchSemaphore(value: 0)

		// Occupy the serial lane with a call that blocks until we release it.
		let serialTask = Task {
			await dispatcher.run {
				serialEntered.signal()
				releaseSerial.wait()
			}
		}
		XCTAssertEqual(serialEntered.wait(timeout: .now() + 2), .success, "serial lane never started")

		// The interrupt lane must run despite the serial lane being blocked.
		let interruptDone = DispatchSemaphore(value: 0)
		Task.detached {
			_ = await dispatcher.runInterrupt { 42 }
			interruptDone.signal()
		}
		XCTAssertEqual(
			interruptDone.wait(timeout: .now() + 2), .success,
			"interrupt lane was blocked behind the occupied serial lane")

		releaseSerial.signal()
		_ = await serialTask.value
	}

	func testRunPropagatesValuesAndErrors() async {
		let dispatcher = CoreDispatcher()

		let value = await dispatcher.run { 7 }
		XCTAssertEqual(try? value.get(), 7)

		let failure = await dispatcher.run { () -> Int in throw TestError.unimplemented }
		switch failure {
		case .success: XCTFail("expected the thrown error to propagate")
		case .failure(let error): XCTAssertTrue(error is TestError)
		}
	}
}
