import XCTest
@testable import AltimCore

/// Tests sur données réelles, exécutés seulement avec `ALTIM_LIVE=1 swift test`.
final class LiveTests: XCTestCase {
    override func setUpWithError() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "ALTIM_LIVE=1 pour les tests réseau")
    }

    func testRealMarketSignalsAndBacktests() async throws {
        let yahoo = YahooMarketData()
        let assets = [Asset.defaults[0], Asset.defaults[1], Asset.defaults[4], Asset.defaults[5]]
        for asset in assets {
            for tf in [Timeframe.h1, .h4, .d1] {
                let candles = try await yahoo.candles(for: asset, timeframe: tf, limit: 500)
                var higher: [Candle]?
                if let h = tf.higher { higher = try await yahoo.candles(for: asset, timeframe: h, limit: 300) }
                let signal = try SignalEngine().analyze(candles, higherTimeframe: higher, timeframe: tf)
                let bt = Backtester().run(candles)
                print(String(format: "%@ %@ n=%d  %@ score %.0f conf %.0f  | BT %+.1f%% vs B&H %+.1f%%  trades %d  win %.0f%%  DD %.1f%%",
                             asset.symbol, tf.rawValue, candles.count, signal.action.label, signal.score, signal.confidence,
                             bt.totalReturnPercent, bt.buyAndHoldPercent, bt.trades.count, bt.winRatePercent, bt.maxDrawdownPercent))
                XCTAssertGreaterThan(candles.count, 100)
                XCTAssertTrue(signal.price > 0)
            }
            let q = try await yahoo.quote(for: asset)
            XCTAssertGreaterThan(q.price, 0)
        }
        let found = try await yahoo.search("apple")
        XCTAssertTrue(found.contains { $0.symbol == "AAPL" })
    }
}

/// Exporte un jeu de données + le score Swift, pour vérifier que le moteur TypeScript du site est identique.
/// `ALTIM_EXPORT_FIXTURE=/chemin/fixture.json swift test --filter FixtureExport`
final class FixtureExport: XCTestCase {
    func testExportFixture() throws {
        guard let path = ProcessInfo.processInfo.environment["ALTIM_EXPORT_FIXTURE"] else {
            throw XCTSkip("ALTIM_EXPORT_FIXTURE non défini")
        }
        struct Case: Encodable {
            let candles: [[Double]]
            let score: Double
            let confidence: Double
            let action: String
            let factors: [String: Double]
        }
        var cases: [Case] = []
        for (i, drift) in [0.004, -0.004, 0.0005, 0.002, -0.001].enumerated() {
            let candles = Fixtures.candles(count: 260, drift: drift, amplitude: 3, seed: UInt64(7 + i))
            let s = try SignalEngine().analyze(candles)
            cases.append(Case(
                candles: candles.map { [$0.time.timeIntervalSince1970 * 1000, $0.open, $0.high, $0.low, $0.close, $0.volume] },
                score: s.score, confidence: s.confidence, action: s.action.rawValue,
                factors: Dictionary(uniqueKeysWithValues: s.factors.map { ($0.name, $0.score) })))
        }
        try JSONEncoder().encode(cases).write(to: URL(fileURLWithPath: path))
    }
}
