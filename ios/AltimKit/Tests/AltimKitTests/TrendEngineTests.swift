import Foundation
import XCTest
@testable import AltimKit

final class TrendEngineTests: XCTestCase {
    let end = 1_800_000_000_000.0

    func testStates() {
        let up = LocalBackendTests.trend(count: 300, step: 86_400_000, end: end, slope: 0.003)
        let v = TrendEngine.verdict(up, kind: .crypto)
        XCTAssertTrue(v.state == .trend || v.state == .pullback)
        XCTAssertTrue(v.action.isBuy)
        XCTAssertGreaterThan(v.momentum!, 0)
        let down = LocalBackendTests.trend(count: 300, step: 86_400_000, end: end, start: 1000, slope: -0.003)
        XCTAssertEqual(TrendEngine.verdict(down, kind: .stock).state, .down)
        XCTAssertEqual(TrendEngine.verdict(down, kind: .stock).action, .sell)
        XCTAssertEqual(TrendEngine.verdict(Array(up.prefix(150)), kind: .crypto).state, .unknown)
        // A sharp 3-day dip inside the rise: the best entry.
        var dip = up
        for k in 1...3 {
            let i = dip.count - 4 + k
            dip[i].close = dip[dip.count - 4].close * (1 - 0.03 * Double(k))
            dip[i].low = min(dip[i].low, dip[i].close)
            dip[i].open = dip[i].close
        }
        XCTAssertEqual(TrendEngine.verdict(dip, kind: .crypto).state, .pullback)
    }

    func testBacktestIsInOnlyInTheTrend() {
        // 300 days up, then 200 days down: the rule exits during the fall, holding does not.
        let up = LocalBackendTests.trend(count: 300, step: 86_400_000, end: end - 200 * 86_400_000, slope: 0.004)
        let top = up.last!.close
        let down = (1...200).map { i -> Bar in
            let p = top * pow(0.995, Double(i))
            return Bar(time: up.last!.time + Double(i) * 86_400_000, open: p / 0.995, high: p / 0.995, low: p, close: p, volume: 1000)
        }
        let r = TrendEngine.backtest(up + down, asset: Asset(symbol: "X", kind: .crypto, name: "X"))
        XCTAssertNil(r.error)
        XCTAssertGreaterThan(r.strategyReturn, r.holdReturn)
        XCTAssertGreaterThan(r.strategyDrawdown, r.holdDrawdown)
        XCTAssertEqual(r.trades.last?.exit, "signal")
    }

    func testRotationPicksTheStrongestInTrend() {
        let assets = (0..<6).map { Asset(symbol: "A\($0)", kind: .crypto, name: "A\($0)") }
        let data = assets.enumerated().map { i, a in
            (a, LocalBackendTests.trend(count: 400, step: 86_400_000, end: end, start: 100, slope: i == 5 ? -0.002 : 0.0005 * Double(i + 1)))
        }
        let picks = Rotation.picks(data, kind: .crypto)
        XCTAssertEqual(picks.filter(\.selected).map(\.symbol), ["A4", "A3", "A2", "A1", "A0"])
        XCTAssertEqual(picks.last?.symbol, "A5")
        XCTAssertEqual(picks.last?.state, "down")
        let t = Rotation.test(data, kind: .crypto)!
        XCTAssertGreaterThan(t.ret, t.basket)
        // Fixed shares: the falling asset's share stays in cash, so the rule beats holding it all.
        let f = Rotation.sleeves(data, kind: .crypto)!
        XCTAssertGreaterThan(f.ret, f.basket)
        XCTAssertGreaterThan(f.drawdown, f.basketDrawdown)
        let back = try? JSONDecoder().decode(RotationReport.self, from: JSONEncoder().encode(RotationReport(kind: .crypto, asOf: 0, top: 3, lookback: 60, picks: picks, scanned: 6, test: t, notes: Rotation.notes)))
        XCTAssertEqual(back?.selected.count, 5)
    }
}

extension TrendEngineTests {
    func testLiveRotation() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "réseau réel non demandé")
        let backend = LocalBackend()
        for kind in [Kind.crypto, .stock] {
            let r = await backend.answer(path: "/api/rotation", query: ["kind": kind.rawValue])!
            let rep = try JSONDecoder().decode(RotationReport.self, from: r.0)
            print("\(kind.rawValue): \(rep.scanned) actifs, sélection \(rep.selected.map(\.symbol))")
            if let t = rep.test { print("  test \(Int(t.ret))% vs \(Int(t.basket))%, moitiés \(Int(t.firstHalf))/\(Int(t.firstHalfBasket)) \(Int(t.secondHalf))/\(Int(t.secondHalfBasket))") }
            for p in rep.picks.prefix(6) { print("  \(p.rank). \(p.symbol) \(p.state): \(p.reason)") }
        }
        let v = await backend.answer(path: "/api/verdict", query: ["symbol": "BTC", "kind": "crypto"])!
        print(String(decoding: v.0, as: UTF8.self))
    }
}

extension TrendEngineTests {
    func testVerdictChangesAreConfigurationTransitions() {
        Money.setDisplay(.usd, nil)
        let a = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
        func v(_ s: TrendEngine.State) -> TrendEngine.Verdict {
            .init(state: s, action: s == .down ? .sell : s == .mixed ? .hold : .buy, score: 0, confidence: 0, momentum: 0, lookback: 60, ema200: 100, rsi2: 50, text: "")
        }
        var (s, t) = ConfigChanges.apply(ConfigState(), verdict: v(.trend), asset: a, now: 1)
        XCTAssertNil(t) // first sight: a baseline, no change
        (s, t) = ConfigChanges.apply(s, verdict: v(.pullback), asset: a, now: 2)
        XCTAssertNil(t) // a pull-back is the same picture
        (s, t) = ConfigChanges.apply(s, verdict: v(.down), asset: a, now: 3)
        XCTAssertEqual(t?.change, "ACHETER / CONSERVER → RESTER À L'ÉCART")
        XCTAssertEqual(t?.triggers.first, "Retour à l'achat quand le prix repasse au-dessus de 100,00\u{a0}$ (moyenne 200 jours) avec un momentum positif")
        XCTAssertEqual(s.transitions.count, 1)
        XCTAssertNil(ConfigChanges.apply(s, verdict: v(.unknown), asset: a, now: 4).transition)
    }
}
