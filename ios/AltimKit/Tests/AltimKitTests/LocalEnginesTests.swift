import Foundation
import XCTest
@testable import AltimKit

final class LocalEnginesTests: XCTestCase {
    /// 60 flat days, a rise from 100 to 200, then a pull-back to `back`.
    static func riseThenPullBack(to back: Double) -> [Bar] {
        var out: [Bar] = []
        var t = 0.0
        func add(_ p: Double) {
            out.append(Bar(time: t, open: p, high: p * 1.01, low: p * 0.99, close: p, volume: 100))
            t += 86_400_000
        }
        for _ in 0..<60 { add(100) }
        for i in 1...30 { add(100 + Double(i) * 100 / 30) }
        for i in 1...10 { add(200 - Double(i) * (200 - back) / 10) }
        return out
    }

    func testZoneOfAnUpMove() {
        Money.setDisplay(.usd, nil)
        let z = FibEngine.zone(Self.riseThenPullBack(to: 150), FibEngine.medium, price: nil, withEvidence: false)
        XCTAssertEqual(z.status, "inZone")
        let zone = z.zone!
        // Low 99 (100 × 0.99), high 202 (200 × 1.01): 38.2 % = 162.65, 65 % = 135.05.
        XCTAssertEqual(zone.to, 202 - 0.382 * 103, accuracy: 1e-9)
        XCTAssertEqual(zone.from, 202 - 0.65 * 103, accuracy: 1e-9)
        XCTAssertEqual(z.invalidation, 99)
        XCTAssertEqual(z.targets.first, 202)
        XCTAssertTrue(z.text.hasPrefix("Prix dans la zone d'achat"))
        XCTAssertEqual(FibEngine.zone(Self.riseThenPullBack(to: 195), FibEngine.medium, price: nil, withEvidence: false).status, "above")
        XCTAssertEqual(FibEngine.zone(Self.riseThenPullBack(to: 132), FibEngine.medium, price: nil, withEvidence: false).status, "deep")
        XCTAssertEqual(FibEngine.zone(Self.riseThenPullBack(to: 150), FibEngine.medium, price: 90, withEvidence: false).status, "broken")
        XCTAssertEqual(FibEngine.zone(Array(Self.riseThenPullBack(to: 150).prefix(20)), FibEngine.medium, price: nil, withEvidence: false).status, "none")
        // Same JSON as the server's: the app's model reads it.
        let r = ZonesReport(symbol: "X", kind: .stock, price: 150, asOf: 0, zones: [z], macro: nil)
        XCTAssertNoThrow(try JSONDecoder().decode(ZonesReport.self, from: JSONEncoder().encode(r)))
    }

    func testBacktestNeverSeesTheFuture() {
        let end = 1_800_000_000_000.0
        let bars = LocalBackendTests.trend(count: 420, step: 86_400_000, end: end, slope: 0.003)
        let asset = Asset(symbol: "X", kind: .crypto, name: "X")
        let full = Backtest.run(bars, asset: asset)
        XCTAssertNil(full.error)
        XCTAssertFalse(full.trades.isEmpty)
        XCTAssertGreaterThan(full.holdReturn, 0)
        XCTAssertLessThanOrEqual(full.strategyDrawdown, 0)
        XCTAssertLessThanOrEqual(full.curve.count, 90)
        // The trades closed before the last 60 days are the same with or without those days.
        let cut = Backtest.run(Array(bars.dropLast(60)), asset: asset)
        let limit = bars[bars.count - 61].time
        let closed = { (r: BacktestReport) in r.trades.filter { $0.exit != "open" && $0.exitTime < limit - 86_400_000 } }
        XCTAssertEqual(closed(full), closed(cut))
        XCTAssertEqual(Backtest.run(Array(bars.prefix(50)), asset: asset).error != nil, true)
    }

    func testBacktestStopIsHit() {
        // A rise that triggers a buy, then a crash: the position is closed by its stop, not held to the bottom.
        var bars = LocalBackendTests.trend(count: 300, step: 86_400_000, end: 1_800_000_000_000, slope: 0.004)
        let last = bars[bars.count - 1]
        for i in 1...40 {
            let p = last.close * pow(0.97, Double(i))
            bars.append(Bar(time: last.time + Double(i) * 86_400_000, open: p / 0.97, high: p / 0.97 * 1.002, low: p * 0.998, close: p, volume: 1000))
        }
        let r = Backtest.run(bars, asset: Asset(symbol: "X", kind: .crypto, name: "X"))
        XCTAssertTrue(r.trades.contains { $0.exit == "stop" || $0.exit == "signal" })
        XCTAssertGreaterThan(r.strategyDrawdown, r.holdDrawdown)
    }

    func testHistoryRouteAddsTheBenchmarks() async throws {
        let backend = LocalBackend(fetcher: FakeFetcher(routes: []))
        let bad = await backend.answer(path: "/api/history", query: ["days": "12"])
        XCTAssertEqual(bad?.1, 400)
        let r = await backend.answer(path: "/api/history", query: ["days": "90", "symbols": "ETH:crypto"])!
        let h = try JSONDecoder().decode(HistoryResponse.self, from: r.0)
        XCTAssertEqual(h.series.map(\.id), ["crypto:ETH", "crypto:BTC", "stock:SPY"])
        XCTAssertEqual(h.series.first?.error, "historique indisponible")
        XCTAssertEqual(h.days, 90)
    }

    func testLiveZonesAndBacktest() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "réseau réel non demandé")
        Money.setDisplay(.usd, nil)
        let backend = LocalBackend()
        let hist = await backend.answer(path: "/api/history", query: ["days": "365", "symbols": "SOL:crypto,AAPL:stock"])!
        for s in try JSONDecoder().decode(HistoryResponse.self, from: hist.0).series { print("history \(s.id): \(s.closes.count) jours \(s.error ?? "")") }
        for (s, k) in [("BTC", "crypto"), ("ETH", "crypto"), ("AAPL", "stock"), ("NVDA", "stock")] {
            let z = await backend.answer(path: "/api/zones", query: ["symbol": s, "kind": k])!
            let zr = try JSONDecoder().decode(ZonesReport.self, from: z.0)
            for zone in zr.zones { print("\(s) \(zone.label): \(zone.statusLabel) — \(zone.text) [\(zone.evidence.map { "\($0.samples) cas, \(Int($0.rate)) % vs \(Int($0.base)) %" } ?? "-")]") }
            let b = await backend.answer(path: "/api/backtest", query: ["symbol": s, "kind": k])!
            let br = try JSONDecoder().decode(BacktestReport.self, from: b.0)
            print("\(s) backtest \(br.days) j : signal \(Int(br.strategyReturn)) % (creux \(Int(br.strategyDrawdown)) %) vs garder \(Int(br.holdReturn)) % (creux \(Int(br.holdDrawdown)) %), \(br.trades.count) trades, réussite \(Int(br.winRate ?? 0)) %, investi \(Int(br.exposure)) % — \(br.verdict)")
        }
    }
}
