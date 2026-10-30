import Foundation
import XCTest
@testable import AltimKit

final class LocalGuardTests: XCTestCase {
    let end = 1_800_000_000_000.0

    func testRegimeOfASteadyRise() {
        let daily = LocalBackendTests.trend(count: 260, step: 86_400_000, end: end, slope: 0.003)
        let h4 = LocalBackendTests.trend(count: 300, step: 14_400_000, end: end, slope: 0.001)
        let r = GuardEngine.regime(daily, h4)
        XCTAssertEqual(r.trend, "up")
        XCTAssertTrue(r.text.hasPrefix("Tendance de fond haussière (moyennes 50 et 200 jours alignées"))
        XCTAssertTrue(r.text.hasSuffix(", confirmée en 4 h."))
        XCTAssertEqual(GuardEngine.regime(Array(daily.prefix(30)), h4).trend, "range")
    }

    func testAnHourlyCrashIsAShock() {
        // 300 calm hours, then the last hours fall 6 % each on a huge volume.
        var h1 = LocalBackendTests.trend(count: 300, step: 3_600_000, end: end - 6 * 3_600_000, slope: 0.0002)
        var p = h1[h1.count - 1].close
        for i in 1...6 {
            let next = p * 0.94
            h1.append(Bar(time: h1[h1.count - 1].time + 3_600_000, open: p, high: p, low: next * 0.99, close: next, volume: 50_000))
            p = next
            _ = i
        }
        let s = GuardEngine.shock(.init(kind: .crypto, daily: [], h4: [], h1: h1, now: end))
        XCTAssertTrue(s.factors.contains { $0.code == "vol2" })
        XCTAssertTrue(s.factors.contains { $0.code == "jump4" })
        XCTAssertTrue(s.factors.contains { $0.code == "volume" })
        XCTAssertNotEqual(s.level, "calm")
        // Calm market: nothing.
        let calm = GuardEngine.shock(.init(kind: .crypto, daily: [], h4: [], h1: LocalBackendTests.trend(count: 300, step: 3_600_000, end: end), now: end))
        XCTAssertEqual(calm.level, "calm")
    }

    func testReversalSentimentAndNewsTone() {
        let daily = LocalBackendTests.trend(count: 260, step: 86_400_000, end: end, slope: 0.003)
        let news = (0..<3).map { GuardEngine.Headline(title: "Exchange hacked, bitcoin plunges #\($0)", time: end - 3_600_000, source: nil) }
        let input = GuardEngine.Input(kind: .crypto, daily: daily, h4: [], h1: [], fundingRate: 0.0007, fearGreed: [50, 85], news: news, now: end)
        let r = GuardEngine.reversal(input, trend: "up")
        XCTAssertEqual(r.direction, "down")
        XCTAssertEqual(r.factors.first { $0.code == "funding" }?.basePoints, 25)
        XCTAssertEqual(r.factors.first { $0.code == "fearGreed" }?.basePoints, 15)
        XCTAssertEqual(r.factors.first { $0.code == "newsTone" }?.text, "3 actualités négatives en 24 h alors que la tendance est haussière.")
        // Unverifiable factors count at 75 %.
        XCTAssertEqual(r.factors.first { $0.code == "funding" }?.points, 19)
        XCTAssertEqual(GuardEngine.newsTone([.init(title: "SEC approves ETF, bitcoin surges", time: 0, source: nil)]).positive, 1)
        XCTAssertTrue(GuardEngine.contains(" a ban on ", "ban"))
        XCTAssertFalse(GuardEngine.contains(" bank run ", "ban"))
        let (_, _, _, policy) = GuardEngine.run(input)
        XCTAssertTrue(policy.notes.last!.hasPrefix("Pour le long terme : privilégier les achats sur repli"))
    }

    func testGuardJsonIsWhatTheAppDecodes() throws {
        let (reg, sh, rev, pol) = GuardEngine.run(.init(kind: .stock, daily: [], h4: [], h1: [], now: end))
        let g = GuardReport(symbol: "AAPL", kind: .stock, price: 1, asOf: end, regime: reg, shock: sh, reversal: rev, policy: pol, inputs: nil, macro: nil)
        let back = try JSONDecoder().decode(GuardReport.self, from: JSONEncoder().encode(g))
        XCTAssertEqual(back.shockLabel, "Calme")
        XCTAssertEqual(back.policyLabel, "Autorisé")
        XCTAssertEqual(back.policy.notes, ["Conditions normales : pas de signal de choc ni de retournement."])
    }

    func testLiveGuard() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "réseau réel non demandé")
        Money.setDisplay(.usd, nil)
        let backend = LocalBackend()
        for (s, k) in [("BTC", "crypto"), ("AAPL", "stock")] {
            let r = await backend.answer(path: "/api/guard", query: ["symbol": s, "kind": k])!
            guard let g = try? JSONDecoder().decode(GuardReport.self, from: r.0) else { print(String(decoding: r.0, as: UTF8.self)); continue }
            print("\(s) garde-fou : \(g.trendLabel) — \(g.regime.text) | choc \(g.shockLabel) \(Int(g.shock.score)) | retournement \(Int(g.reversal.score)) \(g.reversal.direction ?? "-") | \(g.policyLabel)")
            for f in g.shock.factors + g.reversal.factors { print("  · \(f.code) \(Int(f.points))/\(Int(f.basePoints ?? 0)) \(f.status ?? "") : \(f.text)") }
            print("  F&G \(g.inputs?.fearGreed.map { "\(Int($0))" } ?? "-"), actus 24 h \(g.inputs?.news24h ?? 0)")
            let al = await backend.answer(path: "/api/alerts", query: ["symbols": "\(s):\(k)"])!
            for a in try JSONDecoder().decode([BuyAlert].self, from: al.0) { print("  alerte : \(a.title) — \(a.body)") }
        }
    }
}

extension LocalGuardTests {
    func testNewsBurstIsMeasuredOnTheCoveredPeriod() {
        // 2 articles every 6 h over 48 h: steady, no burst.
        let steady = (0..<16).flatMap { k in (0..<2).map { _ in GuardEngine.Headline(title: "t", time: end - Double(k) * 6 * 3_600_000 - 60_000, source: nil) } }
        XCTAssertFalse(GuardEngine.shock(.init(kind: .crypto, daily: [], h4: [], h1: [], news: steady, now: end)).factors.contains { $0.code.hasPrefix("news") })
        // 12 articles in the last 6 h on top of it: a burst.
        let burst = steady + (0..<12).map { _ in GuardEngine.Headline(title: "t", time: end - 3_600_000, source: nil) }
        XCTAssertTrue(GuardEngine.shock(.init(kind: .crypto, daily: [], h4: [], h1: [], news: burst, now: end)).factors.contains { $0.code == "newsBurst" })
    }
}
