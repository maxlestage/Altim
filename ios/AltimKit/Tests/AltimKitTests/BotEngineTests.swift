import XCTest
@testable import AltimKit

final class BotEngineTests: XCTestCase {
    /// Daily candles: a steady rise or fall with waves.
    static func series(days: Int = 700, drift: Double, start: Double = 100, seed: Double = 0) -> [Bar] {
        var p = start
        return (0..<days).map { i in
            p *= 1 + drift + 0.01 * sin(Double(i) / 4 + seed)
            return Bar(time: Double(i) * 86_400_000, open: p * 0.999, high: p * 1.01, low: p * 0.99, close: p, volume: 1000 + Double(i % 9) * 30)
        }
    }

    func testModelLearnsFromHistories() throws {
        let ups = (0..<4).map { Self.series(drift: 0.002, seed: Double($0)) }
        let downs = (0..<4).map { Self.series(drift: -0.002, seed: Double($0) + 10) }
        let model = try XCTUnwrap(BotEngine.Model.train(ups + downs))
        XCTAssertEqual(model.assets, 8)
        XCTAssertGreaterThan(model.examples, 3000)
        let up = try XCTUnwrap(model.predict(Self.series(drift: 0.002, seed: 3.3)))
        let down = try XCTUnwrap(model.predict(Self.series(drift: -0.002, seed: 7.7)))
        XCTAssertGreaterThan(up, down, "une hausse régulière doit être mieux notée qu'une baisse")
        XCTAssertNil(model.predict(Array(Self.series(drift: 0.002).prefix(150))), "moins de 200 jours : pas de prévision")
    }

    func testVetoNeverCreatesAPurchase() {
        let v = TrendEngine.verdict(Self.series(drift: 0.002), kind: .crypto)
        XCTAssertTrue(v.action == .buy || v.action == .strongBuy)
        XCTAssertEqual(BotEngine.apply(0.05, to: v).action, v.action, "prévision positive : verdict inchangé")
        XCTAssertEqual(BotEngine.apply(0.05, to: v).bot, 0.05)
        XCTAssertEqual(BotEngine.apply(-0.005, to: v).action, v.action, "au-dessus de −1 % : inchangé")
        let hold = BotEngine.apply(-0.02, to: v)
        XCTAssertEqual(hold.action, .hold)
        XCTAssertTrue(hold.text.hasPrefix("Le bot entraîné prévoit −2,0 % sur 20 jours : conserver"))
        XCTAssertEqual(BotEngine.apply(-0.05, to: v).action, .sell)
        XCTAssertEqual(BotEngine.apply(nil, to: v).action, v.action, "sans bot : la règle seule")
        let down = TrendEngine.verdict(Self.series(drift: -0.002), kind: .crypto)
        XCTAssertEqual(BotEngine.apply(0.2, to: down).action, down.action, "le bot ne transforme jamais une baisse en achat")
    }

    func testVetoBlocksTheBuyAlert() {
        let v = BotEngine.apply(-0.02, to: TrendEngine.verdict(Self.series(drift: 0.002), kind: .crypto))
        let o = LocalAlertRule.evaluate(.init(asset: Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), price: 100, verdict: v,
                                              reliability: nil, zones: []))
        XCTAssertFalse(o.buy)
        XCTAssertEqual(o.blockers.first, "Tendance haussière, mais le bot entraîné prévoit −2,0 % sur 20 jours : pas d'achat.")
    }
}
