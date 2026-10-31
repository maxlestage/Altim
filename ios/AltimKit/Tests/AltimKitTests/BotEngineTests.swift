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

    /// The risk bot learns the worst fall ahead: a calm rise must look less risky than a jumpy fall.
    func testRiskBotLearnsTheFall() throws {
        func jumpy(_ seed: Double) -> [Bar] {
            var p = 100.0
            return (0..<700).map { i in
                p *= 1 - 0.001 + 0.05 * sin(Double(i) * 1.7 + seed)
                return Bar(time: Double(i) * 86_400_000, open: p, high: p * 1.04, low: p * 0.94, close: p, volume: 1000)
            }
        }
        let calm = (0..<4).map { Self.series(drift: 0.001, seed: Double($0)) }
        let wild = (0..<4).map { jumpy(Double($0)) }
        let model = try XCTUnwrap(BotEngine.Model.train(calm + wild, target: .fall(days: 20)))
        let quiet = try XCTUnwrap(model.predict(Self.series(drift: 0.001, seed: 2.2)))
        let risky = try XCTUnwrap(model.predict(jumpy(9)))
        XCTAssertLessThanOrEqual(quiet, 0)
        XCTAssertLessThan(risky, quiet, "une série agitée doit annoncer une baisse plus forte")
        XCTAssertEqual(BotEngine.bots.map(\.name), ["Bot Tendance", "Bot Risque"])
    }

    /// A trained bot survives a restart: read back from its folder, it predicts the same.
    func testModelIsKeptBetweenLaunches() throws {
        let model = try XCTUnwrap(BotEngine.Model.train((0..<6).map { Self.series(drift: $0 % 2 == 0 ? 0.002 : -0.002, seed: Double($0)) }))
        let back = try JSONDecoder().decode(BotEngine.Model.self, from: JSONEncoder().encode(model))
        let probe = Self.series(drift: 0.001, seed: 4)
        XCTAssertEqual(back.predict(probe), model.predict(probe))
        XCTAssertEqual(back.examples, model.examples)
    }

    func testRiskSizeInTheBuyAlert() {
        XCTAssertEqual(BotEngine.size(forFall: -0.25), 20)
        XCTAssertEqual(BotEngine.size(forFall: -0.02), 100)
        XCTAssertNil(BotEngine.size(forFall: nil))
        var v = TrendEngine.verdict(Self.series(drift: 0.002), kind: .crypto)
        v.botRisk = -0.25
        let o = LocalAlertRule.evaluate(.init(asset: Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), price: 100, verdict: v,
                                              reliability: nil, zones: []))
        XCTAssertTrue(o.buy)
        XCTAssertTrue(o.cautions.contains("Bot Risque : pire baisse prévue −25 % sur 20 jours, engager environ 20 % de la somme prévue."), "\(o.cautions)")
    }

    /// The radar row carries the risk bot's fall; an older cached row without it still reads.
    func testRadarRowCarriesTheRisk() throws {
        let old = try JSONDecoder().decode(RadarRow.self, from: Data(#"{"symbol":"BTC","kind":"crypto"}"#.utf8))
        XCTAssertNil(old.botRisk)
        let row = try JSONDecoder().decode(RadarRow.self, from: Data(#"{"symbol":"BTC","kind":"crypto","botRisk":-0.18}"#.utf8))
        XCTAssertEqual(row.botRisk, -0.18)
    }
}
