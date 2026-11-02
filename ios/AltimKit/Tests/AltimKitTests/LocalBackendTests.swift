import Foundation
import XCTest
@testable import AltimKit

/// Answers the market URLs from a table (prefix → body); anything else fails like a dead provider.
struct FakeFetcher: MarketFetcher {
    var routes: [(String, String)]
    func data(_ url: String, method: String, body: Data?) async throws -> Data {
        guard let r = routes.first(where: { url.hasPrefix($0.0) }) else { throw MarketError("HTTP 404") }
        return Data(r.1.utf8)
    }
}

final class LocalBackendTests: XCTestCase {
    /// Hourly candles of a steady rise with small waves, ending one hour before `now`.
    static func trend(count: Int = 300, step: Double = 3_600_000, end: Double, start: Double = 100, slope: Double = 0.002) -> [Bar] {
        (0..<count).map { i in
            let t = end - Double(count - i) * step
            let base = start * pow(1 + slope, Double(i)) * (1 + 0.01 * sin(Double(i) / 3))
            return Bar(time: t, open: base * 0.998, high: base * 1.004, low: base * 0.995, close: base, volume: 1000 + Double(i % 7) * 50)
        }
    }

    func testIndicatorsMatchTheReferenceFormulas() {
        XCTAssertEqual(Indicators.sma([1, 2, 3, 4, 5], 3), [nil, nil, 2, 3, 4])
        let e = Indicators.ema([1, 2, 3, 4, 5, 6], 3)
        XCTAssertNil(e[1])
        XCTAssertEqual(e[2], 2)
        XCTAssertEqual(e[3]!, 3, accuracy: 1e-12)
        // Only rises: RSI 100; flat: 50.
        XCTAssertEqual(Indicators.rsi(Array(stride(from: 1.0, through: 20, by: 1)), 14).last!, 100)
        XCTAssertEqual(Indicators.rsi([Double](repeating: 5, count: 20), 14).last!, 50)
        XCTAssertEqual(Indicators.median([3, 1, 2]), 2)
        XCTAssertEqual(Indicators.median([4, 1, 2, 3]), 2.5)
    }

    func testRisingMarketGivesABuyAndAPlan() {
        let now = 1_800_000_000_000.0
        let s = SignalEngine.analyze(Self.trend(end: now), intervalMs: 3_600_000, now: now)!
        XCTAssertGreaterThan(s.score, 0)
        XCTAssertTrue(s.hasPlan)
        XCTAssertLessThan(s.stopLoss, s.price)
        XCTAssertGreaterThan(s.takeProfit, s.price)
        XCTAssertEqual(s.factors.map(\.name), ["Tendance", "MACD", "RSI", "Stochastique", "Bollinger", "Volume (OBV)"])
        XCTAssertNil(SignalEngine.analyze(Array(Self.trend(end: now).prefix(59))))
    }

    func testDoubtfulDataNeverBuys() {
        let now = 1_800_000_000_000.0
        let s = SignalEngine.analyze(Self.trend(end: now), intervalMs: 3_600_000, now: now)!
        let one = SignalEngine.reliability(quality: 100, independent: 1, conflict: false)
        XCTAssertEqual(one.level, "low")
        XCTAssertEqual(SignalEngine.gate(s, one, issues: []).action, .hold)
        XCTAssertEqual(SignalEngine.reliability(quality: 100, independent: 3, conflict: false).level, "high")
        XCTAssertEqual(SignalEngine.reliability(quality: 100, independent: 5, conflict: true).score, 30)
        let q = SignalEngine.assessQuality(Self.trend(end: now - 10 * 86_400_000), intervalMs: 3_600_000, kind: .crypto, now: now)
        XCTAssertTrue(q.stale)
        XCTAssertTrue(q.issues.contains("Données périmées."))
    }

    func testParsersReadTheRealFormats() throws {
        func j(_ s: String) throws -> Any { try JSONSerialization.jsonObject(with: Data(s.utf8), options: [.fragmentsAllowed]) }
        let cb = try Parse.coinbase(j("[[1793365200,77087.53,77317.91,77317.91,77115.51,89.9],[1793361600,77144.12,77467.16,77241.91,77318.98,377.6]]"))
        XCTAssertEqual(cb.map(\.time), [1793361600000, 1793365200000])
        XCTAssertEqual(cb[1].open, 77317.91)
        XCTAssertEqual(cb[1].low, 77087.53)
        let kr = try Parse.kraken(j(#"{"error":[],"result":{"XXBTZUSD":[[1782993600,"61286.5","62137.2","61080.8","61550.0","61621.8","1035.8",14671]],"last":1}}"#))
        XCTAssertEqual(kr.first, Bar(time: 1782993600000, open: 61286.5, high: 62137.2, low: 61080.8, close: 61550, volume: 1035.8))
        XCTAssertThrowsError(try Parse.kraken(j(#"{"error":["EQuery:Unknown asset pair"]}"#)))
        let okx = try Parse.okx(j(#"{"code":"0","data":[["1793361600000","77362.4","77588.5","77235.7","77320.1","575.5"],["1793347200000","78200","78250","77222.1","77362.4","2705.5"]]}"#))
        XCTAssertEqual(okx.map(\.time), [1793347200000, 1793361600000])
        let gate = try Parse.gate(j(#"[["1793347200","100","77300","77600","77200","77400","12.5","true"]]"#))
        XCTAssertEqual(gate.first, Bar(time: 1793347200000, open: 77400, high: 77600, low: 77200, close: 77300, volume: 12.5))
        let nas = try Parse.nasdaq(j(#"{"data":{"tradesTable":{"rows":[{"date":"10/29/2026","close":"$348.91","volume":"42,582,130","open":"$348.00","high":"$351.43","low":"$347.30"}]}}}"#))
        // 29 October 2026, 9:30 in New York (EDT, UTC-4) = 13:30 UTC.
        XCTAssertEqual(nas.first?.time, 1793280600000)
        XCTAssertEqual(nas.first?.volume, 42_582_130)
        let y = try Parse.yahoo(j(#"{"chart":{"result":[{"timestamp":[1,2,3],"indicators":{"quote":[{"open":[1,null,3],"high":[2,2,4],"low":[0.5,1,2],"close":[1.5,1,3],"volume":[10,null,30]}]}}]}}"#))
        XCTAssertEqual(y.map(\.time), [1000, 3000])
        XCTAssertThrowsError(try Parse.yahoo(j(#"{"chart":{"result":null,"error":{"description":"No data found, symbol may be delisted"}}}"#)))
        XCTAssertEqual(num("$1,234.50"), 1234.5)
        XCTAssertEqual(num("-1.18%"), -1.18)
        XCTAssertTrue(num(nil).isNaN)
    }

    func testBucketsAndAggregation() {
        let day = 86_400_000.0
        let daily: [Bar] = (3...13).map { (n: Int) -> Bar in
            let x = Double(n)
            return Bar(time: x * day, open: x * 10 - 1, high: x * 10 + 2, low: x * 10 - 3, close: x * 10, volume: 10)
        }
        let b = Candles.bucket(daily, .d4)
        XCTAssertEqual(b.map { $0.time / day }, [4, 8, 12])
        XCTAssertEqual([b[0].open, b[0].close, b[0].high, b[0].low, b[0].volume], [39, 70, 72, 37, 40])
        let hours = (0..<8).map { i in Bar(time: Double(i) * 3_600_000, open: 1, high: 2, low: 0.5, close: Double(i), volume: 1) }
        let four = Candles.aggregate(hours, from: 3_600_000, to: 14_400_000)
        XCTAssertEqual(four.map(\.close), [3, 7])
        XCTAssertEqual(four.map(\.volume), [4, 4])
    }

    /// Three crypto exchanges agree, a fourth is 5 % off: it is set aside, the reliability is high.
    func testConsensusDropsTheOutlier() async throws {
        let now = 1_800_000_000_000.0
        let good = Self.trend(count: 300, step: 14_400_000, end: now)
        func binance(_ bars: [Bar]) -> String {
            "[" + bars.map { "[\(Int($0.time)),\"\($0.open)\",\"\($0.high)\",\"\($0.low)\",\"\($0.close)\",\"\($0.volume)\"]" }.joined(separator: ",") + "]"
        }
        func gate(_ bars: [Bar]) -> String {
            "[" + bars.map { "[\"\(Int($0.time / 1000))\",\"0\",\"\($0.close)\",\"\($0.high)\",\"\($0.low)\",\"\($0.open)\",\"\($0.volume)\"]" }.joined(separator: ",") + "]"
        }
        let off = good.map { Bar(time: $0.time, open: $0.open * 1.05, high: $0.high * 1.05, low: $0.low * 1.05, close: $0.close * 1.05, volume: $0.volume) }
        let fetcher = FakeFetcher(routes: [
            ("https://api.binance.com/api/v3/klines", binance(good)),
            ("https://api.gateio.ws", gate(good)),
            ("https://api.kucoin.com", #"{"code":"200000","data":["# + good.reversed().map { "[\"\(Int($0.time / 1000))\",\"\($0.open)\",\"\($0.close)\",\"\($0.high)\",\"\($0.low)\",\"\($0.volume)\"]" }.joined(separator: ",") + "]}"),
            ("https://www.okx.com", #"{"code":"0","data":["# + off.reversed().map { "[\"\(Int($0.time))\",\"\($0.open)\",\"\($0.high)\",\"\($0.low)\",\"\($0.close)\",\"\($0.volume)\"]" }.joined(separator: ",") + "]}"),
        ])
        let s = try await LocalSnapshot.build(Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), .h4, fetcher: fetcher, now: now)
        XCTAssertEqual(s.sources.filter(\.ok).map(\.name).sorted(), ["Binance", "Gate.io", "KuCoin"])
        XCTAssertEqual(s.sources.first { $0.name == "OKX" }?.ok, false)
        XCTAssertEqual(s.reliability.level, "high")
        XCTAssertEqual(s.candles.count, 300)
        XCTAssertNotNil(s.signal(higher: nil))
        // The JSON is what the app reads from the server.
        let decoded = try JSONDecoder().decode(Snapshot.self, from: try JSONEncoder().encode(s.json))
        XCTAssertEqual(decoded.agreeing, 3)
        XCTAssertEqual(decoded.interval, "4h")
    }

    func testQuotesAreTheMedianOfTheAgreeingSources() {
        let a = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
        let q = QuoteSources.combine(a, [
            ("A", .success(PriceQuote(price: 100, change: 1))), ("B", .success(PriceQuote(price: 100.2, change: 2))),
            ("C", .success(PriceQuote(price: 100.1, change: nil))), ("D", .success(PriceQuote(price: 110, change: 9))),
            ("E", .failure(MarketError("HTTP 451"))),
        ])!
        XCTAssertEqual(q.price, 100.1)
        XCTAssertEqual(q.change, 1.5)
        XCTAssertEqual(q.agreeing, 3)
        XCTAssertEqual(q.total, 5)
        XCTAssertEqual(q.sources.map(\.ok), [true, true, true, false, false])
        XCTAssertEqual(q.sources.last?.error, "HTTP 451")
    }

    /// Sources split into two camps: the median between them is no price; the largest camp is kept, never 0.
    func testQuotesOfSplitSourcesKeepTheLargestCamp() {
        let a = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
        let two = QuoteSources.combine(a, [("A", .success(PriceQuote(price: 100, change: 1))), ("B", .success(PriceQuote(price: 50, change: nil)))])!
        XCTAssertEqual(two.price, 100, "deux sources en désaccord : la première listée, jamais leur moyenne ni 0")
        XCTAssertEqual(two.agreeing, 1)
        XCTAssertEqual(two.sources.map(\.ok), [true, false])
        let split = QuoteSources.combine(a, [
            ("A", .success(PriceQuote(price: 50, change: nil))), ("B", .success(PriceQuote(price: 100, change: 1))),
            ("C", .success(PriceQuote(price: 100.2, change: 3))), ("D", .success(PriceQuote(price: 50.1, change: nil))),
            ("E", .success(PriceQuote(price: 100.1, change: 2))),
        ])!
        XCTAssertEqual(split.price, 100.1, "la médiane du plus grand groupe")
        XCTAssertEqual(split.agreeing, 3)
        let even = QuoteSources.combine(a, [
            ("A", .success(PriceQuote(price: 100, change: nil))), ("B", .success(PriceQuote(price: 100.2, change: nil))),
            ("C", .success(PriceQuote(price: 50, change: nil))), ("D", .success(PriceQuote(price: 50.1, change: nil))),
        ])!
        XCTAssertEqual(even.price, 100.1, accuracy: 1e-9, "2 contre 2 : le groupe de la première source")
        XCTAssertEqual(even.agreeing, 2)
    }

    func testBackendRoutesAndParameters() async throws {
        let backend = LocalBackend(fetcher: FakeFetcher(routes: []))
        let none = await backend.answer(path: "/api/decision", query: [:])
        XCTAssertNil(none)
        let bad = await backend.answer(path: "/api/candles", query: ["symbol": "BTC", "kind": "gold", "interval": "4h"])
        XCTAssertEqual(bad?.1, 400)
        let dead = await backend.answer(path: "/api/candles", query: ["symbol": "BTC", "kind": "crypto", "interval": "4h"])
        XCTAssertEqual(dead?.1, 502)
        XCTAssertEqual(try JSONDecoder().decode([String: String].self, from: dead!.0)["error"], "Toutes les sources ont échoué")
        let radar = await backend.answer(path: "/api/radar", query: ["symbols": "BTC:crypto,AAPL:stock", "interval": "4h"])
        let rows = try JSONDecoder().decode([RadarRow].self, from: radar!.0)
        XCTAssertEqual(rows.map(\.symbol), ["BTC", "AAPL"])
        XCTAssertNotNil(rows[0].error)
        let search = await backend.answer(path: "/api/search", query: ["q": "sol", "limit": "5"])
        let items = try JSONDecoder().decode([SearchItem].self, from: search!.0)
        XCTAssertEqual(items.first?.symbol, "SOL")
        XCTAssertEqual(LocalBackend.assets("btc:crypto,bad,AAPL:stock,X:gold").map(\.id), ["crypto:BTC", "stock:AAPL"])
    }

    func testStandaloneClientNeverCallsAServer() async throws {
        let c = AltimClient.standalone(local: LocalBackend(fetcher: FakeFetcher(routes: [])))
        XCTAssertTrue(c.isStandalone)
        do {
            _ = try await c.macro()
            XCTFail("macro sans aucune source")
        } catch {
            // Computed on the iPhone: with every source down it fails like a dead network, never by asking a server.
            XCTAssertEqual(error as? AltimError, .server("données macro indisponibles"))
        }
        let items = try await c.search("btc")
        XCTAssertEqual(items.first?.symbol, "BTC")
        do {
            _ = try await c.candles(Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), interval: "4h")
            XCTFail("aucune source")
        } catch {
            XCTAssertEqual(error as? AltimError, .server("Toutes les sources ont échoué"))
        }
    }

    func testFxFallsBackToTheECB() async {
        let ecb = "<Cube currency='JPY' rate='160.1'/><Cube currency='USD' rate='1.0850'/>"
        let r = await LocalFx.read(fetcher: FakeFetcher(routes: [("https://www.ecb.europa.eu", ecb)]), now: 5)
        XCTAssertEqual(r.source, "BCE")
        XCTAssertEqual(r.usdPerEur, 1.085)
        XCTAssertNotNil(Fx.parse(r))
        let none = await LocalFx.read(fetcher: FakeFetcher(routes: []), now: 5)
        XCTAssertNil(none.rate)
        XCTAssertNotNil(none.error)
    }

    /// An absurd rate from a source is never used: the next source answers; the ECB rate carries its own day.
    func testFxSkipsAbsurdRates() async {
        let yahoo = #"{"chart":{"result":[{"meta":{"regularMarketPrice":108.5,"regularMarketTime":1}}]}}"#
        let ecb = "<Cube time='2026-10-30'><Cube currency='USD' rate='0.0001'/></Cube>"
        let frank = #"{"rates":{"EUR":0.92}}"#
        let r = await LocalFx.read(fetcher: FakeFetcher(routes: [("https://query1.finance.yahoo.com", yahoo), ("https://www.ecb.europa.eu", ecb),
                                                                 ("https://api.frankfurter.dev", frank)]), now: 5)
        XCTAssertEqual(r.source, "Frankfurter")
        XCTAssertEqual(r.usdPerEur ?? 0, 1 / 0.92, accuracy: 1e-12)
        let good = "<Cube time='2026-10-30'><Cube currency='USD' rate='1.0850'/></Cube>"
        let e = await LocalFx.read(fetcher: FakeFetcher(routes: [("https://query1.finance.yahoo.com", yahoo), ("https://www.ecb.europa.eu", good)]), now: 5)
        XCTAssertEqual(e.source, "BCE")
        XCTAssertEqual(e.time, 1_793_372_400_000, "le jour du taux de la BCE, 15 h UTC")
        let none = await LocalFx.read(fetcher: FakeFetcher(routes: [("https://query1.finance.yahoo.com", yahoo)]), now: 5)
        XCTAssertNil(none.rate, "un taux absurde n'est jamais utilisé")
    }

    /// The real providers (`ALTIM_LIVE=1 swift test --filter LocalBackendTests/testLive`): skipped by default.
    func testLiveMarkets() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "réseau réel non demandé")
        let backend = LocalBackend()
        for (sym, kind, tf) in [("BTC", "crypto", "4h"), ("ETH", "crypto", "1d"), ("SOL", "crypto", "1w"), ("AAPL", "stock", "1d"), ("NVDA", "stock", "4h"), ("MSFT", "stock", "4d")] {
            let r = await backend.answer(path: "/api/candles", query: ["symbol": sym, "kind": kind, "interval": tf])!
            guard let s = try? JSONDecoder().decode(Snapshot.self, from: r.0) else {
                print("\(sym) \(tf): \(String(decoding: r.0, as: UTF8.self))")
                continue
            }
            print("\(sym) \(tf): \(s.candles.count) bougies, \(s.agreeing) sources d'accord, fiabilité \(s.reliability.level) \(s.reliability.score)",
                  s.sources.map { "\($0.name)=\($0.ok ? "ok" : $0.error ?? "écart \($0.deviation ?? -1)")" }.joined(separator: " "))
            XCTAssertGreaterThan(s.candles.count, 60)
        }
        let radar = await backend.answer(path: "/api/radar", query: ["symbols": "BTC:crypto,ETH:crypto,AAPL:stock", "interval": "4h"])!
        for row in try JSONDecoder().decode([RadarRow].self, from: radar.0) {
            print("radar \(row.symbol): \(row.signal?.action.label ?? row.error ?? "?") prix \(row.price ?? 0) (\(row.priceSources ?? "-"))")
        }
        let q = await backend.answer(path: "/api/tickers", query: ["symbols": "BTC:crypto,AAPL:stock"])!
        print(String(decoding: q.0, as: UTF8.self).prefix(600))
        let fx = await backend.answer(path: "/api/fx", query: [:])!
        print(String(decoding: fx.0, as: UTF8.self))
        let search = await backend.answer(path: "/api/search", query: ["q": "tesla"])!
        print(String(decoding: search.0, as: UTF8.self).prefix(300))
    }
}

extension LocalBackendTests {
    func testSectorsFromTheTableThenYahoo() async throws {
        let yahoo = #"{"quotes":[{"symbol":"ZZZ","quoteType":"EQUITY","sector":"Financial Services"}]}"#
        let backend = LocalBackend(fetcher: FakeFetcher(routes: [("https://query1.finance.yahoo.com/v1/finance/search?q=ZZZ", yahoo)]))
        let r = await backend.answer(path: "/api/sectors", query: ["symbols": "AAPL,spy,ZZZ,QQQQ"])!
        let rep = try JSONDecoder().decode(SectorsReport.self, from: r.0)
        let by = rep.bySymbol
        XCTAssertEqual(by["AAPL"]?.sector, "Technologie")
        XCTAssertEqual(by["AAPL"]?.classification, .nasdaq)
        XCTAssertEqual(by["SPY"]?.etf, true)
        XCTAssertEqual(by["ZZZ"]?.sector, "Finance")
        XCTAssertNil(by["QQQQ"]?.sector)
        XCTAssertNotNil(by["QQQQ"]?.reason)
        XCTAssertEqual(rep.sources.first { $0.name == "Yahoo Finance" }?.ok, false)
    }
}

extension LocalBackendTests {
    /// Yahoo's 5-day chart: the day's change is against the previous daily close, not `chartPreviousClose` (5 days ago).
    func testYahooQuoteChangeIsOneDay() async throws {
        let body = #"{"chart":{"result":[{"meta":{"regularMarketPrice":110,"chartPreviousClose":80},"timestamp":[1,2,3],"indicators":{"quote":[{"close":[90,100,110]}]}}]}}"#
        let yahoo = QuoteSources.stock.first { $0.name == "Yahoo Finance" }!
        let q = try await yahoo.fetch(FakeFetcher(routes: [("https://query1.finance.yahoo.com/v8/finance/chart/AAPL", body)]), "AAPL")
        XCTAssertEqual(q.price, 110)
        XCTAssertEqual(q.change!, 10, accuracy: 1e-9)
    }
}

extension LocalBackendTests {
    func testLiveTimings() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "réseau réel non demandé")
        /// Times each source of the BTC 1 d candles and of the AAPL quote.
        struct Timed: MarketFetcher {
            let inner = URLSessionFetcher()
            func data(_ url: String, method: String, body: Data?) async throws -> Data {
                let t = Date()
                defer { print(String(format: "  %.2f s  ", Date().timeIntervalSince(t)) + String(url.prefix(70))) }
                return try await inner.data(url, method: method, body: body)
            }
        }
        _ = try await LocalSnapshot.build(Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), .d1, fetcher: Timed())
        let backend = LocalBackend()
        let t = Date()
        let r = await backend.answer(path: "/api/radar", query: ["symbols": Asset.defaults.map { "\($0.symbol):\($0.kind.rawValue)" }.joined(separator: ","), "interval": "4h"])!
        print(String(format: "radar 8 actifs à froid : %.1f s", Date().timeIntervalSince(t)), r.1)
        let t2 = Date()
        _ = await backend.answer(path: "/api/radar", query: ["symbols": Asset.defaults.map { "\($0.symbol):\($0.kind.rawValue)" }.joined(separator: ","), "interval": "4h"])!
        print(String(format: "radar en cache : %.2f s", Date().timeIntervalSince(t2)))
    }
}

extension LocalBackendTests {
    /// Every EUR/USD source down: the last rate read comes back marked stale, and the sources are not asked again
    /// at every call (at most once a minute).
    func testFxKeepsTheLastRateWhenSourcesFail() async {
        final class Switch: MarketFetcher, @unchecked Sendable {
            private let lock = NSLock()
            private var down = false, n = 0
            func fail() { lock.lock(); down = true; lock.unlock() }
            var count: Int { lock.lock(); defer { lock.unlock() }; return n }
            func data(_ url: String, method: String, body: Data?) async throws -> Data {
                lock.lock(); n += 1; let d = down; lock.unlock()
                if d { throw MarketError("HTTP 503") }
                return Data(#"{"chart":{"result":[{"meta":{"regularMarketPrice":1.08,"regularMarketTime":1800000000}}]}}"#.utf8)
            }
        }
        let f = Switch()
        let b = LocalBackend(fetcher: f, bot: false)
        let ok = await b.fx()
        XCTAssertEqual(ok.usdPerEur ?? 0, 1.08, accuracy: 1e-9)
        await b.setFxTTL(0)
        f.fail()
        let stale = await b.fx()
        XCTAssertEqual(stale.usdPerEur ?? 0, 1.08, accuracy: 1e-9, "dernier taux connu")
        XCTAssertEqual(stale.stale, true)
        XCTAssertNotNil(Fx.parse(stale), "toujours utilisable par l'app")
        let asked = f.count
        _ = await b.fx()
        XCTAssertEqual(f.count, asked, "pas de nouvelle tentative avant une minute")
    }
}
