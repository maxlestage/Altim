import XCTest
@testable import AltimCore

/// Live prices: same feeds, messages and rules as web/server/live.ts
/// (live-samples.json: real BTC messages captured on each stream, shared with web/test/live.test.ts).
final class StreamingTests: XCTestCase {
    static let samples: [String: [Any]] = {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("live-samples.json")
        return (try? JSONSerialization.jsonObject(with: Data(contentsOf: url))) as? [String: [Any]] ?? [:]
    }()

    func parseAll(_ spec: LiveFeeds.Spec) -> [LiveFeeds.Update] {
        var ids: [Int: String] = [:]
        return (Self.samples[spec.name] ?? []).flatMap { spec.parse($0, &ids) }
    }

    func testEachFeedDecodesARealMessage() {
        XCTAssertEqual(LiveFeeds.all.map(\.name), ["OKX", "Coinbase", "Kraken", "Bitfinex", "Bitget", "Gate.io", "Crypto.com"])
        let expected: [String: (Double, Double)] = [
            "OKX": (84595.2, (84595.2 / 84060.1 - 1) * 100),
            "Coinbase": (84581.99, (84581.99 / 84146.86 - 1) * 100),
            "Kraken": (84576, 0.63),
            "Bitfinex": (84529, 0.418166),
            "Bitget": (84608.29, (84608.29 / 84161.83 - 1) * 100),
            "Gate.io": (84596.4, 0.6384),
            "Crypto.com": (84600.03, 0.64),
        ]
        for spec in LiveFeeds.all {
            let u = parseAll(spec).first
            XCTAssertEqual(u?.base, "BTC", spec.name)
            XCTAssertEqual(u?.price, expected[spec.name]!.0, spec.name)
            XCTAssertEqual(u?.change ?? .nan, expected[spec.name]!.1, accuracy: 1e-6, spec.name)
        }
    }

    func testMessagesWithoutPriceAndSubscriptions() throws {
        for spec in LiveFeeds.all {
            var ids: [Int: String] = [:]
            XCTAssertEqual(spec.parse(["event": "subscribe"], &ids), [], spec.name)
            XCTAssertEqual(spec.parse([1, "hb"], &ids), [], spec.name)
            XCTAssertFalse(spec.subscribe(["BTC", "PEPE"]).isEmpty)
        }
        let sub = try XCTUnwrap(LiveFeeds.bitfinex.subscribe(["DOGE"]).first)
        XCTAssertTrue(sub.contains("tDOGE:USD"))
        var ids: [Int: String] = [:]
        _ = LiveFeeds.bitfinex.parse(["event": "subscribed", "channel": "ticker", "chanId": 7, "symbol": "tDOGE:USD"], &ids)
        XCTAssertEqual(ids[7], "DOGE")
        let reply = try XCTUnwrap(LiveFeeds.cryptoCom.reply(["id": 42, "method": "public/heartbeat"]))
        let obj = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(reply.utf8)) as? [String: Any])
        XCTAssertEqual(obj["method"] as? String, "public/respond-heartbeat")
        XCTAssertEqual(obj["id"] as? Int, 42)
        XCTAssertNil(LiveFeeds.cryptoCom.reply(["method": "subscribe"]))
    }

    func testConsensusMedianOutlierAndStale() {
        let now = Date(timeIntervalSince1970: 1_000)
        let q: [String: LiveConsensus.SourceQuote] = [
            "A": .init(price: 100, change: 1, time: now.addingTimeInterval(-1)),
            "B": .init(price: 100.4, change: 2, time: now.addingTimeInterval(-0.5)),
            "C": .init(price: 99.8, change: nil, time: now),
            "D": .init(price: 110, change: 9, time: now), // 10 % away: ignored
            "E": .init(price: 50, change: 0, time: now.addingTimeInterval(-700)), // stale: ignored
        ]
        let c = LiveConsensus.combine(q, assetID: "crypto:X", now: now, maxAge: 600)!
        XCTAssertEqual(c.price, 100)
        XCTAssertEqual(c.change, 1.5)
        XCTAssertEqual(c.agreeing, 3)
        XCTAssertEqual(c.total, 4)
        XCTAssertEqual(c.sources, ["A", "B", "C"])
        XCTAssertEqual(c.time, now)
        XCTAssertNil(LiveConsensus.combine([:], assetID: "x", now: now))
    }

    func testUSMarketHours() {
        let d = { (s: String) in ISO8601DateFormatter().date(from: s)! }
        XCTAssertTrue(USMarket.isOpen(d("2026-09-28T14:00:00Z")))  // Monday 10:00 EDT
        XCTAssertFalse(USMarket.isOpen(d("2026-09-28T13:29:00Z")))
        XCTAssertTrue(USMarket.isOpen(d("2026-09-28T19:59:00Z")))
        XCTAssertFalse(USMarket.isOpen(d("2026-09-28T20:00:00Z")))
        XCTAssertFalse(USMarket.isOpen(d("2026-09-27T15:00:00Z"))) // Sunday
        XCTAssertTrue(USMarket.isOpen(d("2026-12-01T14:31:00Z")))  // Tuesday 9:31 EST
        XCTAssertFalse(USMarket.isOpen(d("2026-12-01T14:29:00Z")))
    }

    static let btc = Asset(symbol: "BTCUSDT", name: "Bitcoin", assetClass: .crypto, quote: "USDT")
    static let aapl = Asset(symbol: "AAPL", name: "Apple", assetClass: .stock, quote: "USD")

    func testHubStreamsLatestStateThrottledAndOnlyOnChange() async throws {
        let hub = LivePriceHub(feeds: [], pollEvery: 3600) { _ in [] }
        let stream = await hub.stream([Self.btc])
        var it = stream.makeAsyncIterator()
        await hub.handle(LiveFeeds.okx, #"{"arg":{"channel":"tickers"},"data":[{"instId":"BTC-USDT","last":"100","open24h":"99"}]}"#)
        let first = await it.next()
        XCTAssertEqual(first?.price, 100)
        XCTAssertEqual(first?.assetID, "crypto:BTCUSDT")
        XCTAssertNil(first?.marketOpen)
        // A burst within 250 ms: only the latest state is sent.
        for p in 101...120 { await hub.ingest(Self.btc.id, source: "OKX", price: Double(p), change: nil) }
        await hub.ingest(Self.btc.id, source: "OKX", price: 120, change: nil)
        let second = await it.next()
        XCTAssertEqual(second?.price, 120)
        let watched = await hub.watched
        XCTAssertEqual(watched, 1)
        // A new subscriber gets the last price right away.
        var it2 = await hub.stream([Self.btc]).makeAsyncIterator()
        let snap = await it2.next()
        XCTAssertEqual(snap?.price, 120)
        await hub.close()
    }

    func testHubPollsStocksAndSilentCryptos() async throws {
        let calls = Calls()
        let hub = LivePriceHub(feeds: [LiveFeeds.okx].map { $0 }, session: .shared, pollEvery: 3600) { asset in
            await calls.add(asset.symbol)
            return asset.assetClass == .stock
                ? [(source: "Robinhood", quote: Quote(price: 200, changePercent24h: 1, time: Date())),
                   (source: "Webull", quote: Quote(price: 200.2, changePercent24h: 1.1, time: Date()))]
                : [(source: "Consensus", quote: Quote(price: 50, changePercent24h: 0, time: Date()))]
        }
        await hub.ingest(Self.btc.id, source: "OKX", price: 100, change: nil)
        _ = await hub.stream([Self.aapl])
        await hub.poll()
        let polled = await calls.list
        XCTAssertTrue(polled.contains("AAPL"))
        var it = await hub.stream([Self.aapl]).makeAsyncIterator()
        let t = await it.next()
        XCTAssertEqual(t?.price ?? 0, 200.1, accuracy: 1e-9)
        XCTAssertEqual(t?.agreeing, 2)
        XCTAssertNotNil(t?.marketOpen)
        await hub.close()
    }

    /// Real streams: `ALTIM_LIVE=1 swift test --filter StreamingTests` (Apple platforms: Linux's libcurl has no WebSockets).
    func testLiveExchangesStreamBitcoin() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "ALTIM_LIVE=1 pour les tests réseau")
        #if os(Linux)
        throw XCTSkip("WebSockets indisponibles dans FoundationNetworking (Linux)")
        #else
        let hub = LivePriceHub(pollEvery: 3600) { _ in [] }
        let stream = await hub.stream([Self.btc])
        let stop = Task { try await Task.sleep(nanoseconds: 15_000_000_000); await hub.close() }
        var ticks: [LiveTick] = []
        for await t in stream {
            ticks.append(t)
            if ticks.count >= 5 && t.agreeing >= 4 { break }
        }
        stop.cancel()
        await hub.close()
        let last = try XCTUnwrap(ticks.last)
        print("BTC en direct :", last.price, "\(last.agreeing)/\(last.total)", last.sources, "\(ticks.count) ticks")
        XCTAssertGreaterThanOrEqual(last.agreeing, 4)
        XCTAssertGreaterThan(last.price, 1000)
        #endif
    }

    /// Real polling (every platform): stocks from the fast quote sources, cryptos without a live feed from the consensus.
    func testLiveStandardHubPollsStocks() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "ALTIM_LIVE=1 pour les tests réseau")
        let transport = URLSessionTransport()
        let hub = LivePriceHub.standard(transport: transport, market: .standard(transport: transport))
        let stream = await hub.stream([Self.aapl])
        let stop = Task { try await Task.sleep(nanoseconds: 30_000_000_000); await hub.close() }
        var tick: LiveTick?
        for await t in stream { tick = t; break }
        stop.cancel()
        await hub.close()
        let t = try XCTUnwrap(tick)
        print("AAPL en direct :", t.price, "\(t.agreeing)/\(t.total)", t.sources, t.marketOpen as Any)
        XCTAssertGreaterThanOrEqual(t.agreeing, 3)
        XCTAssertNotNil(t.marketOpen)
    }
}

actor Calls {
    var list: [String] = []
    func add(_ s: String) { list.append(s) }
}
