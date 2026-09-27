import XCTest
@testable import AltimCore

/// Same fixtures as web/test/universe.test.ts (extracts of real responses).
final class AssetUniverseTests: XCTestCase {
    let nasdaqListed = """
    Symbol|Security Name|Market Category|Test Issue|Financial Status|Round Lot Size|ETF|NextShares
    AAPL|Apple Inc. - Common Stock|Q|N|N|100|N|N
    QQQ|Invesco QQQ Trust, Series 1|G|N|N|100|Y|N
    GOOG|Alphabet Inc. - Class C Capital Stock|Q|N|N|100|N|N
    ZXZZT|NASDAQ TEST STOCK|G|Y|N|100|N|N
    ABCDW|Some Acquisition Corp - Warrants|S|N|N|100|N|N
    File Creation Time: 0925202621:31|||||||
    """

    let otherListed = """
    ACT Symbol|Security Name|Exchange|CQS Symbol|ETF|Round Lot Size|Test Issue|NASDAQ Symbol
    BRK.B|Berkshire Hathaway Inc. New Common Stock|N|BRK.B|N|100|N|BRK.B
    BFH$A|Bread Financial Holdings, Inc. Depositary Shares 8.625% Preferred Stock|N|BFHpA|N|100|N|BFH-A
    SPY|State Street SPDR S&P 500 ETF Trust|P|SPY|Y|100|N|SPY
    TNL|Travel   Leisure Co. Common  Stock|N|TNL|N|100|N|TNL
    File Creation Time: 0925202621:31||||||
    """

    func json(_ o: Any) -> Data { try! JSONSerialization.data(withJSONObject: o) }

    func testStockDirectoryFilters() {
        let nasdaq = AssetUniverse.nasdaqDirectory(nasdaqListed)
        let other = AssetUniverse.nasdaqDirectory(otherListed)
        XCTAssertEqual(nasdaq.map(\.symbol), ["AAPL", "QQQ", "GOOG"])
        XCTAssertEqual(other.map(\.symbol), ["BRK-B", "SPY", "TNL"])
        XCTAssertTrue(nasdaq.first { $0.symbol == "QQQ" }!.etf)
        XCTAssertEqual(nasdaq.first { $0.symbol == "GOOG" }!.name, "Alphabet Inc. Class C")
        XCTAssertEqual(other.first { $0.symbol == "TNL" }!.name, "Travel Leisure Co.")
        XCTAssertEqual(AssetUniverse.cleanStockName("Apple Inc. - Common Stock"), "Apple Inc.")
        XCTAssertEqual(AssetUniverse.cleanStockName("Alphabet Inc. - Class A Common Stock"), "Alphabet Inc. Class A")
        XCTAssertEqual(AssetUniverse.cleanStockName("Invesco QQQ Trust, Series 1"), "Invesco QQQ Trust, Series 1")
    }

    func testStocksRankedByMarketCapThenPopularETFs() {
        let caps = AssetUniverse.screener(json(["data": ["rows": [
            ["symbol": "AAPL", "marketCap": "3,500,000,000,000"],
            ["symbol": "BRK/B", "marketCap": "1000000000000.00"],
            ["symbol": "GOOG", "marketCap": "2000000000000"],
        ]]]))
        let list = AssetUniverse.buildStocks([AssetUniverse.nasdaqDirectory(nasdaqListed), AssetUniverse.nasdaqDirectory(otherListed)], caps: caps)
        XCTAssertEqual(list.prefix(3).map(\.symbol), ["AAPL", "GOOG", "BRK-B"])
        XCTAssertEqual(list.prefix(3).map(\.rank), [1, 2, 3])
        XCTAssertEqual(list.dropFirst(3).map(\.symbol), ["SPY", "QQQ", "TNL"])
        XCTAssertTrue(list.first { $0.symbol == "SPY" }!.isETF)
    }

    func testCryptoUnionWithoutStablecoinsNorLeveragedTokens() {
        let okx = AssetUniverse.okx(json(["code": "0", "data": [
            ["instId": "BTC-USDT", "baseCcy": "BTC", "quoteCcy": "USDT", "state": "live"],
            ["instId": "ETH-USDC", "baseCcy": "ETH", "quoteCcy": "USDC", "state": "live"],
            ["instId": "PEPE-USDT", "baseCcy": "PEPE", "quoteCcy": "USDT", "state": "live"],
            ["instId": "OLD-USDT", "baseCcy": "OLD", "quoteCcy": "USDT", "state": "suspend"],
        ]]))
        let coinbase = AssetUniverse.coinbase(json([
            ["base_currency": "BTC", "quote_currency": "USD", "status": "online", "trading_disabled": false],
            ["base_currency": "ETH", "quote_currency": "USD", "status": "online", "trading_disabled": false],
            ["base_currency": "USDT", "quote_currency": "USD", "status": "online", "trading_disabled": false],
            ["base_currency": "DEAD", "quote_currency": "USD", "status": "delisted", "trading_disabled": true],
        ]))
        let kraken = AssetUniverse.kraken(json(["error": [], "result": [
            "XXBTZUSD": ["wsname": "XBT/USD"], "XDGUSD": ["wsname": "XDG/USD"], "ETHEUR": ["wsname": "ETH/EUR"],
        ]]))
        let gate = AssetUniverse.gate(json([
            ["base": "BTC3L", "quote": "USDT", "trade_status": "tradable"],
            ["base": "PEPE", "quote": "USDT", "trade_status": "tradable"],
            ["base": "NEWCOIN", "quote": "USDT", "trade_status": "tradable"],
        ]))
        XCTAssertEqual(okx, ["BTC", "PEPE"])
        XCTAssertEqual(coinbase, ["BTC", "ETH", "USDT"])
        XCTAssertEqual(Set(kraken), ["BTC", "DOGE"])
        let gecko = AssetUniverse.gecko(json([
            ["symbol": "btc", "name": "Bitcoin", "market_cap_rank": 1],
            ["symbol": "eth", "name": "Ethereum", "market_cap_rank": 2],
            ["symbol": "doge", "name": "Dogecoin", "market_cap_rank": 9],
            ["symbol": "pepe", "name": "Pepe", "market_cap_rank": 30],
            ["symbol": "pepe", "name": "Pepe copycat", "market_cap_rank": 900],
        ]))
        let list = AssetUniverse.buildCrypto([okx, coinbase, kraken, gate, ["NVDAX", "SPYON"]], gecko: gecko,
                                             names: [("NEWCOIN", "New Coin"), ("NVDAX", "NVIDIA xStock"),
                                                     ("SPYON", "SPDR S&P 500 ETF (Ondo Tokenized ETF)")])
        XCTAssertEqual(list.map(\.symbol), ["BTC", "ETH", "DOGE", "PEPE", "NEWCOIN"])
        XCTAssertEqual(list[0], UniverseEntry(symbol: "BTC", name: "Bitcoin", kind: .crypto, rank: 1, flag: 3))
        XCTAssertEqual(list.first { $0.symbol == "PEPE" }!.name, "Pepe")
        XCTAssertEqual(list.last, UniverseEntry(symbol: "NEWCOIN", name: "New Coin", kind: .crypto, rank: nil, flag: 1))
        XCTAssertEqual(list[0].asset.symbol, "BTCUSDT")
    }

    func testSearch() {
        let list = AssetUniverse.buildStocks([AssetUniverse.nasdaqDirectory(nasdaqListed), AssetUniverse.nasdaqDirectory(otherListed)],
                                             caps: ["AAPL": 3e12, "SPY": 6e11])
        XCTAssertEqual(AssetUniverse.search(list, "spy").map(\.symbol), ["SPY"])
        XCTAssertEqual(AssetUniverse.search(list, "apple").first?.symbol, "AAPL")
        XCTAssertEqual(AssetUniverse.search(list, "berkshire").first?.symbol, "BRK-B")
        XCTAssertEqual(AssetUniverse.search(list, "BRK").first?.symbol, "BRK-B")
        XCTAssertEqual(AssetUniverse.search(list, "tràvel").first?.symbol, "TNL")
        XCTAssertEqual(AssetUniverse.search(list, "", limit: 100).count, list.count)
        XCTAssertEqual(AssetUniverse.search(list, "S&P (500"), [])
        XCTAssertEqual(AssetUniverse.search(list, "S&P 500").first?.symbol, "SPY")
    }

    func testSearchAllMixesCryptosAndStocks() {
        let crypto = AssetUniverse.buildCrypto([["BTC", "SOL", "SPYX"]],
                                               gecko: AssetUniverse.gecko(json([["symbol": "btc", "name": "Bitcoin", "market_cap_rank": 1],
                                                                                ["symbol": "sol", "name": "Solana", "market_cap_rank": 7]])),
                                               names: [("SPYX", "Spy Token")])
        let stocks = AssetUniverse.buildStocks([AssetUniverse.nasdaqDirectory(nasdaqListed), AssetUniverse.nasdaqDirectory(otherListed)],
                                               caps: ["AAPL": 3e12])
        let ids = { (q: String) in AssetUniverse.searchAll(crypto: crypto, stocks: stocks, q, limit: 5).map(\.id) }
        XCTAssertEqual(ids("spy"), ["stock:SPY", "crypto:SPYX"])
        XCTAssertEqual(ids("apple"), ["stock:AAPL"])
        XCTAssertEqual(ids("sol").first, "crypto:SOL")
        XCTAssertEqual(ids("bitcoin"), ["crypto:BTC"])
        XCTAssertEqual(ids("  "), [])
    }

    /// Real download of both lists (ALTIM_LIVE=1).
    func testLiveUniverse() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1")
        let crypto = try await AssetUniverse.load(.crypto)
        let stocks = try await AssetUniverse.load(.stock)
        print("crypto", crypto.count, crypto.prefix(5).map(\.symbol), "stocks", stocks.count, stocks.prefix(5).map(\.symbol))
        XCTAssertGreaterThan(crypto.count, 1000)
        XCTAssertGreaterThan(stocks.count, 10000)
    }
}
