import XCTest
@testable import AltimCore

/// Parity with web/src/engine/guard.ts: same deterministic scenarios, same scores, factors, statuses and points
/// (guard-fixture.json is written by web/test/guard-fixture.test.ts).
final class GuardTests: XCTestCase {
    static let D = 86_400_000.0, H4 = 14_400_000.0, H1 = 3_600_000.0, NOW = 1_800_000_000_000.0

    /// Same linear congruential generator as the TypeScript fixture (bit for bit).
    static func series(_ n: Int, _ step: Double, _ drift: Double, _ vol: Double, _ seed: UInt32, start: Double = 100,
                       t0: Double = 1_700_000_000_000) -> [Candle] {
        var s = seed
        func r() -> Double { s = s &* 1_664_525 &+ 1_013_904_223; return Double(s) / 4_294_967_296 }
        var p = start
        return (0..<n).map { i in
            let o = p
            p = p * (1 + drift + (r() - 0.5) * 2 * vol)
            let hi = max(o, p) * (1 + r() * vol * 0.5), lo = min(o, p) * (1 - r() * vol * 0.5)
            return Candle(time: Date(timeIntervalSince1970: (t0 + Double(i) * step) / 1000), open: o, high: hi, low: lo, close: p,
                          volume: 1000 + r() * 200)
        }
    }

    static var now: Date { Date(timeIntervalSince1970: NOW / 1000) }
    static func at(_ ms: Double) -> Date { Date(timeIntervalSince1970: ms / 1000) }

    static func calmUp() -> MarketGuard.Input {
        MarketGuard.Input(kind: .crypto, daily: series(400, D, 0.004, 0.01, 1), h4: series(300, H4, 0.0007, 0.004, 2),
                          h1: series(400, H1, 0.0002, 0.002, 3), now: now)
    }

    static let scenarios: [String: () -> MarketGuard.Input] = [
        "calmUp": calmUp,
        "down": {
            MarketGuard.Input(kind: .crypto, daily: series(400, D, -0.004, 0.01, 4), h4: series(300, H4, -0.0007, 0.004, 5),
                              h1: series(400, H1, -0.0002, 0.002, 12), positioning: .init(fundingRate: -0.0005),
                              sentiment: .init(fearGreed: [30, 18]), now: now)
        },
        "broken": {
            MarketGuard.Input(kind: .crypto,
                              daily: series(355, D, 0.003, 0.005, 6) + series(25, D, -0.012, 0.005, 7, start: 289, t0: 1_700_000_000_000 + 355 * D),
                              h4: series(300, H4, 0, 0.004, 7), h1: series(400, H1, 0, 0.003, 13), now: now)
        },
        "jumped": {
            var input = calmUp()
            var h1 = input.h1
            let L = h1[h1.count - 1], n = h1.count
            let c = { (k: Candle, o: Double?, h: Double?, l: Double?, cl: Double?, v: Double) in
                Candle(time: k.time, open: o ?? k.open, high: h ?? k.high, low: l ?? k.low, close: cl ?? k.close, volume: v)
            }
            h1[n - 3] = c(h1[n - 3], nil, nil, L.open * 0.94, L.open * 0.95, 20_000)
            h1[n - 2] = c(h1[n - 2], L.open * 0.95, L.open * 0.975, L.open * 0.945, L.open * 0.97, 18_000)
            h1[n - 1] = c(L, L.open * 0.97, L.open * 0.975, L.open * 0.95, L.open * 0.955, 15_000)
            input.h1 = h1
            return input
        },
        "crowd": {
            var input = calmUp()
            input.positioning = .init(fundingRate: 0.0008, longShortRatio: (0..<48).map { 1.2 + Double($0 % 5) * 0.02 } + [1.9])
            input.sentiment = .init(fearGreed: [60, 70, 86], socialBullish: 91, socialSample: 30)
            input.news = [
                .init(title: "Exchange hacked, $200M stolen", time: at(NOW - H1)),
                .init(title: "SEC opens investigation into token issuer", time: at(NOW - 2 * H1)),
                .init(title: "Bitcoin rally continues", time: at(NOW - 3 * H1)),
            ]
            return input
        },
        "stock": {
            var input = calmUp()
            input.kind = .stock
            input.news = (0..<10).map { MarketGuard.NewsItem(title: "Headline \($0)", time: at(NOW - Double($0 + 1) * 20 * H1)) }
                + (0..<6).map { MarketGuard.NewsItem(title: "Breaking \($0)", time: at(NOW - Double($0 + 1) * 0.5 * H1)) }
            input.vix = [18, 24, 31]
            return input
        },
    ]

    struct Expected: Decodable {
        struct F: Decodable { let code: String; let points: Int; let basePoints: Int; let status: String; let samples: Int?; let lift: Double? }
        struct Layer: Decodable { let score: Int; let level: String?; let direction: String?; let factors: [F] }
        struct Regime: Decodable { let trend: String; let strength: Int }
        struct Policy: Decodable { let scalping: String; let sizeMultiplier: Double; let stopMultiplier: Double }
        let name: String
        let regime: Regime
        let shock: Layer
        let reversal: Layer
        let policy: Policy
    }

    func check(_ got: [MarketGuard.Factor], _ want: [Expected.F], _ ctx: String) {
        XCTAssertEqual(got.map(\.code), want.map(\.code), ctx)
        for (g, w) in zip(got, want) {
            XCTAssertEqual(g.points, w.points, "\(ctx) \(w.code) points")
            XCTAssertEqual(g.basePoints, w.basePoints, "\(ctx) \(w.code) basePoints")
            XCTAssertEqual(g.status.rawValue, w.status, "\(ctx) \(w.code) status")
            XCTAssertEqual(g.evidence?.samples, w.samples, "\(ctx) \(w.code) samples")
            if let wl = w.lift { XCTAssertEqual(g.evidence!.lift, wl, accuracy: 1e-9, "\(ctx) \(w.code) lift") } else { XCTAssertNil(g.evidence) }
        }
    }

    func testParityWithTypeScript() throws {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("guard-fixture.json")
        let expected = try JSONDecoder().decode([Expected].self, from: Data(contentsOf: url))
        XCTAssertEqual(expected.count, Self.scenarios.count)
        for e in expected {
            let g = MarketGuard.evaluate(try XCTUnwrap(Self.scenarios[e.name], e.name)())
            XCTAssertEqual(g.regime.trend.rawValue, e.regime.trend, e.name)
            XCTAssertEqual(g.regime.strength, e.regime.strength, e.name)
            XCTAssertEqual(g.shock.score, e.shock.score, "\(e.name) shock")
            XCTAssertEqual(g.shock.level.rawValue, e.shock.level, "\(e.name) shock level")
            check(g.shock.factors, e.shock.factors, "\(e.name) shock")
            XCTAssertEqual(g.reversal.score, e.reversal.score, "\(e.name) reversal")
            XCTAssertEqual(g.reversal.direction?.rawValue, e.reversal.direction, "\(e.name) direction")
            check(g.reversal.factors, e.reversal.factors, "\(e.name) reversal")
            XCTAssertEqual(g.policy.scalping.rawValue, e.policy.scalping, e.name)
            XCTAssertEqual(g.policy.sizeMultiplier, e.policy.sizeMultiplier, e.name)
            XCTAssertEqual(g.policy.stopMultiplier, e.policy.stopMultiplier, e.name)
        }
    }

    func testWeighAndNewsTone() {
        XCTAssertEqual(MarketGuard.weigh(nil, historical: false).status, .unverifiable)
        XCTAssertEqual(MarketGuard.weigh(.init(samples: 5, rate: 80, base: 20, lift: 4), historical: true).weight, 0.5)
        XCTAssertEqual(MarketGuard.weigh(.init(samples: 100, rate: 20, base: 20, lift: 1), historical: true).status, .rejected)
        XCTAssertEqual(MarketGuard.weigh(.init(samples: 100, rate: 30, base: 20, lift: 1.5), historical: true).weight, 1)
        let tone = MarketGuard.newsTone([
            .init(title: "Company hit by lawsuit", time: Date()), .init(title: "Shares surge after upgrade", time: Date()),
            .init(title: "Stock plunges then surges", time: Date()), .init(title: "Quarterly report published", time: Date()),
            .init(title: "Bankruptcy fears", time: Date()), .init(title: "New banner campaign", time: Date()),
        ])
        XCTAssertEqual(tone.negative, 2)
        XCTAssertEqual(tone.positive, 1)
        XCTAssertEqual(MarketGuard.pivots([1, 2, 3, 9, 3, 2, 1, 2, 3, 4, 10, 4, 3, 2], high: true), [3, 10])
    }
}
