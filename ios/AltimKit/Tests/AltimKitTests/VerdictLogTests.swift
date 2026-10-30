import Foundation
import XCTest
@testable import AltimKit

final class VerdictLogTests: XCTestCase {
    let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")

    func v(_ s: TrendEngine.State, _ price: Double) -> TrendEngine.Verdict {
        .init(state: s, action: s == .down ? .sell : s == .mixed ? .hold : .buy, score: 0, confidence: 0, momentum: 0, lookback: 60,
              ema200: 1, rsi2: 50, text: "", price: price)
    }

    func testOnlyChangesAreLoggedAndMeasured() {
        var log: [VerdictLogEntry] = []
        log = VerdictLog.record(log, v(.trend, 100), asset: btc, now: 1)
        log = VerdictLog.record(log, v(.pullback, 95), asset: btc, now: 2) // same advice
        log = VerdictLog.record(log, v(.down, 120), asset: btc, now: 3)
        log = VerdictLog.record(log, v(.unknown, 130), asset: btc, now: 4) // no verdict
        XCTAssertEqual(log.map(\.side), ["buy", "out"])
        let o = VerdictLog.outcomes(log, prices: ["crypto:BTC": 90])
        XCTAssertEqual(o[0].change!, 20, accuracy: 1e-9) // 100 → 120 at the next verdict
        XCTAssertTrue(o[0].ended)
        XCTAssertEqual(o[1].change!, -25, accuracy: 1e-9) // 120 → 90 now: a fall avoided
        XCTAssertFalse(o[1].ended)
        let s = VerdictLog.summary(o)
        XCTAssertEqual(s.buyCount, 1)
        XCTAssertEqual(s.text, "Après « acheter / conserver » : +20,0 % en moyenne (1 cas). Après « rester à l'écart » : −25,0 % en moyenne (1 cas), une baisse évitée.")
        XCTAssertTrue(VerdictLog.summary([]).text.hasPrefix("Pas encore de verdict suivi"))
    }

    func testVerdictCarriesItsPrice() {
        let bars = LocalBackendTests.trend(count: 300, step: 86_400_000, end: 1_800_000_000_000, slope: 0.003)
        XCTAssertEqual(TrendEngine.verdict(bars, kind: .crypto).price, bars.last!.close)
    }
}
