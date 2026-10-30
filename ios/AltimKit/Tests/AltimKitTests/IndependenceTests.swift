import Foundation
import XCTest
@testable import AltimKit

/// The iPhone app never takes its data from the Altim web server: every route it uses is computed on the device, from
/// public market and news sources only.
final class IndependenceTests: XCTestCase {
    /// Records every URL asked (and answers nothing, like a dead network).
    final class Recorder: MarketFetcher, @unchecked Sendable {
        private let lock = NSLock()
        private var urls: [String] = []
        var hosts: Set<String> { lock.lock(); defer { lock.unlock() }; return Set(urls.compactMap { URL(string: $0)?.host }) }
        func data(_ url: String, method: String, body: Data?) async throws -> Data {
            lock.lock(); urls.append(url); lock.unlock()
            throw MarketError("hors ligne")
        }
    }

    /// The public sources the iPhone may read: exchanges, stock data, rates, sentiment, news feeds.
    static let allowed: Set<String> = [
        "api.binance.com", "www.okx.com", "api.exchange.coinbase.com", "api.kraken.com", "api.kucoin.com", "api.gateio.ws",
        "www.bitstamp.net", "api-pub.bitfinex.com", "query1.finance.yahoo.com", "api.nasdaq.com", "api.robinhood.com",
        "www.ecb.europa.eu", "api.frankfurter.dev", "api.coingecko.com", "api.alternative.me", "feeds.finance.yahoo.com",
        "news.google.com", "www.lemonde.fr", "www.bfmtv.com", "www.latribune.fr", "feeds.content.dowjones.io", "search.cnbc.com",
        "www.investing.com", "www.coindesk.com", "cointelegraph.com", "decrypt.co", "www.theblock.co", "cryptoast.fr", "journaldutoken.com",
    ]

    func testEveryRouteReadsOnlyPublicSources() async {
        let rec = Recorder()
        let backend = LocalBackend(fetcher: rec)
        let one = ["symbol": "BTC", "kind": "crypto"], stock = ["symbol": "AAPL", "kind": "stock"]
        let calls: [(String, [String: String])] = [
            ("/api/candles", one.merging(["interval": "4h"]) { a, _ in a }), ("/api/radar", ["symbols": "BTC:crypto,AAPL:stock"]),
            ("/api/tickers", ["symbols": "BTC:crypto,AAPL:stock"]), ("/api/search", ["q": "apple"]), ("/api/fx", [:]),
            ("/api/zones", stock), ("/api/backtest", one), ("/api/history", ["days": "90", "symbols": "ETH:crypto"]),
            ("/api/news", ["symbols": "AAPL:stock"]), ("/api/alerts", ["symbols": "BTC:crypto"]), ("/api/guard", one),
            ("/api/rotation", ["kind": "crypto"]), ("/api/verdict", stock), ("/api/sectors", ["symbols": "ZZZZ"]), ("/api/macro", [:]),
            ("/api/earnings", ["symbols": "AAPL"]),
        ]
        XCTAssertEqual(Set(calls.map(\.0)), LocalBackend.routes, "chaque route calculée sur l'iPhone est vérifiée ici")
        for (path, q) in calls {
            let r = await backend.answer(path: path, query: q)
            XCTAssertNotNil(r, path)
        }
        let unknown = rec.hosts.subtracting(Self.allowed)
        XCTAssertTrue(unknown.isEmpty, "hôtes non prévus : \(unknown.sorted())")
        XCTAssertFalse(rec.hosts.contains { $0.contains("herokuapp") || $0.contains("altim") })
    }

    /// A route the iPhone does not compute fails at once, without any request.
    func testStandaloneClientNeverReachesAServer() async {
        let c = AltimClient.standalone(local: LocalBackend(fetcher: Recorder()))
        do {
            _ = try await c.decision(asset: Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"))
            XCTFail("décision sans serveur")
        } catch {
            XCTAssertEqual(error as? AltimError, .needsServer)
        }
    }

    /// The app's sources (iPhone, Watch, widgets) call no route of the Altim server.
    func testTheAppCallsNoServerRoute() throws {
        let ios = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let forbidden = ["decision", "brief", "calendar", "strategies", "why", "ask", "opportunities", "anomalies", "selection", "validation",
                         "bot", "botViews", "login", "logout", "accessMode", "renewSession", "liveRequest", "liveTicks"]
        let pattern = try NSRegularExpression(pattern: "client\\??\\.(\(forbidden.joined(separator: "|")))\\(")
        var found: [String] = []
        var files = 0
        for dir in ["Altim", "AltimWatch", "AltimWidgets", "Shared"] {
            guard let e = FileManager.default.enumerator(at: ios.appendingPathComponent(dir), includingPropertiesForKeys: nil) else { continue }
            for case let url as URL in e where url.pathExtension == "swift" {
                files += 1
                let text = try String(contentsOf: url, encoding: .utf8)
                for m in pattern.matches(in: text, range: NSRange(text.startIndex..., in: text)) {
                    found.append("\(url.lastPathComponent): \((text as NSString).substring(with: m.range))")
                }
                XCTAssertFalse(text.contains("AltimClient(baseURL"), "\(url.lastPathComponent) crée un client de serveur")
            }
        }
        XCTAssertGreaterThan(files, 20, "sources de l'app introuvables depuis \(ios.path)")
        XCTAssertEqual(found, [], "appels au serveur : \(found)")
    }
}
