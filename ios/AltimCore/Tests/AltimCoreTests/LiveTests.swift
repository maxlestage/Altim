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

/// Every source, every timeframe, live network: `ALTIM_LIVE=1 swift test --filter LiveSourcesTests`.
final class LiveSourcesTests: XCTestCase {
    override func setUpWithError() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "ALTIM_LIVE=1 pour les tests réseau")
    }

    func testEachSourceEachTimeframe() async throws {
        let sources: [MarketSource] = [OKXMarketData(), CoinbaseMarketData(), KrakenMarketData(), KuCoinMarketData(),
                                       GateMarketData(), BitfinexMarketData(), BinanceSource.us(), YahooMarketData(server: 1),
                                       YahooMarketData(server: 2)]
        let btc = Asset.defaults[0]
        var failures: [String] = []
        for source in sources {
            for tf in Timeframe.allCases {
                do {
                    let c = try await source.candles(for: btc, timeframe: tf, limit: 500)
                    let clean = c.sanitized()
                    let ordered = zip(c.dropFirst(), c).allSatisfy { $0.time > $1.time }
                    let unclosed = c.filter { !$0.isClosed }.count
                    let lastClosed = clean.last!
                    let age = Date().timeIntervalSince(lastClosed.time) / tf.seconds
                    let q = DataQuality.assess(c, timeframe: tf, assetClass: .crypto)
                    print(String(format: "%-18@ %-3@ n=%4d propres=%4d ordre=%@ en-cours=%d âge=%.1f UT dernier=%.1f qualité=%.0f %@",
                                 source.name, tf.rawValue, c.count, clean.count, ordered ? "ok" : "KO", unclosed, age,
                                 lastClosed.close, q.score, q.issues.joined(separator: " | ")))
                    if !ordered || clean.count < 50 || unclosed > 2 || age > 2.5 { failures.append("\(source.name) \(tf.rawValue)") }
                } catch {
                    print("\(source.name) \(tf.rawValue) ÉCHEC : \(error.localizedDescription)")
                    failures.append("\(source.name) \(tf.rawValue) : \(error.localizedDescription)")
                }
            }
        }
        print("Échecs :", failures)
        XCTAssertTrue(failures.isEmpty, failures.joined(separator: "\n"))
    }

    func testConsensusOnRealMarkets() async throws {
        let consensus = ConsensusMarketData.standard(transport: ResilientTransport())
        let assets = Array(Asset.defaults.prefix(4)) + [Asset.defaults[4], Asset.defaults[5],
            Asset(symbol: "DOGEUSDT", name: "Dogecoin", assetClass: .crypto, quote: "USDT")]
        for asset in assets {
            for tf in [Timeframe.h1, .h4, .d1] {
                let s = try await consensus.snapshot(for: asset, timeframe: tf)
                let checks = s.checks.map { c -> String in
                    let status: String = switch c.status {
                    case .primary: "★"
                    case .agrees: "✓"
                    case .diverges: "✗"
                    case .failed: "⚠︎"
                    case .skipped: "·"
                    }
                    return "\(status)\(c.name)" + (c.deviationPercent.map { String(format: "(%.3f%%)", $0) } ?? "")
                }
                print(String(format: "%-9@ %-3@ fiab %3.0f %@ | %@", asset.symbol, tf.rawValue, s.reliabilityScore,
                             s.summary, checks.joined(separator: " ")))
                XCTAssertGreaterThanOrEqual(s.independentSources, 2, "\(asset.symbol) \(tf.rawValue)")
                XCTAssertFalse(s.conflict)
            }
        }
        let fg = try await SentimentProvider().cryptoFearGreed()
        print("Fear & Greed :", fg.value, fg.label)
        for asset in [Asset.defaults[0], Asset.defaults[4]] {
            let s = try await SentimentProvider().social(asset)
            print("StockTwits \(asset.symbol) :", s.bullishPercent.map { String(format: "%.0f %% haussier", $0) } ?? "-", "sur", s.sampleSize)
        }
        let nasdaq = try await NasdaqMarketData().candles(for: Asset.defaults[4], timeframe: .d1, limit: 500)
        let yahoo = try await YahooMarketData().candles(for: Asset.defaults[4], timeframe: .d1, limit: 500)
        let common = Set(nasdaq.map(\.time)).intersection(yahoo.map(\.time))
        print("Nasdaq/Yahoo AAPL : \(nasdaq.count) / \(yahoo.count) bougies, \(common.count) dates alignées")
        XCTAssertGreaterThan(common.count, 200)
        let cboe = try await CboeQuotes().quote(for: Asset.defaults[4])
        XCTAssertEqual(cboe.price, yahoo.last!.close, accuracy: yahoo.last!.close * 0.02)
    }
}
