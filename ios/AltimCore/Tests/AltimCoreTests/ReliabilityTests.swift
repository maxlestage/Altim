import XCTest
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
@testable import AltimCore

final class SourceParsingTests: XCTestCase {
    let now = Date(timeIntervalSince1970: 1_790_500_200) // during the 1790499600 candle
    func d(_ s: String) -> Data { Data(s.utf8) }

    func testOKX() throws {
        let c = try OKXMarketData.parse(d(Samples.okx))
        XCTAssertEqual(c.map(\.time.timeIntervalSince1970), [1_790_492_400, 1_790_496_000, 1_790_499_600])
        XCTAssertEqual(c[1].close, 84791.7)
        XCTAssertEqual(c[1].high, 84885)
        XCTAssertTrue(c[1].isClosed)
        XCTAssertFalse(c[2].isClosed)
    }

    func testCoinbase() throws {
        let c = try CoinbaseMarketData.parse(d(Samples.coinbase), step: 3600, now: now)
        XCTAssertEqual(c.first?.time.timeIntervalSince1970, 1_790_492_400)
        XCTAssertEqual(c[2].open, 84775.68)
        XCTAssertEqual(c[2].low, 84659.52)
        XCTAssertEqual(c[2].close, 84662.29)
        XCTAssertTrue(c.allSatisfy(\.isValid))
        XCTAssertFalse(c[2].isClosed)
        XCTAssertTrue(c[1].isClosed)
    }

    func testKraken() throws {
        let c = try KrakenMarketData.parse(d(Samples.kraken), step: 3600, now: now)
        XCTAssertEqual(c.count, 3)
        XCTAssertEqual(c[0].close, 84767.9)
        XCTAssertEqual(c[0].volume, 32.90613655)
        XCTAssertFalse(c[2].isClosed)
        XCTAssertEqual(KrakenMarketData.pair("BTC"), "XBTUSD")
        XCTAssertThrowsError(try KrakenMarketData.parse(d(#"{"error":["EQuery:Unknown asset pair"]}"#), step: 3600, now: now))
    }

    func testKuCoin() throws {
        let c = try KuCoinMarketData.parse(d(Samples.kucoin), step: 3600, now: now)
        XCTAssertEqual(c.map(\.time.timeIntervalSince1970), [1_790_492_400, 1_790_496_000, 1_790_499_600])
        XCTAssertEqual(c[2].open, 84795.6)
        XCTAssertEqual(c[2].close, 84612.8)
        XCTAssertEqual(c[2].high, 84795.6)
        XCTAssertEqual(c[2].low, 84600.3)
        XCTAssertTrue(c.allSatisfy(\.isValid))
    }

    func testGate() throws {
        let c = try GateMarketData.parse(d(Samples.gate))
        XCTAssertEqual(c[0].open, 84519.9)
        XCTAssertEqual(c[0].close, 84790)
        XCTAssertEqual(c[0].volume, 75.915673)
        XCTAssertTrue(c[0].isClosed)
        XCTAssertFalse(c[2].isClosed)
        XCTAssertTrue(c.allSatisfy(\.isValid))
    }

    func testBitfinex() throws {
        let c = try BitfinexMarketData.parse(d(Samples.bitfinex), step: 3600, now: now)
        XCTAssertEqual(c.first?.time.timeIntervalSince1970, 1_790_492_400)
        XCTAssertEqual(c[2].open, 84713)
        XCTAssertEqual(c[2].close, 84537)
        XCTAssertTrue(c.allSatisfy(\.isValid))
        XCTAssertEqual(BitfinexMarketData.symbol("DOGE"), "tDOGE:USD")
    }

    func testCoinGeckoAndFearGreed() throws {
        let q = try CoinGeckoQuotes.parse(d(Samples.coingecko), id: "bitcoin")
        XCTAssertEqual(q.price, 84614)
        let f = try SentimentProvider.parse(d(Samples.fng))
        XCTAssertEqual(f.value, 70)
        XCTAssertEqual(f.label, "Avidité")
    }

    // Keyed sources: documented formats.
    func testTwelveData() throws {
        let json = #"{"meta":{"symbol":"AAPL","interval":"1day"},"values":[{"datetime":"2026-09-25","open":"250.1","high":"252.0","low":"249.0","close":"251.5","volume":"50000000"},{"datetime":"2026-09-24","open":"248.0","high":"250.5","low":"247.5","close":"250.0","volume":"45000000"}],"status":"ok"}"#
        let c = try TwelveDataMarketData.parse(d(json), timeframe: .d1, isStock: true, now: now)
        XCTAssertEqual(c.map(\.close), [250.0, 251.5])
        XCTAssertEqual(Calendar(identifier: .gregorian).dateComponents(in: TimeZone(identifier: "UTC")!, from: c[0].time).hour, 13)
        XCTAssertThrowsError(try TwelveDataMarketData.parse(d(#"{"code":401,"message":"Invalid API key","status":"error"}"#), timeframe: .d1, isStock: true, now: now))
    }

    func testPolygon() throws {
        let json = #"{"ticker":"AAPL","status":"OK","resultsCount":2,"results":[{"v":1000,"o":250,"c":251,"h":252,"l":249,"t":1790380800000},{"v":1200,"o":251,"c":253,"h":254,"l":250,"t":1790467200000}]}"#
        let c = try PolygonMarketData.parse(d(json), timeframe: .d1, now: now)
        XCTAssertEqual(c.map(\.close), [251, 253])
        XCTAssertTrue(c[0].isClosed)
    }

    func testAlpacaBars() throws {
        let json = #"{"bars":[{"t":"2026-09-25T13:30:00Z","o":250,"h":252,"l":249,"c":251,"v":1000,"n":10,"vw":250.5}],"symbol":"AAPL","next_page_token":null}"#
        let c = try AlpacaMarketData.parse(d(json), timeframe: .h1, now: now)
        XCTAssertEqual(c.first?.close, 251)
    }

    func testFinnhub() throws {
        let q = try FinnhubQuotes.parse(d(#"{"c":251.3,"d":1.2,"dp":0.48,"h":252,"l":249,"o":250,"pc":250.1,"t":1790467200}"#))
        XCTAssertEqual(q.price, 251.3)
        XCTAssertThrowsError(try FinnhubQuotes.parse(d(#"{"c":0,"d":null,"dp":null,"h":0,"l":0,"o":0,"pc":0,"t":0}"#)))
    }

    func testNasdaqAndCboe() throws {
        let json = #"{"data":{"symbol":"AAPL","totalRecords":2,"tradesTable":{"asOf":null,"rows":[{"date":"09/25/2026","close":"$341.07","volume":"30,002,510","open":"$336.04","high":"$341.67","low":"$334.53"},{"date":"01/15/2026","close":"$230.00","volume":"1,000","open":"$229.00","high":"$231.00","low":"$228.00"}]}},"status":{"rCode":200}}"#
        let c = try NasdaqMarketData.parse(d(json), now: now)
        XCTAssertEqual(c.map(\.close), [230, 341.07])
        XCTAssertEqual(c[1].volume, 30_002_510)
        // Session open: 13:30 UTC in summer, 14:30 UTC in winter (US daylight saving time).
        XCTAssertEqual(c[1].time.timeIntervalSince1970, 1_790_343_000) // same timestamp as Yahoo
        let utc = Calendar(identifier: .gregorian).dateComponents(in: TimeZone(identifier: "UTC")!, from: c[0].time)
        XCTAssertEqual(utc.hour, 14)
        XCTAssertEqual(try NasdaqMarketData.parse(d(#"{"data":null,"status":{"rCode":400}}"#), now: now).count, 0)

        let q = try CboeQuotes.parse(d(#"{"success":true,"details":{"symbol":"AAPL","current_price":341.4603,"price_change_percent":1.51}}"#))
        XCTAssertEqual(q.price, 341.4603)
    }

    func testStockTwitsSentiment() throws {
        let json = #"{"messages":[{"entities":{"sentiment":{"basic":"Bullish"}}},{"entities":{"sentiment":{"basic":"Bearish"}}},{"entities":{"sentiment":{"basic":"Bullish"}}},{"entities":{"sentiment":null}}]}"#
        let s = try SentimentProvider.parseSocial(d(json))
        XCTAssertEqual(s.sampleSize, 3)
        XCTAssertEqual(s.bullishPercent!, 200.0 / 3, accuracy: 1e-9)
    }

    func testAggregator() {
        let base = 1_790_467_200.0 // 00:00 UTC, multiple of 4h
        let hourly = (0..<10).map { (i: Int) -> Candle in
            let x = Double(i)
            return Candle(time: Date(timeIntervalSince1970: base + x * 3600), open: 100 + x, high: 101 + x,
                          low: 99 + x, close: 100.5 + x, volume: 1, isClosed: i < 9)
        }
        let h4 = CandleAggregator.aggregate(hourly, from: 3600, to: 14_400)
        XCTAssertEqual(h4.count, 3)
        XCTAssertEqual(h4[0].open, 100)
        XCTAssertEqual(h4[0].close, 103.5)
        XCTAssertEqual(h4[0].high, 104)
        XCTAssertTrue(h4[1].isClosed)
        XCTAssertFalse(h4[2].isClosed) // current bucket: 2 of 4 hours, the last one unfinished
        // An incomplete first bucket is dropped.
        XCTAssertEqual(CandleAggregator.aggregate(Array(hourly.dropFirst()), from: 3600, to: 14_400).first?.time.timeIntervalSince1970, base + 14_400)
    }
}

// MARK: - Consensus

struct FakeSource: MarketSource {
    let name: String
    var assetClass: AssetClass = .crypto
    var candlesResult: Result<[Candle], APIError>
    var quoteOnly: Double?
    var providesCandles: Bool { quoteOnly == nil }

    func candles(for asset: Asset, timeframe: Timeframe, limit: Int) async throws -> [Candle] { try candlesResult.get() }
    func quote(for asset: Asset) async throws -> Quote {
        if let q = quoteOnly { return Quote(price: q, changePercent24h: 0, time: Date()) }
        return Quote(price: try candlesResult.get().last!.close, changePercent24h: 0, time: Date())
    }
}

final class ConsensusTests: XCTestCase {
    let btc = Asset.defaults[0]
    let base = Fixtures.candles(count: 300, drift: 0.001)
    var now: Date { base.last!.time.addingTimeInterval(3600) }

    func scaled(_ factor: Double) -> [Candle] {
        base.map { Candle(time: $0.time, open: $0.open * factor, high: $0.high * factor, low: $0.low * factor,
                          close: $0.close * factor, volume: $0.volume) }
    }

    func testAllAgree() async throws {
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "A", candlesResult: .success(base)),
            FakeSource(name: "B", candlesResult: .success(scaled(1.0005))),
            FakeSource(name: "C", candlesResult: .success(scaled(0.9998))),
        ])
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertEqual(s.primarySource, "A")
        XCTAssertEqual(s.agreeingSources, 3)
        XCTAssertEqual(s.reliability, .high)
        XCTAssertFalse(s.conflict)
    }

    func testDivergentPrimaryIsReplaced() async throws {
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "Faux", candlesResult: .success(scaled(1.05))), // bad price +5 %
            FakeSource(name: "B", candlesResult: .success(base)),
            FakeSource(name: "C", candlesResult: .success(scaled(1.0003))),
        ])
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertEqual(s.primarySource, "B")
        XCTAssertEqual(s.checks.first { $0.name == "Faux" }?.status, .diverges)
        // Median of B and C (the false source excluded): within 0.02 % of the true price.
        XCTAssertEqual(s.candles.last!.close, base.last!.close, accuracy: base.last!.close * 0.0002)
    }

    func testEverySourceIsQueried() async throws {
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "Panne", candlesResult: .failure(.unavailableInRegion)),
            FakeSource(name: "Vide", candlesResult: .success([])),
            FakeSource(name: "B", candlesResult: .success(base)),
            FakeSource(name: "C", candlesResult: .success(scaled(1.0001))),
            FakeSource(name: "D", candlesResult: .success(scaled(0.9999))),
            FakeSource(name: "E", candlesResult: .success(base)),
        ])
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertEqual(s.primarySource, "B")
        XCTAssertEqual(s.agreeingSources, 4)
        XCTAssertFalse(s.checks.contains { $0.status == .skipped })
        XCTAssertEqual(s.reliabilityScore, 90)
        XCTAssertEqual(s.summary.prefix(24), "4/4 sources concordantes")
        if case .failed = s.checks.first(where: { $0.name == "Panne" })?.status {} else { XCTFail("Panne should be failed") }
    }

    func testTargetStillLimitsWaves() async throws {
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "Panne", candlesResult: .failure(.unavailableInRegion)),
            FakeSource(name: "B", candlesResult: .success(base)),
            FakeSource(name: "C", candlesResult: .success(scaled(1.0001))),
            FakeSource(name: "D", candlesResult: .success(scaled(0.9999))),
            FakeSource(name: "E", candlesResult: .success(base)),
        ], targetSources: 3)
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertEqual(s.agreeingSources, 3)
        XCTAssertEqual(s.checks.first { $0.name == "E" }?.status, .skipped)
    }

    func testMinorityAgreementIsConflict() async throws {
        // 2 sources agree, 3 others are all far apart: fewer than half agree.
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "A", candlesResult: .success(base)),
            FakeSource(name: "B", candlesResult: .success(scaled(1.0002))),
            FakeSource(name: "C", candlesResult: .success(scaled(1.08))),
            FakeSource(name: "D", candlesResult: .success(scaled(0.9))),
            FakeSource(name: "E", candlesResult: .success(scaled(1.2))),
        ])
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertTrue(s.conflict)
        XCTAssertEqual(s.reliability, .low)
    }

    func testMajorityAgreementIsNotConflict() async throws {
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "A", candlesResult: .success(base)),
            FakeSource(name: "B", candlesResult: .success(scaled(1.0002))),
            FakeSource(name: "C", candlesResult: .success(scaled(0.9998))),
            FakeSource(name: "Faux", candlesResult: .success(scaled(1.08))),
        ])
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertFalse(s.conflict)
        XCTAssertEqual(s.reliabilityScore, 75)
        XCTAssertEqual(s.checks.first { $0.name == "Faux" }?.status, .diverges)
    }

    func testConsensusCandlesAreTheMedianOfAgreeingSources() async throws {
        // Bad tick on one exchange: absurd wick on candle 150.
        let spiked = scaled(1.0002).enumerated().map { i, c in
            i == 150 ? Candle(time: c.time, open: c.open, high: c.high * 1.3, low: c.low, close: c.close * 1.02, volume: c.volume) : c
        }
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "A", candlesResult: .success(base)),
            FakeSource(name: "B", candlesResult: .success(spiked)),
            FakeSource(name: "C", candlesResult: .success(scaled(0.9998))),
            FakeSource(name: "Faux", candlesResult: .success(scaled(1.05))),
        ])
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertEqual(s.primarySource, "A")
        XCTAssertEqual(s.candles.count, base.count)
        XCTAssertEqual(s.candles[150].close, base[150].close, accuracy: 1e-9)
        XCTAssertLessThan(s.candles[150].high, base[150].high * 1.001)
        XCTAssertEqual(s.candles[10].close, base[10].close, accuracy: 1e-9)
        XCTAssertTrue(s.candles.allSatisfy { $0.high >= max($0.open, $0.close) && $0.low <= min($0.open, $0.close) })
        XCTAssertEqual(ConsensusMarketData.blend(base, []), base)
    }

    func testSingleSourceIsCapped() async throws {
        let c = ConsensusMarketData(sources: [FakeSource(name: "Seule", candlesResult: .success(base))])
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertEqual(s.reliabilityScore, 40)
        XCTAssertEqual(s.reliability, .low)
    }

    func testSameProviderCountsOnce() async throws {
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "Yahoo Finance", candlesResult: .success(base)),
            FakeSource(name: "Yahoo Finance (2)", candlesResult: .success(base)),
        ])
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertEqual(s.agreeingSources, 2)
        XCTAssertEqual(s.independentSources, 1)
        XCTAssertEqual(s.reliabilityScore, 40)
    }

    func testConflictGivesLowReliability() async throws {
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "A", candlesResult: .success(base)),
            FakeSource(name: "B", candlesResult: .success(scaled(1.1))),
        ])
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertTrue(s.conflict)
        XCTAssertEqual(s.reliability, .low)
    }

    func testAllFail() async {
        let c = ConsensusMarketData(sources: [FakeSource(name: "A", candlesResult: .failure(.invalidResponse))])
        do {
            _ = try await c.snapshot(for: btc, timeframe: .h1, now: now)
            XCTFail("Erreur attendue")
        } catch {
            XCTAssertTrue(error.localizedDescription.contains("Toutes les sources"))
        }
    }

    func testQuoteSourceCrossCheck() async throws {
        let last = base.last!.close
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "A", candlesResult: .success(base)),
            FakeSource(name: "B", candlesResult: .success(base)),
            FakeSource(name: "Cours OK", candlesResult: .success([]), quoteOnly: last * 1.001),
            FakeSource(name: "Cours faux", candlesResult: .success([]), quoteOnly: last * 1.2),
        ])
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: now)
        XCTAssertEqual(s.checks.first { $0.name == "Cours OK" }?.status, .agrees)
        XCTAssertEqual(s.checks.first { $0.name == "Cours faux" }?.status, .diverges)
        XCTAssertEqual(s.consensusPrice!, last, accuracy: last * 0.001)
    }

    func testMisalignedTimestampsFallBackToLastClose() {
        let shifted = base.map { Candle(time: $0.time.addingTimeInterval(1800), open: $0.open, high: $0.high,
                                        low: $0.low, close: $0.close, volume: $0.volume) }
        let dev = ConsensusMarketData.deviations([base, shifted])
        XCTAssertEqual(dev[0]!, 0, accuracy: 1e-9)
        XCTAssertEqual(dev[1]!, 0, accuracy: 1e-9)
    }

    func testReliabilityGateSuspendsSignal() async throws {
        let c = ConsensusMarketData(sources: [
            FakeSource(name: "A", candlesResult: .success(Fixtures.candles(count: 300, drift: 0.004))),
            FakeSource(name: "B", candlesResult: .success(Fixtures.candles(count: 300, drift: 0.004).map {
                Candle(time: $0.time, open: $0.open * 2, high: $0.high * 2, low: $0.low * 2, close: $0.close * 2, volume: $0.volume)
            })),
        ])
        let candles = Fixtures.candles(count: 300, drift: 0.004)
        let s = try await c.snapshot(for: btc, timeframe: .h1, now: candles.last!.time.addingTimeInterval(3600))
        let raw = Signal(action: .strongBuy, score: 60, confidence: 80, price: 100, time: Date(),
                         factors: [], plan: nil, warnings: [])
        let gated = ReliabilityGate.apply(raw, snapshot: s)
        XCTAssertEqual(gated.action, .hold)
        XCTAssertTrue(gated.warnings.first?.contains("signal suspendu") ?? false)
        XCTAssertEqual(gated.confidence, 80 * s.reliabilityScore / 100, accuracy: 1e-9)
    }

    func testPriceGuard() {
        XCTAssertNil(PriceGuard.issue(brokerPrice: 100.5, consensusPrice: 100))
        XCTAssertNotNil(PriceGuard.issue(brokerPrice: 103, consensusPrice: 100))
        XCTAssertNotNil(PriceGuard.issue(brokerPrice: 100, consensusPrice: nil))
    }
}

// MARK: - Data quality

final class DataQualityTests: XCTestCase {
    func testCleanSeries() {
        let c = Fixtures.candles(count: 300, drift: 0.001)
        let r = DataQuality.assess(c, timeframe: .h1, assetClass: .crypto, now: c.last!.time.addingTimeInterval(3600))
        XCTAssertEqual(r.score, 100, "\(r.issues)")
    }

    func testDetectsGapsStaleAndBadTicks() {
        var c = Fixtures.candles(count: 300, drift: 0.001)
        c.removeSubrange(100..<105)                   // gap
        let i = 200, p = c[i]                          // aberrant spike, immediately reversed
        c[i] = Candle(time: p.time, open: p.open, high: p.close * 1.5, low: p.low, close: p.close * 1.5, volume: p.volume)
        c[i + 1] = Candle(time: c[i + 1].time, open: p.close * 1.5, high: p.close * 1.5, low: c[i + 1].low,
                          close: c[i + 1].close, volume: c[i + 1].volume)
        let r = DataQuality.assess(c, timeframe: .h1, assetClass: .crypto, now: c.last!.time.addingTimeInterval(86_400))
        XCTAssertEqual(r.gaps, 1)
        XCTAssertEqual(r.badTicks, 1)
        XCTAssertTrue(r.isStale)
        XCTAssertLessThan(r.score, 50)
    }

    func testStockWeekendIsNotAGap() {
        let c = Fixtures.candles(count: 300, drift: 0.001).enumerated().map { i, x in
            // one day per candle, skipping weekends (5 days out of 7)
            Candle(time: Date(timeIntervalSince1970: 1_700_000_000 + Double(i / 5 * 7 + i % 5) * 86_400),
                   open: x.open, high: x.high, low: x.low, close: x.close, volume: x.volume)
        }
        let r = DataQuality.assess(c, timeframe: .d1, assetClass: .stock, now: c.last!.time.addingTimeInterval(86_400))
        XCTAssertEqual(r.gaps, 0)
    }
}

// MARK: - Network and orders

actor FlakyTransport: HTTPTransport {
    var responses: [Result<(Int, String), URLError>]
    var calls = 0
    init(_ responses: [Result<(Int, String), URLError>]) { self.responses = responses }

    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        calls += 1
        let next = responses.isEmpty ? .success((200, "{}")) : responses.removeFirst()
        switch next {
        case let .success((status, body)):
            return (Data(body.utf8), HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: nil)!)
        case let .failure(e):
            throw e
        }
    }
}

final class ResilienceTests: XCTestCase {
    func req(_ method: String = "GET") -> URLRequest {
        var r = URLRequest(url: URL(string: "https://example.com/x")!)
        r.httpMethod = method
        return r
    }

    func testRetriesThenSucceeds() async throws {
        let inner = FlakyTransport([.failure(URLError(.timedOut)), .success((503, "")), .success((200, "ok"))])
        let t = ResilientTransport(inner: inner, sleep: { _ in })
        let (data, response) = try await t.send(req())
        XCTAssertEqual(response.statusCode, 200)
        XCTAssertEqual(String(data: data, encoding: .utf8), "ok")
        let calls = await inner.calls
        XCTAssertEqual(calls, 3)
    }

    func testNeverRetriesOrders() async {
        let inner = FlakyTransport([.failure(URLError(.timedOut)), .success((200, "ok"))])
        let t = ResilientTransport(inner: inner, sleep: { _ in })
        do { _ = try await t.send(req("POST")); XCTFail("Erreur attendue") } catch {}
        let calls = await inner.calls
        XCTAssertEqual(calls, 1)
    }

    func testCircuitBreakerOpensAndCloses() async throws {
        final class Clock: @unchecked Sendable { var now = Date(timeIntervalSince1970: 0) }
        let clock = Clock()
        let inner = FlakyTransport(Array(repeating: .failure(URLError(.cannotConnectToHost)), count: 9))
        var policy = ResilientTransport.Policy()
        policy.maxAttempts = 1
        let t = ResilientTransport(inner: inner, policy: policy, sleep: { _ in }, now: { clock.now })
        for _ in 0..<3 { _ = try? await t.send(req()) }
        do {
            _ = try await t.send(req())
            XCTFail("Disjoncteur attendu")
        } catch {
            XCTAssertEqual(error as? ResilientTransport.TransportError, .circuitOpen(host: "example.com"))
        }
        let callsWhileOpen = await inner.calls
        XCTAssertEqual(callsWhileOpen, 3)
        clock.now = clock.now.addingTimeInterval(61)
        let available = await t.isAvailable(host: "example.com")
        XCTAssertTrue(available)
    }
}

/// Transport that loses the connection on the order submission.
final class DroppingTransport: HTTPTransport, @unchecked Sendable {
    let mock: MockTransport
    let dropPath: String
    init(mock: MockTransport, dropPath: String) { self.mock = mock; self.dropPath = dropPath }

    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let result = try await mock.send(request)
        if request.httpMethod == "POST" && request.url!.path == dropPath { throw URLError(.networkConnectionLost) }
        return result
    }
}

final class OrderRecoveryTests: XCTestCase {
    func routes(orderLookup: (Int, String)) -> [String: (Int, String)] {
        [
            "GET /api/v3/time": (200, #"{"serverTime":1700000000000}"#),
            "GET /api/v3/exchangeInfo": (200, TradingTests.exchangeInfo),
            "GET /api/v3/ticker/price": (200, #"{"price":"65000.00"}"#),
            "POST /api/v3/order": (200, "{}"),
            "GET /api/v3/order": orderLookup,
        ]
    }

    func testBinanceRecoversOrderAfterConnectionLoss() async throws {
        let mock = MockTransport()
        mock.routes = routes(orderLookup: (200, #"{"orderId":99,"status":"FILLED","executedQty":"0.001","cummulativeQuoteQty":"65"}"#))
        let broker = BinanceBroker(apiKey: "k", secret: "s", environment: .test,
                                   transport: DroppingTransport(mock: mock, dropPath: "/api/v3/order"))
        let r = try await broker.place(OrderRequest(symbol: "BTCUSDT", side: .buy, type: .market, quantity: Decimal(string: "0.001")!))
        XCTAssertEqual(r.orderId, "99")
        XCTAssertTrue(r.notes.contains { $0.contains("retrouvé") })
        // The order was sent only once.
        XCTAssertEqual(mock.requests.filter { $0.httpMethod == "POST" && $0.url!.path == "/api/v3/order" }.count, 1)
        let lookup = try XCTUnwrap(mock.requests.first { $0.httpMethod == "GET" && $0.url!.path == "/api/v3/order" })
        let postQuery = mock.requests.first { $0.httpMethod == "POST" }!.url!.query!
        let clientId = postQuery.components(separatedBy: "newClientOrderId=")[1].components(separatedBy: "&")[0]
        XCTAssertTrue(lookup.url!.query!.contains("origClientOrderId=\(clientId)"))
    }

    func testBinanceReportsOrderNotSent() async {
        let mock = MockTransport()
        mock.routes = routes(orderLookup: (400, #"{"code":-2013,"msg":"Order does not exist."}"#))
        let broker = BinanceBroker(apiKey: "k", secret: "s", environment: .test,
                                   transport: DroppingTransport(mock: mock, dropPath: "/api/v3/order"))
        do {
            _ = try await broker.place(OrderRequest(symbol: "BTCUSDT", side: .buy, type: .market, quantity: Decimal(string: "0.001")!))
            XCTFail("Erreur attendue")
        } catch {
            XCTAssertTrue(error.localizedDescription.contains("non transmis"))
        }
    }
}
