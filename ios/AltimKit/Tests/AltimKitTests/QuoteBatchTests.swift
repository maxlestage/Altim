import XCTest
@testable import AltimKit

/// Binance, Bitfinex and Robinhood quote a list of assets in one request; the consensus is unchanged.
final class QuoteBatchTests: XCTestCase {
    /// Answers by URL prefix (first match), counts requests.
    final class Routes: MarketFetcher, @unchecked Sendable {
        private let lock = NSLock()
        private var urls: [String] = []
        let routes: [(String, String)]
        init(_ routes: [(String, String)]) { self.routes = routes }
        var all: [String] { lock.lock(); defer { lock.unlock() }; return urls }
        func data(_ url: String, method: String, body: Data?) async throws -> Data {
            lock.lock(); urls.append(url); lock.unlock()
            guard let r = routes.first(where: { url.hasPrefix($0.0) }) else { throw MarketError("HTTP 404") }
            if r.1 == "400" { throw MarketError("HTTP 400") }
            return Data(r.1.utf8)
        }
    }

    // Formats of the real list answers (abridged).
    static let binanceList = #"[{"symbol":"BTCUSDT","priceChangePercent":"1.50","lastPrice":"100000.00"},{"symbol":"ETHUSDT","priceChangePercent":"-2.00","lastPrice":"3000.00"}]"#
    static let bitfinexList = #"[["tBTCUSD",99990,1.2,100010,0.8,1500,0.0151,100005,12.3,101000,98000],["tETHUSD",2999,5,3001,4,-60,-0.0197,3001,100,3100,2900]]"#
    static let robinhoodList = #"{"results":[{"symbol":"AAPL","last_trade_price":"200.00","adjusted_previous_close":"196.00"},{"symbol":"BRK.B","last_trade_price":"500.00","adjusted_previous_close":"505.00"},null]}"#

    func testListAnswersAreRead() async throws {
        let f = Routes([("https://api.binance.com/api/v3/ticker/24hr?symbols=", Self.binanceList),
                        ("https://api-pub.bitfinex.com/v2/tickers?symbols=", Self.bitfinexList),
                        ("https://api.robinhood.com/quotes/?symbols=", Self.robinhoodList)])
        let b = try await XCTUnwrap(QuoteSources.batches[.crypto]?["Binance"])(f, ["BTC", "ETH"])
        XCTAssertEqual(b["BTC"]?.price, 100_000)
        XCTAssertEqual(b["ETH"]?.change ?? 0, -2, accuracy: 1e-9)
        let x = try await XCTUnwrap(QuoteSources.batches[.crypto]?["Bitfinex"])(f, ["BTC", "ETH"])
        XCTAssertEqual(x["BTC"]?.price, 100_005)
        XCTAssertEqual(x["ETH"]?.change ?? 0, -1.97, accuracy: 1e-9)
        let r = try await XCTUnwrap(QuoteSources.batches[.stock]?["Robinhood"])(f, ["AAPL", "BRK-B", "ZZZZ"])
        XCTAssertEqual(r["AAPL"]?.change ?? 0, (200.0 / 196 - 1) * 100, accuracy: 1e-9)
        XCTAssertEqual(r["BRK-B"]?.price, 500, "symbole pointé BRK.B rendu sous le nom de l'app")
        XCTAssertNil(r["ZZZZ"])
        XCTAssertTrue(f.all.contains { $0.contains("symbols=%5B%22BTCUSDT%22,%22ETHUSDT%22%5D") }, f.all.joined(separator: "\n"))
    }

    /// Two cryptos: Binance and Bitfinex asked once for both; the consensus uses them like single answers.
    func testFewerRequestsSameConsensus() async {
        let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), eth = Asset(symbol: "ETH", kind: .crypto, name: "Ethereum")
        let f = Routes([("https://api.binance.com/api/v3/ticker/24hr?symbols=", Self.binanceList),
                        ("https://api-pub.bitfinex.com/v2/tickers?symbols=", Self.bitfinexList)])
        let q = await QuoteSources.consensusByID([btc, eth], fetcher: f)
        XCTAssertEqual(q["crypto:BTC"]?.price ?? 0, 100_002.5, accuracy: 0.01, "médiane de Binance et Bitfinex")
        XCTAssertEqual(q["crypto:ETH"]?.agreeing, 2)
        let perAsset = QuoteSources.crypto.count
        XCTAssertEqual(f.all.count, 2 * perAsset - 2, "Binance et Bitfinex : une requête pour les deux actifs")
        XCTAssertFalse(f.all.contains { $0.contains("ticker/24hr?symbol=") }, "plus de requête Binance par actif")
    }

    /// Binance refuses the whole list (one unknown symbol): each asset is read on its own, nothing is lost.
    func testRefusedListFallsBackToOneByOne() async {
        let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), eth = Asset(symbol: "ETH", kind: .crypto, name: "Ethereum")
        let f = Routes([("https://api.binance.com/api/v3/ticker/24hr?symbols=", "400"),
                        ("https://api.binance.com/api/v3/ticker/24hr?symbol=BTCUSDT", #"{"lastPrice":"100000","priceChangePercent":"1"}"#),
                        ("https://api.binance.com/api/v3/ticker/24hr?symbol=ETHUSDT", #"{"lastPrice":"3000","priceChangePercent":"1"}"#)])
        let q = await QuoteSources.consensusByID([btc, eth], fetcher: f)
        XCTAssertEqual(q["crypto:BTC"]?.price, 100_000)
        XCTAssertEqual(q["crypto:ETH"]?.price, 3000)
    }
}
