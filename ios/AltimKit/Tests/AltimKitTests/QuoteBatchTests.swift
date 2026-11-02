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
    static let krakenList = #"{"error":[],"result":{"XXBTZUSD":{"a":["100010.0","1","1.000"],"c":["100000.0","0.01"],"o":"98000.0"},"XETHZUSD":{"a":["3001","1","1"],"c":["3000.5","0.5"],"o":"2950"},"SOLUSD":{"c":["150.25","2"],"o":"149"}}}"#
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

    func testKrakenListWithLegacyNames() async throws {
        let f = Routes([("https://api.kraken.com/0/public/Ticker?pair=XBTUSD,ETHUSD,SOLUSD", Self.krakenList),
                        ("https://api.kraken.com/0/public/Ticker?pair=XBTUSD,ZZZUSD", #"{"error":["EQuery:Unknown asset pair"]}"#)])
        let k = try await XCTUnwrap(QuoteSources.batches[.crypto]?["Kraken"])(f, ["BTC", "ETH", "SOL"])
        XCTAssertEqual(k["BTC"]?.price, 100_000, "XXBTZUSD relu comme BTC")
        XCTAssertEqual(k["ETH"]?.price, 3000.5)
        XCTAssertEqual(k["SOL"]?.price, 150.25)
        XCTAssertNil(k["BTC"]?.change, "l'ouverture de Kraken est celle de minuit : pas de variation 24 h")
        do {
            _ = try await XCTUnwrap(QuoteSources.batches[.crypto]?["Kraken"])(f, ["BTC", "ZZZ"])
            XCTFail("liste refusée attendue")
        } catch {}
    }

    /// Two cryptos: Binance, Kraken and Bitfinex asked once for both; the consensus uses them like single answers.
    func testFewerRequestsSameConsensus() async {
        let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), eth = Asset(symbol: "ETH", kind: .crypto, name: "Ethereum")
        let f = Routes([("https://api.binance.com/api/v3/ticker/24hr?symbols=", Self.binanceList),
                        ("https://api.kraken.com/0/public/Ticker?pair=XBTUSD,ETHUSD", Self.krakenList),
                        ("https://api-pub.bitfinex.com/v2/tickers?symbols=", Self.bitfinexList)])
        let q = await QuoteSources.consensusByID([btc, eth], fetcher: f)
        XCTAssertEqual(q["crypto:BTC"]?.price ?? 0, 100_000, accuracy: 0.01, "médiane de Binance, Kraken et Bitfinex")
        XCTAssertEqual(q["crypto:ETH"]?.agreeing, 3)
        let perAsset = QuoteSources.crypto.count
        XCTAssertEqual(f.all.count, 2 * perAsset - 3, "Binance, Kraken et Bitfinex : une requête pour les deux actifs")
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

extension QuoteBatchTests {
    /// An ETF quoted by Nasdaq under "etf": the next refresh asks that class directly (1 request instead of 2).
    func testNasdaqRemembersTheAssetClass() async throws {
        let f = Routes([("https://api.nasdaq.com/api/quote/QQQX/info?assetclass=etf", #"{"data":{"primaryData":{"lastSalePrice":"$500.10","percentageChange":"+0.50%"}}}"#)])
        let nasdaq = try XCTUnwrap(QuoteSources.stock.first { $0.name == "Nasdaq" })
        let first = try await nasdaq.fetch(f, "QQQX")
        XCTAssertEqual(first.price, 500.10, accuracy: 1e-9)
        XCTAssertEqual(f.all.count, 2, "1re fois : « stocks » puis « etf »")
        _ = try await nasdaq.fetch(f, "QQQX")
        XCTAssertEqual(f.all.count, 3, "ensuite : directement « etf »")
    }
}

extension QuoteBatchTests {
    /// Nasdaq history of an ETF: a failing "stocks" request no longer stops the "etf" one, and the class is reused.
    func testNasdaqHistoryTriesTheOtherClass() async throws {
        let rows = #"{"data":{"tradesTable":{"rows":[{"date":"10/30/2026","open":"$500.00","high":"$505.00","low":"$498.00","close":"$503.00","volume":"1,000"},{"date":"10/29/2026","open":"$497.00","high":"$501.00","low":"$495.00","close":"$500.00","volume":"900"}]}}}"#
        let f = Routes([("https://api.nasdaq.com/api/quote/ETFX/historical?assetclass=stocks", "400"),
                        ("https://api.nasdaq.com/api/quote/ETFX/historical?assetclass=etf", rows)])
        let bars = try await Sources.nasdaq.fetch(f, "ETFX", .d1)
        XCTAssertEqual(bars.count, 2)
        XCTAssertEqual(bars.last?.close, 503)
        XCTAssertEqual(f.all.count, 2)
        _ = try await Sources.nasdaq.fetch(f, "ETFX", .d1)
        XCTAssertEqual(f.all.count, 3, "ensuite : directement « etf »")
    }
}

extension QuoteBatchTests {
    /// A crypto Binance does not list made it refuse the whole list at every refresh (then one request per asset):
    /// once it answered « not listed », it is left out of the list and not asked again.
    func testUnlistedSymbolIsLeftOut() async {
        let btc = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), eth = Asset(symbol: "ETH", kind: .crypto, name: "Ethereum")
        let zzz = Asset(symbol: "ZZZ", kind: .crypto, name: "Inconnue")
        let f = Routes([("https://api.binance.com/api/v3/ticker/24hr?symbols=%5B%22BTCUSDT%22,%22ETHUSDT%22,%22ZZZUSDT%22%5D", "400"),
                        ("https://api.binance.com/api/v3/ticker/24hr?symbols=%5B%22BTCUSDT%22,%22ETHUSDT%22%5D", Self.binanceList),
                        ("https://api.binance.com/api/v3/ticker/24hr?symbol=BTCUSDT", #"{"lastPrice":"100000","priceChangePercent":"1"}"#),
                        ("https://api.binance.com/api/v3/ticker/24hr?symbol=ETHUSDT", #"{"lastPrice":"3000","priceChangePercent":"1"}"#),
                        ("https://api.binance.com/api/v3/ticker/24hr?symbol=ZZZUSDT", "400")])
        let unlisted = Unlisted()
        let first = await QuoteSources.consensusByID([btc, eth, zzz], fetcher: f, unlisted: unlisted)
        XCTAssertEqual(first["crypto:BTC"]?.price, 100_000)
        let after1 = f.all.count
        let second = await QuoteSources.consensusByID([btc, eth, zzz], fetcher: f, unlisted: unlisted)
        XCTAssertEqual(second["crypto:BTC"]?.price, 100_000)
        let secondURLs = Array(f.all.dropFirst(after1))
        XCTAssertFalse(secondURLs.contains { $0.contains("ZZZ") }, "la crypto non cotée n'est plus demandée")
        XCTAssertTrue(secondURLs.contains { $0.hasSuffix("%5B%22BTCUSDT%22,%22ETHUSDT%22%5D") }, "liste Binance sans elle")
        XCTAssertFalse(secondURLs.contains { $0.contains("ticker/24hr?symbol=") }, "plus de requête Binance actif par actif")
        XCTAssertLessThan(secondURLs.count, after1, "\(after1) requêtes la 1re fois, \(secondURLs.count) ensuite")
    }
}
