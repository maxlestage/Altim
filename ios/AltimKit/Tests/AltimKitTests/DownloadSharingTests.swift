import XCTest
@testable import AltimKit

/// The same history asked by several screens at once (Selection, bots, time machine, market filter) is downloaded once.
final class DownloadSharingTests: XCTestCase {
    /// Answers Binance daily candles, counts every request.
    final class Counter: MarketFetcher, @unchecked Sendable {
        private let lock = NSLock()
        private var n = 0
        let body: String
        init(_ bars: [Bar]) {
            body = "[" + bars.map { "[\(Int($0.time)),\"\($0.open)\",\"\($0.high)\",\"\($0.low)\",\"\($0.close)\",\"\($0.volume)\"]" }.joined(separator: ",") + "]"
        }
        var count: Int { lock.lock(); defer { lock.unlock() }; return n }
        func data(_ url: String, method: String, body: Data?) async throws -> Data {
            lock.lock(); n += 1; lock.unlock()
            try await Task.sleep(nanoseconds: 20_000_000)
            guard url.hasPrefix("https://api.binance.com/api/v3/klines") else { throw MarketError("HTTP 404") }
            return Data(self.body.utf8)
        }
    }

    func testLongHistoryIsDownloadedOnceForSimultaneousCallers() async throws {
        let now = Date().timeIntervalSince1970 * 1000
        let bars = LocalBackendTests.trend(count: 400, step: 86_400_000, end: now - 86_400_000)
        let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
        let single = Counter(bars)
        _ = try await LocalBackend(fetcher: single, bot: false).longDaily(btc)
        let one = single.count
        let shared = Counter(bars)
        let backend = LocalBackend(fetcher: shared, bot: false)
        let got = await withTaskGroup(of: Int.self) { g in
            for _ in 0..<4 { g.addTask { (try? await backend.longDaily(btc))?.count ?? 0 } }
            var out: [Int] = []
            for await c in g { out.append(c) }
            return out
        }
        XCTAssertEqual(got.count, 4)
        XCTAssertTrue(got.allSatisfy { $0 > 300 }, "\(got)")
        XCTAssertEqual(shared.count, one, "4 demandes simultanées : un seul téléchargement (\(one) requêtes), pas \(shared.count)")
        _ = try await backend.longDaily(btc)
        XCTAssertEqual(shared.count, one, "redemandé ensuite : servi par le cache")
    }
}

extension DownloadSharingTests {
    /// Counts requests per URL, answers an empty RSS feed.
    final class FeedCounter: MarketFetcher, @unchecked Sendable {
        private let lock = NSLock()
        private var urls: [String] = []
        var all: [String] { lock.lock(); defer { lock.unlock() }; return urls }
        func data(_ url: String, method: String, body: Data?) async throws -> Data {
            lock.lock(); urls.append(url); lock.unlock()
            try await Task.sleep(nanoseconds: 10_000_000)
            return Data("<rss><channel></channel></rss>".utf8)
        }
    }

    /// The news of several assets and of the macro context read each general feed once.
    func testNewsFeedsAreReadOncePerFiveMinutes() async {
        let a = Asset(symbol: "AAPL", kind: .stock, name: "Apple"), b = Asset(symbol: "ETH", kind: .crypto, name: "Ethereum")
        let counter = FeedCounter()
        let backend = LocalBackend(fetcher: counter, bot: false)
        async let x = backend.news([a])
        async let y = backend.news([b])
        async let z = backend.news([])
        _ = await (x, y, z)
        let general = NewsEngine.feeds.map(\.url)
        let urls = counter.all
        for u in general { XCTAssertEqual(urls.filter { $0 == u }.count, 1, u) }
        XCTAssertEqual(urls.count, Set(urls).count, "aucun flux lu deux fois")
        let before = urls.count
        _ = await backend.news([a, b])
        XCTAssertEqual(counter.all.count, before, "même flux dans les 5 minutes : servis par le cache")
    }
}

extension DownloadSharingTests {
    /// The same quote asked by several screens at once is read once from each source.
    func testQuotesAreReadOnceForSimultaneousCallers() async {
        let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), aapl = Asset(symbol: "AAPL", kind: .stock, name: "Apple")
        let single = FeedCounter()
        _ = await LocalBackend(fetcher: single, bot: false).quotes([btc, aapl])
        let one = single.all.count
        XCTAssertGreaterThan(one, 0)
        let shared = FeedCounter()
        let backend = LocalBackend(fetcher: shared, bot: false)
        async let a = backend.quotes([btc, aapl])
        async let b = backend.quotes([btc])
        async let c = backend.quotes([aapl, btc])
        _ = await (a, b, c)
        XCTAssertEqual(shared.all.count, one, "3 demandes simultanées : \(one) requêtes, pas \(shared.all.count)")
    }
}

extension DownloadSharingTests {
    /// The macro series, the Fear & Greed index and the earnings calendar asked by several assets at once are read once.
    func testMarketWideDataIsReadOnce() async {
        let counter = FeedCounter()
        let backend = LocalBackend(fetcher: counter, bot: false)
        async let m1 = try? backend.macroReport()
        async let m2 = try? backend.macroReport()
        async let m3 = try? backend.macroReport()
        _ = await (m1, m2, m3)
        let yahoo = counter.all.filter { $0.hasPrefix("https://query1.finance.yahoo.com/v8/finance/chart/") }
        XCTAssertEqual(yahoo.count, MacroEngine.symbols.count, "chaque série macro lue une fois, pas \(yahoo.count)")
        async let f1 = LocalBackend.fearGreed(await backend.sharedFetcher)
        async let f2 = LocalBackend.fearGreed(await backend.sharedFetcher)
        _ = await (f1, f2)
        XCTAssertEqual(counter.all.filter { $0.contains("alternative.me") }.count, 1)
        let now = Date(timeIntervalSince1970: 1_793_000_000)
        async let e1 = backend.earnings(["AAPL"], now: now)
        async let e2 = backend.earnings(["MSFT"], now: now)
        _ = await (e1, e2)
        let pages = counter.all.filter { $0.hasPrefix("https://api.nasdaq.com/api/calendar/earnings") }
        XCTAssertEqual(pages.count, Set(pages).count, "chaque page du calendrier lue une fois")
    }
}

extension DownloadSharingTests {
    /// Every feed is dead (HTTP 404), counts requests.
    final class DeadFeeds: MarketFetcher, @unchecked Sendable {
        private let lock = NSLock()
        private var urls: [String] = []
        var all: [String] { lock.lock(); defer { lock.unlock() }; return urls }
        func data(_ url: String, method: String, body: Data?) async throws -> Data {
            lock.lock(); urls.append(url); lock.unlock()
            try await Task.sleep(nanoseconds: 10_000_000)
            throw MarketError("HTTP 404")
        }
    }

    /// A dead feed is not asked again by every reader that follows (2 minutes), nor by those waiting for it.
    func testDeadFeedsAreNotRetriedByEveryReader() async {
        let dead = DeadFeeds()
        let backend = LocalBackend(fetcher: dead, bot: false)
        let a = Asset(symbol: "AAPL", kind: .stock, name: "Apple"), b = Asset(symbol: "ETH", kind: .crypto, name: "Ethereum")
        async let x = backend.news([a])
        async let y = backend.news([])
        _ = await (x, y)
        _ = await backend.news([b])
        _ = await backend.news([])
        let general = Set(NewsEngine.feeds.map(\.url))
        for u in general { XCTAssertEqual(dead.all.filter { $0 == u }.count, 1, u) }
    }

    func testFailureIsRetriedAfterItsDelay() async throws {
        let dead = DeadFeeds()
        let cache = FeedCache(ttl: 60_000, failureTTL: 1)
        _ = try? await cache.data("https://exemple.test/a", fetcher: dead)
        try await Task.sleep(nanoseconds: 20_000_000)
        _ = try? await cache.data("https://exemple.test/a", fetcher: dead)
        XCTAssertEqual(dead.all.count, 2, "après le délai d'échec, la source est relue")
    }
}
