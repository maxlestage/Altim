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
