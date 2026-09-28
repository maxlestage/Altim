import Foundation
import XCTest
@testable import AltimKit

private let t0 = 1_788_271_200_000.0 // Date.UTC(2026, 8, 1, 14, 0)
private func utcDay(_ d: Int) -> Double { 1_788_220_800_000 + Double(d - 1) * 86_400_000 } // Date.UTC(2026, 8, d)
private func candle(_ time: Double, _ open: Double, _ high: Double, _ low: Double, _ close: Double) -> Candle {
    Candle(time: time, open: open, high: high, low: low, close: close, volume: nil)
}

/// Same cases as the site (web/test/paper.test.ts) and the shared reference scenario (paper-fixture.json).
final class PaperTests: XCTestCase {
    private func order(_ id: String, _ symbol: String, _ kind: Kind, _ name: String, price: Double, amount: Double,
                       stop: Double? = nil, target: Double? = nil) -> PaperOrder {
        PaperOrder(id: id, symbol: symbol, kind: kind, name: name, price: price, amount: amount, stop: stop, target: target)
    }

    func testRoundLikeJavaScript() {
        // Math.round: halves toward +∞ (Swift's .rounded() would give −3 and 1 for the first and last).
        XCTAssertEqual(Paper.jsRound(-2.5), -2)
        XCTAssertEqual(Paper.jsRound(2.5), 3)
        XCTAssertEqual(Paper.jsRound(-0.5), 0)
        XCTAssertEqual(Paper.jsRound(0.49999999999999994), 0)
        XCTAssertEqual(Paper.jsRound(-1.4), -1)
        XCTAssertEqual(Paper.round(-0.125, 2), -0.12)
        XCTAssertEqual(Paper.round(1.005, 2), 1) // 1.005 × 100 = 100.49999999999999 in binary, like the web
        XCTAssertEqual(Paper.jsString(10_000), "10000")
        XCTAssertEqual(Paper.jsString(1_234.5), "1234.5")
        XCTAssertEqual(Paper.jsString(0.07), "0.07")
    }

    func testBuyPaysFeesAndSlippage() throws {
        let r = Paper.open(Paper.new(capital: 10_000, now: t0), order("a", "BTC", .crypto, "Bitcoin", price: 64_000, amount: 3_000, stop: 60_000, target: 72_000), now: t0)
        XCTAssertNil(r.error)
        let p = try XCTUnwrap(r.state.positions.first)
        XCTAssertEqual(p.entry, 64_000 * (1 + Paper.slippage), accuracy: 1e-6)
        XCTAssertEqual(p.quantity, (3_000 * (1 - Paper.feeRate)) / (64_000 * 1.0005), accuracy: 1e-9)
        XCTAssertEqual(r.state.cash, 7_000)
    }

    func testTargetReachedNextDay() throws {
        var s = Paper.open(Paper.new(capital: 10_000, now: t0), order("a", "BTC", .crypto, "Bitcoin", price: 64_000, amount: 3_000, stop: 60_000, target: 72_000), now: t0).state
        // The opening day (low 59 000 < stop) is not used: its low may be before the purchase.
        s = Paper.checkExits(s, candles: ["crypto:BTC": [candle(utcDay(1), 63_000, 65_000, 59_000, 64_500)]]).state
        XCTAssertEqual(s.positions.count, 1)
        let r = Paper.checkExits(s, candles: ["crypto:BTC": [candle(utcDay(3), 70_000, 73_000, 69_000, 72_500)]])
        let t = try XCTUnwrap(r.closed.first)
        XCTAssertEqual(t.reason, .target)
        XCTAssertEqual(t.exit, 72_000 * 0.9995, accuracy: 1e-6)
        let qty = (3_000 * 0.999) / (64_000 * 1.0005)
        XCTAssertEqual(t.proceeds, qty * 71_964 * 0.999, accuracy: 0.005)
        XCTAssertEqual(t.pnl, 364.89)
    }

    func testGapBelowStopAndStopFirst() throws {
        var s = Paper.open(Paper.new(capital: 10_000, now: t0), order("e", "ETH", .crypto, "Ethereum", price: 2_500, amount: 1_000, stop: 2_300, target: 3_000), now: t0).state
        var r = Paper.checkExits(s, candles: ["crypto:ETH": [candle(utcDay(2), 2_200, 2_250, 2_150, 2_210)]])
        XCTAssertEqual(try XCTUnwrap(r.closed.first).exit, 2_200 * 0.9995, accuracy: 1e-6)
        s = Paper.open(Paper.new(capital: 10_000, now: t0), order("b", "BTC", .crypto, "Bitcoin", price: 70_000, amount: 2_000, stop: 66_000, target: 76_000), now: t0).state
        r = Paper.checkExits(s, candles: ["crypto:BTC": [candle(utcDay(2), 70_000, 77_000, 65_000, 71_000)]])
        XCTAssertEqual(try XCTUnwrap(r.closed.first).reason, .stop)
    }

    func testRefusals() throws {
        let s = Paper.new(capital: 1_000, now: t0)
        let o = order("x", "AAPL", .stock, "Apple", price: 250, amount: 500)
        var big = o; big.amount = 1_500
        XCTAssertTrue(try XCTUnwrap(Paper.open(s, big, now: t0).error).contains("insuffisantes"))
        var nan = o; nan.price = .nan
        XCTAssertEqual(Paper.open(s, nan, now: t0).error, "Prix indisponible.")
        var negative = o; negative.amount = -1
        XCTAssertEqual(Paper.open(s, negative, now: t0).error, "Montant invalide.")
        var high = o; high.stop = 260
        XCTAssertNil(try XCTUnwrap(Paper.open(s, high, now: t0).state.positions.first).stop)
        XCTAssertEqual(Paper.close(s, id: "nope", price: 10, now: t0).error, "Position introuvable.")
    }

    func testValuationAsIfSoldNow() throws {
        let s = Paper.open(Paper.new(capital: 10_000, now: t0), order("a", "SOL", .crypto, "Solana", price: 100, amount: 1_000), now: t0).state
        let v = Paper.valuation(s, prices: ["crypto:SOL": 110])
        let qty = 999 / 100.05
        let value = try XCTUnwrap(v.lines.first?.value)
        XCTAssertEqual(value, qty * 110 * 0.9995 * 0.999, accuracy: 0.005)
        XCTAssertEqual(v.equity, 9_000 + value, accuracy: 0.005)
        let none = Paper.valuation(s, prices: [:])
        XCTAssertEqual(none.unpriced, 1)
        XCTAssertEqual(none.equity, 10_000)
    }

    func testEmptyStats() {
        let st = Paper.stats(Paper.new(capital: 1_000, now: t0))
        XCTAssertEqual(st.trades, 0)
        XCTAssertNil(st.profitFactor)
        XCTAssertEqual(st.maxDrawdownPct, 0)
        XCTAssertEqual(st.winRate, 0)
        XCTAssertNil(st.best)
    }

    func testSavedStateChecked() throws {
        let good = try JSONEncoder().encode(Paper.new(capital: 5_000, now: t0))
        XCTAssertNotNil(Paper.decode(good))
        XCTAssertNil(Paper.decode(Data(#"{"version":1,"startCapital":1000,"cash":1000,"startedAt":0,"positions":[{"id":1}],"trades":[]}"#.utf8)))
        XCTAssertNil(Paper.decode(Data("null".utf8)))
        XCTAssertNil(Paper.decode(Data(#"{"version":2,"startCapital":1000,"cash":1000,"startedAt":0,"positions":[],"trades":[]}"#.utf8)))
        XCTAssertNil(Paper.decode(Data(#"{"version":1,"startCapital":0,"cash":1000,"startedAt":0,"positions":[],"trades":[]}"#.utf8)))
        var s = Paper.open(Paper.new(capital: 1_000, now: t0), order("a", "SOL", .crypto, "Solana", price: 100, amount: 100), now: t0).state
        XCTAssertTrue(Paper.isValid(s))
        s.positions[0].quantity = 0
        XCTAssertFalse(Paper.isValid(s))
        // A saved state is read back identically (nulls kept).
        let r = Paper.open(Paper.new(capital: 1_000, now: t0), order("b", "BTC", .crypto, "Bitcoin", price: 100, amount: 100, target: 120), now: t0).state
        XCTAssertEqual(Paper.decode(try JSONEncoder().encode(r)), r)
    }

    // MARK: Reference scenario (shared with the web and Android)

    private struct Step: Decodable {
        var op: String
        var capital: Double?
        var now: Double?
        var order: PaperOrder?
        var id: String?
        var price: Double?
        var candles: [String: [Candle]]?
        var prices: [String: Double?]?
    }

    private struct Entry: Decodable { var step: Step }

    private func json<T: Encodable>(_ v: T) throws -> Any {
        try JSONSerialization.jsonObject(with: JSONEncoder().encode(v), options: [.fragmentsAllowed])
    }

    /// Deep comparison: numbers within 1e-6, a missing key equal to null.
    private func assertSame(_ a: Any?, _ b: Any?, _ path: String, file: StaticString = #filePath, line: UInt = #line) {
        let a = a is NSNull ? nil : a
        let b = b is NSNull ? nil : b
        switch (a, b) {
        case (nil, nil):
            return
        case let (x as String, y as String):
            XCTAssertEqual(x, y, path, file: file, line: line)
        case let (x as [String: Any], y as [String: Any]):
            for k in Set(x.keys).union(y.keys) { assertSame(x[k], y[k], "\(path).\(k)", file: file, line: line) }
        case let (x as [Any], y as [Any]):
            XCTAssertEqual(x.count, y.count, "\(path) count", file: file, line: line)
            for (i, (u, v)) in zip(x, y).enumerated() { assertSame(u, v, "\(path)[\(i)]", file: file, line: line) }
        case let (x as NSNumber, y as NSNumber):
            XCTAssertEqual(x.doubleValue, y.doubleValue, accuracy: 1e-6, path, file: file, line: line)
        default:
            XCTFail("\(path): \(String(describing: a)) ≠ \(String(describing: b))", file: file, line: line)
        }
    }

    func testReferenceScenario() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "paper-fixture", withExtension: "json", subdirectory: "Fixtures"))
        let data = try Data(contentsOf: url)
        let entries = try JSONDecoder().decode([Entry].self, from: data)
        let raw = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [[String: Any]])
        XCTAssertEqual(entries.count, 15)
        var s = Paper.new(capital: 10_000, now: t0)
        for (i, e) in entries.enumerated() {
            let step = e.step
            var got: [String: Any] = [:]
            switch step.op {
            case "new":
                s = Paper.new(capital: try XCTUnwrap(step.capital), now: try XCTUnwrap(step.now))
            case "open":
                let r = Paper.open(s, try XCTUnwrap(step.order), now: try XCTUnwrap(step.now))
                s = r.state
                if let error = r.error { got["error"] = error }
            case "close":
                let r = Paper.close(s, id: try XCTUnwrap(step.id), price: try XCTUnwrap(step.price), now: try XCTUnwrap(step.now))
                s = r.state
                if let error = r.error { got["error"] = error }
            case "exits":
                let r = Paper.checkExits(s, candles: try XCTUnwrap(step.candles))
                s = r.state
                got["closed"] = r.closed.map(\.id)
            case "value":
                got["valuation"] = try json(Paper.valuation(s, prices: try XCTUnwrap(step.prices).compactMapValues { $0 }))
            default:
                XCTFail("unknown op \(step.op)")
            }
            got["state"] = try json(s)
            got["stats"] = try json(Paper.stats(s))
            var expected = raw[i]
            expected["step"] = nil
            assertSame(got, expected, "step \(i) (\(step.op))")
        }
        // The same end figures as the web's test.
        XCTAssertEqual(s.cash, 10_509.12)
        let st = Paper.stats(s)
        XCTAssertEqual(st.trades, 5)
        XCTAssertEqual(st.winRate, 40)
        XCTAssertEqual(st.byReason, PaperReasonCounts(stop: 2, target: 2, manual: 1))
        XCTAssertEqual(st.byVerdict.map { "\($0.verdict):\($0.trades)" }, ["buy:2", "buyZone:1", "none:1", "wait:1"])
    }

    func testVerdictOrderLikeLocaleCompare() {
        // "none" before "noPosition" (letters compared without case, like localeCompare).
        XCTAssertTrue(Paper.localeLess("none", "noPosition"))
        XCTAssertTrue(Paper.localeLess("buy", "buyZone"))
        XCTAssertTrue(Paper.localeLess("buyZone", "none"))
    }
}
