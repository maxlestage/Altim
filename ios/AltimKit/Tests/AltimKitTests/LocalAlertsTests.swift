import Foundation
import XCTest
@testable import AltimKit

final class LocalAlertsTests: XCTestCase {
    let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")

    func signal(_ a: Action, higher: Double = 1) -> LocalSignal {
        LocalSignal(action: a, score: 60, confidence: 72.4, price: 100, time: 0,
                    factors: [SignalFactor(name: "UT supérieure", score: higher, weight: 1.5, detail: "")],
                    stopLoss: 90, takeProfit: 120, hasPlan: true, warnings: [])
    }

    func zone(_ status: String) -> FibZone {
        FibZone(horizon: "medium", label: "Moyen terme", unit: "1 j", holding: "", status: status, levels: [],
                zone: Band(from: 80, to: 95), golden: Band(from: 80, to: 83), invalidation: 70, targets: [120], distance: nil,
                evidence: nil, text: "", macroNote: nil)
    }

    func testStrongBuyNeedsSignalZoneAndReliableData() {
        Money.setDisplay(.usd, nil)
        let high = SignalEngine.reliability(quality: 100, independent: 4, conflict: false)
        let o = LocalAlertRule.evaluate(.init(asset: btc, price: 90, signal: signal(.buy), reliability: high, zones: [zone("inZone")]))
        XCTAssertTrue(o.buy && o.strong)
        XCTAssertEqual(o.key, "signal+zone:medium")
        XCTAssertTrue(o.title.hasPrefix("BTC : achat conseillé à 90"))
        XCTAssertTrue(o.body.contains(LocalAlertRule.noGuard))
        XCTAssertTrue(o.body.hasSuffix("Invalidé sous 70,00\u{a0}$."))
        // A zone alone: possible, not advised.
        let z = LocalAlertRule.evaluate(.init(asset: btc, price: 90, signal: signal(.hold), reliability: high, zones: [zone("golden")]))
        XCTAssertTrue(z.buy && !z.strong)
        XCTAssertTrue(z.reasons[0].hasPrefix("Prix dans la zone d'or moyen terme"))
    }

    func testBlockersWithoutTheServer() {
        let medium = SignalEngine.reliability(quality: 100, independent: 2, conflict: false)
        let m = LocalAlertRule.evaluate(.init(asset: btc, price: 90, signal: signal(.strongBuy), reliability: medium, zones: []))
        XCTAssertFalse(m.buy)
        XCTAssertTrue(m.body.hasPrefix("Fiabilité moyenne"))
        XCTAssertEqual(m.title, "BTC : pas d'achat pour l'instant")
        let high = SignalEngine.reliability(quality: 100, independent: 5, conflict: false)
        let down = LocalAlertRule.evaluate(.init(asset: btc, price: 90, signal: signal(.buy, higher: -1), reliability: high, zones: []))
        XCTAssertTrue(down.cautions.contains("Tendance de fond baissière : taille réduite."))
        let nothing = LocalAlertRule.evaluate(.init(asset: btc, price: 120, signal: signal(.hold), reliability: high, zones: [zone("above")]))
        XCTAssertEqual(nothing.body, "Ni signal d'achat ni prix dans une zone d'achat.")
    }

    func testRouteJsonIsWhatTheAppDecodes() async throws {
        let r = await LocalBackend(fetcher: FakeFetcher(routes: [])).answer(path: "/api/alerts", query: ["symbols": "BTC:crypto"])!
        let items = try JSONDecoder().decode([BuyAlert].self, from: r.0)
        XCTAssertEqual(items.count, 1)
        XCTAssertFalse(items[0].buy)
        let o = LocalAlertRule.evaluate(.init(asset: btc, price: 90, signal: signal(.buy), reliability: SignalEngine.reliability(quality: 100, independent: 3, conflict: false), zones: []))
        let data = try JSONSerialization.data(withJSONObject: [LocalAlertRule.json(btc, o, price: 90, asOf: 1)])
        let decoded = try JSONDecoder().decode([BuyAlert].self, from: data)
        XCTAssertEqual(decoded.first?.key, "signal")
        XCTAssertEqual(decoded.first?.buy, true)
    }
}
