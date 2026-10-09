import XCTest
@testable import AltimKit

/// The pure side of the app's motion: same logo, timing and tick rules as the web (frontend/door.js, app.css).
final class MotionTests: XCTestCase {
    func testLogoStrokesFitTheUnitBox() {
        let all = Motion.Logo.peak + Motion.Logo.line + [Motion.Logo.dot]
        XCTAssertTrue(all.allSatisfy { (0...1).contains($0.x) && (0...1).contains($0.y) })
        // The peak's top is the logo's highest point; the rising line ends on the dot.
        XCTAssertEqual(Motion.Logo.peak.map(\.y).min(), Motion.Logo.peak[1].y)
        XCTAssertEqual(Motion.Logo.line.last, Motion.Logo.dot)
    }

    func testLaunchTraceDrawsThePeakThenTheLineThenTheDot() {
        typealias T = Motion.LaunchTrace
        XCTAssertEqual(T.progress(T.peak, at: 0), 0)
        XCTAssertEqual(T.progress(T.peak, at: 0.55), 1)
        XCTAssertEqual(T.progress(T.line, at: 0.3), 0)
        XCTAssertEqual(T.progress(T.line, at: 0.61), 0.5, accuracy: 0.01)
        XCTAssertEqual(T.progress(T.dot, at: 2), 1)
        XCTAssertLessThan(T.dot.upperBound, T.duration)
        // Short: never more than about a second before the app shows.
        XCTAssertLessThanOrEqual(T.duration, 1.2)
    }

    func testTickDirection() {
        XCTAssertEqual(Motion.Tick.between(100, 101), .up)
        XCTAssertEqual(Motion.Tick.between(100, 99.5), .down)
        XCTAssertNil(Motion.Tick.between(100, 100))
        XCTAssertNil(Motion.Tick.between(nil, 100))
        XCTAssertNil(Motion.Tick.between(100, nil))
        XCTAssertNil(Motion.Tick.between(.nan, 100))
    }

    func testEase() {
        XCTAssertEqual(Motion.ease(-1), 0)
        XCTAssertEqual(Motion.ease(0.5), 0.5)
        XCTAssertEqual(Motion.ease(2), 1)
    }
}
