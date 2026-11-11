import XCTest

final class BubbleShellUITests: XCTestCase {
    func testSwipeScrollsTheList() throws {
        let app = XCUIApplication(bundleIdentifier: "dev.bubble.shell")
        app.launch()
        // The driver taps the button, injects a dead loop, then remounts.
        // Swipe after the remount so the list is ready for input.
        sleep(3)
        let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35))
        start.press(forDuration: 0.05, thenDragTo: end)
        sleep(1)
    }
}
