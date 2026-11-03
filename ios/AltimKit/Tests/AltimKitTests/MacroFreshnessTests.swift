import XCTest
@testable import AltimKit

/// A macro series stuck on old closes is left out, like a source that did not answer.
final class MacroFreshnessTests: XCTestCase {
    func testStaleSeriesLeftOut() {
        let now = 1_800_000_000_000.0
        let fresh = [MacroEngine.Point(time: now - 3 * 86_400_000, close: 18)]
        let stale = [MacroEngine.Point(time: now - 9 * 86_400_000, close: 40)]
        let kept = MacroEngine.fresh(["vix": fresh, "spx": stale, "dxy": []], now: now)
        XCTAssertEqual(Set(kept.keys), ["vix"], "VIX à jour gardé ; S&P figé depuis 9 jours et série vide écartés")
    }
}
