import Foundation
import XCTest
@testable import AltimKit

private let dayMs = 86_400_000.0
private func day(_ iso: String) -> Double { ISO8601DateFormatter().date(from: "\(iso)T00:00:00Z")!.timeIntervalSince1970 * 1000 }

/// Same cases as the site's test (web/test/tools.test.ts) and Android.
final class ToolsTests: XCTestCase {
    func testCompareRealCloses() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "history", withExtension: "json", subdirectory: "Fixtures"))
        let r = try JSONDecoder().decode(HistoryResponse.self, from: Data(contentsOf: url))
        let c = try XCTUnwrap(Tools.compare(r.byId, ids: ["crypto:BTC", "crypto:ETH", "stock:AAPL"], days: 90, now: Date(timeIntervalSince1970: r.asOf / 1000)))
        for s in c.stats {
            XCTAssertEqual(s.pct.count, c.days.count)
            XCTAssertEqual(s.pct.first, 0)
            XCTAssertLessThanOrEqual(s.maxDrawdown, 0)
        }
        XCTAssertGreaterThan(c.stats[1].volatility, c.stats[0].volatility)
        XCTAssertGreaterThan(c.stats[0].volatility, c.stats[2].volatility)
        XCTAssertGreaterThan(try XCTUnwrap(c.correlation[0][1]), 0.5)
    }

    func testCompareExact() throws {
        let a: [(Double, Double)] = [100.0, 110, 99, 120].enumerated().map { (day("2026-09-01") + Double($0.offset) * dayMs, $0.element) }
        let b: [(Double, Double)] = [50.0, 55].enumerated().map { (day("2026-09-03") + Double($0.offset) * dayMs, $0.element) }
        let c = try XCTUnwrap(Tools.compare(["crypto:A": a, "crypto:B": b], ids: ["crypto:A", "crypto:B", "crypto:GONE"], days: 30, now: Date(timeIntervalSince1970: day("2026-09-05") / 1000)))
        XCTAssertEqual(c.missing, ["crypto:GONE"])
        XCTAssertEqual(c.days.first, day("2026-09-03"))
        XCTAssertEqual(c.stats[0].pct.last!, 21.21, accuracy: 0.005)
        XCTAssertEqual(c.stats[1].change, 10, accuracy: 1e-9)
        XCTAssertNil(c.correlation[0][1])
    }

    func testPositionAndRebalance() throws {
        let p = try XCTUnwrap(Tools.positionSize(capital: 10_000, riskPct: 1, entry: 100, stop: 95, target: 110))
        XCTAssertEqual(p.quantity, 20, accuracy: 1e-9)
        XCTAssertEqual(p.risk, 100, accuracy: 1e-9)
        XCTAssertEqual(p.ratio!, 2, accuracy: 1e-9)
        XCTAssertTrue(try XCTUnwrap(Tools.positionSize(capital: 1000, riskPct: 2, entry: 100, stop: 99.5)).capped)
        XCTAssertNil(Tools.positionSize(capital: 1000, riskPct: 1, entry: 100, stop: 120))

        let r = try XCTUnwrap(Tools.rebalance([(id: "crypto:BTC", kind: .crypto, value: 6000), (id: "crypto:ETH", kind: .crypto, value: 2000), (id: "stock:AAPL", kind: .stock, value: 2000)], targetCrypto: 50))
        XCTAssertEqual(r.total, 10_000)
        XCTAssertEqual(r.moves[.crypto]!, -3000, accuracy: 1e-9)
        XCTAssertEqual(r.moves[.stock]!, 3000, accuracy: 1e-9)
        XCTAssertEqual(r.lines.map(\.amount), [-2250, -750, 3000])
    }

    func testSaleAfterTax() {
        let t = Tools.saleTotal([(id: "a", value: 10_000, cost: 6000), (id: "b", value: 5000, cost: 7000), (id: "c", value: 1000, cost: nil)])
        XCTAssertEqual(t.lines[0].fees, 10, accuracy: 1e-9)
        XCTAssertEqual(t.lines[0].gain!, 3990, accuracy: 1e-9)
        XCTAssertEqual(t.lines[0].tax, 1197, accuracy: 1e-9)
        XCTAssertEqual(t.lines[0].net, 8793, accuracy: 1e-9)
        XCTAssertEqual(t.lines[1].tax, 0)
        XCTAssertNil(t.lines[2].gain)
        XCTAssertEqual(t.tax, (3990 - 2005) * 0.3, accuracy: 1e-9)
        XCTAssertEqual(t.unknownCost, 1)
    }

    func testProjection() {
        XCTAssertEqual(Tools.projection(start: 1000, monthly: 100, years: 10, ratePct: 0).last!.value, 13_000, accuracy: 1e-6)
        XCTAssertEqual(Tools.projection(start: 10_000, monthly: 0, years: 5, ratePct: 8).last!.value, 10_000 * pow(1.08, 5), accuracy: 1e-6)
        let r = pow(1.04, 1.0 / 12) - 1
        XCTAssertEqual(Tools.projection(start: 0, monthly: 200, years: 20, ratePct: 4).last!.value, 200 * (pow(1 + r, 240) - 1) / r, accuracy: 1e-6)
    }

    func testMoveAlert() throws {
        let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
        let t = PriceTarget(asset: btc, above: false, price: 100, move: 5)
        XCTAssertFalse(t.isReached(by: 104.9))
        XCTAssertTrue(t.isReached(by: 105))
        XCTAssertTrue(t.isReached(by: 94))
        let result = PriceTarget.evaluate([t], prices: [btc.id: 106])
        XCTAssertEqual(result.fired.count, 1)
        let again = result.targets[0].rearmed(at: 106)
        XCTAssertNil(again.triggered)
        XCTAssertEqual(again.price, 106)
        // Alerts stored before keep working (no "move" field).
        var old = t
        old.move = nil
        old.above = true
        let data = try JSONEncoder().encode(old)
        let decoded = try JSONDecoder().decode(PriceTarget.self, from: data)
        XCTAssertNil(decoded.move)
    }
}
