import Foundation
import XCTest
@testable import AltimKit

final class LocalAlertsTests: XCTestCase {
    let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
    let high = SignalEngine.reliability(quality: 100, independent: 4, conflict: false)

    func verdict(_ s: TrendEngine.State) -> TrendEngine.Verdict {
        TrendEngine.Verdict(state: s, action: s == .pullback ? .strongBuy : s == .trend ? .buy : s == .down ? .sell : .hold, score: 40,
                            confidence: 100, momentum: 0.2, lookback: 60, ema200: 70, rsi2: 5, text: "Tendance de fond haussière.")
    }

    func zone(_ status: String) -> FibZone {
        FibZone(horizon: "medium", label: "Moyen terme", unit: "1 j", holding: "", status: status, levels: [],
                zone: Band(from: 80, to: 95), golden: Band(from: 80, to: 83), invalidation: 75, targets: [120], distance: nil,
                evidence: nil, text: "", macroNote: nil)
    }

    func testBuyNeedsTheTrend() {
        Money.setDisplay(.usd, nil)
        let p = LocalAlertRule.evaluate(.init(asset: btc, price: 90, verdict: verdict(.pullback), reliability: high, zones: []))
        XCTAssertTrue(p.buy && p.strong)
        XCTAssertEqual(p.key, "repli")
        XCTAssertTrue(p.title.hasPrefix("BTC : achat conseillé à 90"))
        XCTAssertTrue(p.body.hasSuffix("Sortie si le prix repasse sous 70,00\u{a0}$ (moyenne 200 jours) ou si le momentum devient négatif."))
        let t = LocalAlertRule.evaluate(.init(asset: btc, price: 90, verdict: verdict(.trend), reliability: high, zones: []))
        XCTAssertTrue(t.buy && !t.strong)
        XCTAssertTrue(t.cautions.contains("Pas de repli en cours : entrer en plusieurs fois."))
        // A buy zone makes it advised, only inside the trend.
        let tz = LocalAlertRule.evaluate(.init(asset: btc, price: 90, verdict: verdict(.trend), reliability: high, zones: [zone("inZone")]))
        XCTAssertTrue(tz.strong)
        XCTAssertEqual(tz.key, "tendance+zone:medium")
        let dz = LocalAlertRule.evaluate(.init(asset: btc, price: 90, verdict: verdict(.down), reliability: high, zones: [zone("golden")]))
        XCTAssertFalse(dz.buy)
        XCTAssertTrue(dz.body.hasPrefix("Tendance de fond baissière"))
    }

    func testBlockers() {
        let low = SignalEngine.reliability(quality: 100, independent: 1, conflict: false)
        XCTAssertFalse(LocalAlertRule.evaluate(.init(asset: btc, price: 90, verdict: verdict(.trend), reliability: low, zones: [])).buy)
        XCTAssertFalse(LocalAlertRule.evaluate(.init(asset: btc, price: 90, verdict: verdict(.trend), reliability: high, zones: [], shock: "shock")).buy)
        XCTAssertFalse(LocalAlertRule.evaluate(.init(asset: btc, price: 90, verdict: verdict(.mixed), reliability: high, zones: [])).buy)
        let ag = LocalAlertRule.evaluate(.init(asset: btc, price: 90, verdict: verdict(.trend), reliability: high, zones: [], shock: "agitated"))
        XCTAssertTrue(ag.buy && ag.cautions.contains("Marché agité : taille réduite, stop plus large."))
    }

    func testRouteJsonIsWhatTheAppDecodes() async throws {
        let r = await LocalBackend(fetcher: FakeFetcher(routes: [])).answer(path: "/api/alerts", query: ["symbols": "BTC:crypto"])!
        let items = try JSONDecoder().decode([BuyAlert].self, from: r.0)
        XCTAssertEqual(items.count, 1)
        XCTAssertFalse(items[0].buy)
        let o = LocalAlertRule.evaluate(.init(asset: btc, price: 90, verdict: verdict(.trend), reliability: high, zones: []))
        let decoded = try JSONDecoder().decode([BuyAlert].self, from: JSONSerialization.data(withJSONObject: [LocalAlertRule.json(btc, o, price: 90, asOf: 1)]))
        XCTAssertEqual(decoded.first?.key, "tendance")
        XCTAssertEqual(decoded.first?.buy, true)
    }
}
