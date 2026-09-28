import Foundation
import XCTest
@testable import AltimKit

/// Same cases as web/test/strategies.test.ts, on the same real /api/strategies answers
/// (backend/tests/samples/strategies-aapl.json and strategies-btc.json).
final class StrategiesTests: XCTestCase {
    func sample(_ name: String) throws -> StrategiesReport {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try JSONDecoder().decode(StrategiesReport.self, from: Data(contentsOf: url))
    }

    let box = Strategies.Box(w: 320, h: 180, l: 34, r: 62, t: 10, b: 18)

    func testEightStrategiesInFixedOrderSameDates() throws {
        for r in [try sample("strategies-aapl"), try sample("strategies-btc")] {
            XCTAssertEqual(r.strategies.map(\.id), Strategies.order)
            for s in r.strategies where s.available {
                XCTAssertLessThanOrEqual(s.equity.count, 200)
                XCTAssertEqual(s.equity.first?.t, r.from)
                XCTAssertEqual(s.equity.last?.t, r.to)
                XCTAssertNotNil(s.metrics)
                XCTAssertGreaterThan(s.rule.count, 20)
            }
            XCTAssertTrue(r.notes.contains { $0.contains("aucune optimisation") })
        }
    }

    func testValueComputedOnStockNotOnCrypto() throws {
        XCTAssertEqual(try sample("strategies-aapl").strategy(.value)?.available, true)
        let v = try XCTUnwrap(try sample("strategies-btc").strategy(.value))
        XCTAssertFalse(v.available)
        XCTAssertTrue(v.unavailable?.contains("pas de bénéfices") == true)
    }

    func testLowSampleFollowsTheTenTradeRule() throws {
        let all = try sample("strategies-aapl").strategies + sample("strategies-btc").strategies
        for s in all where s.metrics != nil {
            if s.id == .buyHold || s.id == .dca { XCTAssertFalse(s.lowSample) } else { XCTAssertEqual(s.lowSample, s.metrics!.trades < 10) }
            XCTAssertEqual(Strategies.lowSampleText(s) != nil, s.lowSample)
        }
    }

    func testDefaultSelectionOwnColours() throws {
        let btc = try sample("strategies-btc")
        XCTAssertEqual(Strategies.drawable(btc, selected: Strategies.defaultSelection).map(\.id), Strategies.defaultSelection)
        XCTAssertEqual(Set(Strategies.order.compactMap { Strategies.colorHex[$0] }).count, Strategies.order.count)
        // Unavailable strategies are never drawn.
        XCTAssertTrue(Strategies.drawable(btc, selected: [.value]).isEmpty)
        // Chips: removed, then added back in the fixed order.
        XCTAssertEqual(Strategies.toggle(.trend, in: Strategies.defaultSelection), [.breakout, .meanReversion, .dca, .buyHold])
        XCTAssertEqual(Strategies.toggle(.momentum, in: [.dca, .trend]), [.trend, .momentum, .dca])
        XCTAssertEqual(Strategies.chosen(btc, selected: [.value, .trend]).map(\.id), [.trend, .value])
    }

    func testSingleAxisKeeps100InsidePointerMapsBack() throws {
        let d = Strategies.drawable(try sample("strategies-aapl"), selected: Strategies.defaultSelection)
        let g = try XCTUnwrap(Strategies.geometry(d, box: box))
        XCTAssertLessThan(g.lo, 100)
        XCTAssertGreaterThan(g.hi, 100)
        XCTAssertEqual(g.y(g.hi), box.t, accuracy: 1e-9)
        XCTAssertEqual(g.y(g.lo), box.h - box.b, accuracy: 1e-9)
        XCTAssertEqual(Strategies.indexAt(g.x(37), g, box: box), 37)
        XCTAssertEqual(Strategies.indexAt(-50, g, box: box), 0)
        XCTAssertEqual(Strategies.indexAt(999, g, box: box), g.n - 1)
        let s = d[0]
        XCTAssertEqual(Strategies.nearestIndex(s.equity, t: s.equity[37].t + 1000), 37)
    }

    func testDirectLabelsNeverOverlapStayInPlot() {
        let ys: [Double] = [50, 52, 51, 170, 171]
        let out = Strategies.placeLabels(ys, gap: 11, top: 14, bottom: 162)
        let sorted = out.sorted()
        for k in 1..<sorted.count { XCTAssertGreaterThanOrEqual(sorted[k] - sorted[k - 1], 11 - 1e-9) }
        XCTAssertLessThanOrEqual(out.max()!, 162)
        XCTAssertGreaterThanOrEqual(out.min()!, 14 - 1e-9)
        // Order preserved: the highest curve keeps the highest label.
        XCTAssertLessThan(out[0], out[3])
    }

    func testListViewCheckpoints() throws {
        let s = try sample("strategies-aapl").strategies[0]
        let c = Strategies.checkpoints(s)
        XCTAssertEqual(c.count, 5)
        XCTAssertEqual(c.first?.t, s.equity.first?.t)
        XCTAssertEqual(c.last?.t, s.equity.last?.t)
    }

    func testFormattingAndQuery() {
        XCTAssertEqual(Strategies.signedPct(12.345), "+12,3\u{202F}%")
        XCTAssertEqual(Strategies.signedPct(-4), "−4,0\u{202F}%")
        XCTAssertEqual(Strategies.signedPct(nil), "—")
        XCTAssertEqual(Strategies.query(symbol: "BRK.B", kind: .stock), ["symbol": "BRK.B", "kind": "stock"])
        let url = AltimClient(baseURL: URL(string: "https://altim.example")!, credentials: nil).request("/api/strategies", query: Strategies.query(symbol: "BRK.B", kind: .stock)).url
        XCTAssertEqual(url?.absoluteString, "https://altim.example/api/strategies?kind=stock&symbol=BRK.B")
        XCTAssertEqual(Strategies.shortDate(1721520000000), "21 juil. 2024")
    }

    /// Same rows as the web's metric cards: TRI and "tout investir au départ" for the DCA, "Non applicable" for the value.
    func testMetricCards() throws {
        let aapl = try sample("strategies-aapl")
        let dca = try XCTUnwrap(aapl.strategy(.dca))
        let rows = Strategies.metricRows(dca)
        XCTAssertTrue(rows.contains { $0.label == "Rendement annuel (TRI)" })
        XCTAssertTrue(rows.contains { $0.label == "Sharpe / Sortino" && $0.value == "non pertinent" })
        XCTAssertFalse(rows.contains { $0.label == "Profit factor" })
        XCTAssertTrue(dca.note?.contains("tout investir au départ") == true)
        let trend = try XCTUnwrap(aapl.strategy(.trend))
        XCTAssertTrue(Strategies.metricRows(trend).contains { $0.label == "Espérance par trade" })
        let value = try XCTUnwrap(try sample("strategies-btc").strategy(.value))
        XCTAssertTrue(Strategies.metricRows(value).isEmpty)
        XCTAssertTrue(value.unavailable?.hasPrefix("Non applicable") == true)
        let regime = Strategies.regimeRow(StrategyRegimeStat(regime: "bear", label: "Marché baissier (x)", trades: 2, winRate: 50, avgReturn: 13.05, lowSample: true), dca: true)
        XCTAssertEqual(regime.label, "Marché baissier")
        XCTAssertEqual(regime.value, "2 achats · moy. +13,1\u{202F}%")
        XCTAssertTrue(regime.lowSample)
    }
}
