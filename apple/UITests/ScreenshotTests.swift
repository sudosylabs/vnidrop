import XCTest

/// Captures App Store screenshots by driving the running app and grabbing the
/// full-screen image on each tab. Run via `apple/scripts/appstore-screenshots.sh`,
/// which boots the target simulator, runs this test, and extracts the attachments
/// at the device's native resolution (e.g. 2064×2752 on the 13-inch iPad).
final class ScreenshotTests: XCTestCase {

	override func setUp() {
		continueAfterFailure = false
	}

	func testCaptureAppStoreScreenshots() {
		let app = XCUIApplication()
		app.launch()

		// The top pill exposes the three primary tabs. Tap by accessibility label
		// when available, falling back to a normalized coordinate on the pill.
		let tabs: [(name: String, dx: CGFloat)] = [
			("01-Send", 0.407),
			("02-Receive", 0.487),
			("03-Settings", 0.579),
		]

		for tab in tabs {
			let label = String(tab.name.dropFirst(3)) // "Send" / "Receive" / "Settings"
			let button = app.buttons[label]
			if button.waitForExistence(timeout: 10), button.isHittable {
				button.tap()
			} else {
				app.coordinate(withNormalizedOffset: CGVector(dx: tab.dx, dy: 0.039)).tap()
			}

			// Let the tab transition and any content settle before capturing.
			Thread.sleep(forTimeInterval: 1.5)

			let screenshot = XCUIScreen.main.screenshot()
			let attachment = XCTAttachment(screenshot: screenshot)
			attachment.name = tab.name
			attachment.lifetime = .keepAlways
			add(attachment)
		}
	}
}
