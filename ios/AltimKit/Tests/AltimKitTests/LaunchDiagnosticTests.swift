import XCTest
@testable import AltimKit

/// A simulated launch (Radar, alerts, Selection and an asset page at once, bots on) never downloads the same address
/// twice: histories, quotes, news and daily candles are shared between the screens.
final class LaunchDiagnosticTests: XCTestCase {
    final class Rec: MarketFetcher, @unchecked Sendable {
        private let lock = NSLock()
        private var urls: [String] = []
        let body: String
        init(_ bars: [Bar]) { body = "[" + bars.map { "[\(Int($0.time)),\"\($0.open)\",\"\($0.high)\",\"\($0.low)\",\"\($0.close)\",\"\($0.volume)\"]" }.joined(separator: ",") + "]" }
        var all: [String] { lock.lock(); defer { lock.unlock() }; return urls }
        func data(_ url: String, method: String, body: Data?) async throws -> Data {
            lock.lock(); urls.append(url); lock.unlock()
            try await Task.sleep(nanoseconds: 5_000_000)
            if url.hasPrefix("https://api.binance.com/api/v3/klines") || url.hasPrefix("https://api.kraken.com") { return Data(self.body.utf8) }
            throw MarketError("HTTP 404")
        }
    }

    func testLaunchDownloadsEachAddressOnce() async throws {
        let now = Date().timeIntervalSince1970 * 1000
        let rec = Rec(LocalBackendTests.trend(count: 400, step: 86_400_000, end: now - 86_400_000))
        let b = LocalBackend(fetcher: rec, bot: true, botStore: nil)
        let syms = "crypto:BTC,crypto:ETH,crypto:SOL"
        async let r1 = b.answer(path: "/api/radar", query: ["symbols": syms])
        async let r2 = b.answer(path: "/api/alerts", query: ["symbols": syms])
        async let r3 = b.answer(path: "/api/rotation", query: ["kind": "crypto"])
        async let r4 = b.answer(path: "/api/verdict", query: ["symbol": "ETH", "kind": "crypto"])
        _ = await (r1, r2, r3, r4)
        var c: [String: Int] = [:]
        for u in rec.all { c[u, default: 0] += 1 }
        let twice = c.filter { $0.value > 1 }.map { "\($0.value)× \($0.key)" }.sorted()
        XCTAssertGreaterThan(rec.all.count, 100)
        XCTAssertEqual(twice, [], "adresses téléchargées plusieurs fois au lancement")
    }
}
