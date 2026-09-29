import XCTest
@testable import AltimKit

/// `JSFormat.fr` against the browser's `toLocaleString("fr-FR")` (values printed by Bun on the same inputs).
final class JSFormatTests: XCTestCase {
    func testRoundsTheShortestDecimalLikeIntl() {
        XCTAssertEqual(JSFormat.fr(86.55, max: 1), "86,6")
        XCTAssertEqual(JSFormat.fr(9.01 - 95.56, max: 1), "-86,6")
        XCTAssertEqual(JSFormat.fr(1.005, max: 2), "1,01")
        XCTAssertEqual(JSFormat.fr(0.125, max: 2), "0,13")
        XCTAssertEqual(JSFormat.fr(2.675, max: 2), "2,68")
    }

    func testVerySmallAndVeryLargeNumbers() {
        XCTAssertEqual(JSFormat.fr(0.00001, max: 6), "0,00001")
        XCTAssertEqual(JSFormat.fr(2.5e20, max: 0), "250\u{202F}000\u{202F}000\u{202F}000\u{202F}000\u{202F}000\u{202F}000")
        XCTAssertEqual(JSFormat.fr(1_234_567.891, max: 2), "1\u{202F}234\u{202F}567,89")
    }
}
