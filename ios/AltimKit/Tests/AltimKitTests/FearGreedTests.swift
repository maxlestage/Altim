import XCTest
@testable import AltimKit

/// Fear & Greed (alternative.me): values within 0–100 only, and nothing from a feed stuck on an old reading.
final class FearGreedTests: XCTestCase {
    func page(newest: Double, values: [String]) -> Any {
        ["data": values.enumerated().map { i, v in ["value": v, "timestamp": String(Int(newest) - i * 86_400)] }]
    }

    func testParse() {
        let now = 1_800_000_000_000.0
        let fresh = page(newest: now / 1000 - 3_600, values: ["72", "65", "140", "abc", "40"])
        XCTAssertEqual(LocalBackend.parseFearGreed(fresh, now: now), [40, 65, 72], "plus ancien d'abord, valeurs hors 0–100 écartées")
        let stale = page(newest: now / 1000 - 5 * 86_400, values: ["72", "65"])
        XCTAssertEqual(LocalBackend.parseFearGreed(stale, now: now), [], "dernière valeur vieille de 5 jours : rien")
        XCTAssertEqual(LocalBackend.parseFearGreed(["data": []], now: now), [])
    }

    /// OKX funding rate: a plausible value is kept, an absurd one (read error) is dropped.
    func testFunding() {
        XCTAssertEqual(LocalBackend.parseFunding(["data": [["fundingRate": "0.0001"]]]), 0.0001)
        XCTAssertEqual(LocalBackend.parseFunding(["data": [["fundingRate": "-0.003"]]]), -0.003)
        XCTAssertNil(LocalBackend.parseFunding(["data": [["fundingRate": "0.8"]]]), "80 % par 8 h : erreur de lecture")
        XCTAssertNil(LocalBackend.parseFunding(["data": []]))
    }
}
