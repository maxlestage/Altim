import XCTest
@testable import AltimKit

/// « Marchés » widget and Point du jour: the market filter's state from the benchmark's daily snapshot.
final class MarketBoardTests: XCTestCase {
    func snap(_ bars: [Bar], level: String = "high") -> Snapshot {
        Snapshot(symbol: "BTC", kind: .crypto, interval: "1d",
                 candles: bars.map { Candle(time: $0.time, open: $0.open, high: $0.high, low: $0.low, close: $0.close, volume: $0.volume) },
                 source: "test", sources: [], agreeing: 3, reliability: Reliability(score: level == "low" ? 30 : 90, level: level, independent: 3, conflict: false))
    }

    func testStates() {
        let now = 1_800_000_000_000.0
        let up = LocalBackendTests.trend(count: 400, step: 86_400_000, end: now - 3_600_000, slope: 0.003)
        let down = LocalBackendTests.trend(count: 400, step: 86_400_000, end: now - 3_600_000, slope: -0.003)
        XCTAssertEqual(MarketBoard.state(snap(up), kind: .crypto, now: now), true)
        XCTAssertEqual(MarketBoard.state(snap(down), kind: .crypto, now: now), false)
        XCTAssertNil(MarketBoard.state(snap(up, level: "low"), kind: .crypto, now: now), "sources en désaccord : inconnu")
        // Last candle one day old already: +3 days makes it 4 days old.
        XCTAssertNil(MarketBoard.state(snap(up), kind: .crypto, now: now + 3 * 86_400_000), "Bitcoin vieux de plus de 3 jours : inconnu")
        XCTAssertEqual(MarketBoard.state(snap(up), kind: .stock, now: now + 3 * 86_400_000), true, "SPY : un long week-end reste valable")
        XCTAssertNil(MarketBoard.state(nil, kind: .stock, now: now))
    }

    func testTexts() {
        XCTAssertEqual(MarketBoard.shortText(true), "Haussier : achats permis")
        XCTAssertEqual(MarketBoard.shortText(false), "Hors tendance : pas d'achat, sortir")
        XCTAssertEqual(MarketBoard.shortText(nil), "Inconnu : filtre non appliqué")
        XCTAssertTrue(MarketBoard.text(.stock, false).hasPrefix("Marché actions (S&P 500) hors tendance haussière"))
    }

    /// Offline: both unknown, never a made-up state.
    func testReadOffline() async {
        let c = AltimClient.standalone(local: LocalBackend(fetcher: FakeFetcher(routes: [])))
        let s = await MarketBoard.read(c)
        XCTAssertEqual(s.count, 2)
        XCTAssertTrue(s.values.allSatisfy { $0 == nil })
    }
}
