import XCTest
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
@testable import AltimCore

final class TradingTests: XCTestCase {
    /// Vecteur officiel de la documentation Binance.
    func testBinanceSignatureVector() {
        let secret = "NhqPtmdSJYdKjVHjA7PZj4Mge3R5YNiP1e3UZjInClVN65XAbvqqM6A7H5fATj0j"
        let query = "symbol=LTCBTC&side=BUY&type=LIMIT&timeInForce=GTC&quantity=1&price=0.1&recvWindow=5000&timestamp=1499827319559"
        XCTAssertEqual(Signer.hmacSHA256Hex(key: secret, message: query),
                       "c8db56825ae71d6d79447849e617115f4a920fa2acdcab2b053c4b2838bd6b71")
    }

    func testDecimalHelpers() {
        XCTAssertEqual(Decimal(string: "0.123456789")!.floored(toStep: Decimal(string: "0.001")!), Decimal(string: "0.123")!)
        XCTAssertEqual(Decimal(string: "19.999")!.floored(toStep: 1), 19)
        XCTAssertEqual(Decimal(string: "0.00000001")!.plainString, "0.00000001")
        XCTAssertEqual(Decimal(string: "65432.10")!.plainString, "65432.1")
        XCTAssertEqual(Decimal(string: "1.2345")!.rounded(toStep: Decimal(string: "0.01")!), Decimal(string: "1.23")!)
    }

    static let exchangeInfo = #"""
    {"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","filters":[
      {"filterType":"PRICE_FILTER","minPrice":"0.01","maxPrice":"1000000.00","tickSize":"0.01"},
      {"filterType":"LOT_SIZE","minQty":"0.00001000","maxQty":"9000.00000000","stepSize":"0.00001000"},
      {"filterType":"NOTIONAL","minNotional":"5.00000000","applyMinToMarket":true}]}]}
    """#

    func testParseRulesAndValidate() throws {
        let rules = try BinanceBroker.parseRules(Data(Self.exchangeInfo.utf8))
        XCTAssertEqual(rules.baseAsset, "BTC")
        XCTAssertEqual(rules.stepSize, Decimal(string: "0.00001")!)
        XCTAssertEqual(rules.minNotional, 5)
        XCTAssertEqual(rules.normalizeQuantity(Decimal(string: "0.0123456")!), Decimal(string: "0.01234")!)

        let ok = OrderRequest(symbol: "BTCUSDT", side: .buy, type: .market, quantity: Decimal(string: "0.001")!,
                              stopLoss: 60_000, takeProfit: 70_000)
        XCTAssertTrue(rules.issues(for: ok, referencePrice: 65_000).isEmpty)

        let tooSmall = OrderRequest(symbol: "BTCUSDT", side: .buy, type: .market, quantity: Decimal(string: "0.00001")!)
        XCTAssertFalse(rules.issues(for: tooSmall, referencePrice: 65_000).isEmpty) // 0,65 USDT < 5

        let offStep = OrderRequest(symbol: "BTCUSDT", side: .buy, type: .market, quantity: Decimal(string: "0.000015")!)
        XCTAssertTrue(rules.issues(for: offStep, referencePrice: 65_000).contains { $0.contains("multiple") })

        let badStop = OrderRequest(symbol: "BTCUSDT", side: .buy, type: .market, quantity: Decimal(string: "0.001")!,
                                   stopLoss: 66_000, takeProfit: 70_000)
        XCTAssertFalse(rules.issues(for: badStop, referencePrice: 65_000).isEmpty)
    }

    func testParseKlines() throws {
        let now = Date(timeIntervalSince1970: 1_700_007_000)
        let json = #"""
        [[1700000000000,"100.0","110.0","95.0","105.0","12.5",1700003599999,"0",1,"0","0","0"],
         [1700003600000,"105.0","106.0","104.0","105.5","3.0",1700007199999,"0",1,"0","0","0"]]
        """#
        let candles = try BinanceMarketData.parseKlines(Data(json.utf8), now: now)
        XCTAssertEqual(candles.count, 2)
        XCTAssertEqual(candles[0].close, 105)
        XCTAssertTrue(candles[0].isClosed)
        XCTAssertFalse(candles[1].isClosed)
    }

    func testParseYahooChartSkipsNulls() throws {
        let json = #"""
        {"chart":{"result":[{"meta":{"regularMarketPrice":190.5,"chartPreviousClose":188.0},
         "timestamp":[1700000000,1700086400,1700172800],
         "indicators":{"quote":[{"open":[1,null,3],"high":[2,null,4],"low":[0.5,null,2.5],"close":[1.5,null,3.5],"volume":[100,null,300]}]}}],"error":null}}
        """#
        let candles = try YahooMarketData.parseChart(Data(json.utf8), interval: .d1, isStock: true,
                                                     now: Date(timeIntervalSince1970: 1_800_000_000))
        XCTAssertEqual(candles.map(\.close), [1.5, 3.5])
        XCTAssertEqual(try YahooMarketData.meta(Data(json.utf8))["regularMarketPrice"] as? Double, 190.5)
    }

    func testYahooAggregation() {
        let base = Date(timeIntervalSince1970: 1_700_006_400) // 00:00 UTC
        let hourly: [Candle] = (0..<8).map { (i: Int) -> Candle in
            let x = Double(i)
            return Candle(time: base.addingTimeInterval(x * 3600), open: x, high: x + 1, low: x - 0.5, close: x + 0.5, volume: 1)
        }
        let h4 = YahooMarketData.aggregate(hourly, by: 4)
        XCTAssertEqual(h4.count, 2)
        XCTAssertEqual(h4[0].open, 0)
        XCTAssertEqual(h4[0].close, 3.5)
        XCTAssertEqual(h4[0].high, 4)
        XCTAssertEqual(h4[0].volume, 4)
    }

    func testBinanceBuyPlacesSignedOrderThenOCOForNetQuantity() async throws {
        let mock = MockTransport()
        mock.routes = [
            "GET /api/v3/time": (200, #"{"serverTime":1700000000000}"#),
            "GET /api/v3/exchangeInfo": (200, Self.exchangeInfo),
            "GET /api/v3/ticker/price": (200, #"{"symbol":"BTCUSDT","price":"65000.00"}"#),
            "POST /api/v3/order": (200, #"""
                {"symbol":"BTCUSDT","orderId":28,"status":"FILLED","executedQty":"0.00100000","cummulativeQuoteQty":"65.00000000",
                 "fills":[{"price":"65000.00","qty":"0.00100000","commission":"0.00000100","commissionAsset":"BTC"}]}
                """#),
            "POST /api/v3/orderList/oco": (200, #"{"orderListId":7}"#),
        ]
        let broker = BinanceBroker(apiKey: "key", secret: "secret", environment: .test, transport: mock)
        let order = OrderRequest(symbol: "BTCUSDT", side: .buy, type: .market, quantity: Decimal(string: "0.001")!,
                                 stopLoss: 62_000, takeProfit: 71_000)
        let result = try await broker.place(order)

        XCTAssertEqual(result.orderId, "28")
        XCTAssertEqual(result.averagePrice, 65_000)
        XCTAssertEqual(result.protectionOrderId, "7")

        let orderRequest = try XCTUnwrap(mock.requests.first { $0.url!.path == "/api/v3/order" })
        XCTAssertEqual(orderRequest.url!.host, "testnet.binance.vision")
        XCTAssertEqual(orderRequest.value(forHTTPHeaderField: "X-MBX-APIKEY"), "key")
        let query = try XCTUnwrap(orderRequest.url!.query)
        XCTAssertTrue(query.contains("side=BUY&type=MARKET&quantity=0.001"))
        // La signature couvre exactement la requête envoyée.
        let parts = query.components(separatedBy: "&signature=")
        XCTAssertEqual(parts.count, 2)
        XCTAssertEqual(Signer.hmacSHA256Hex(key: "secret", message: parts[0]), parts[1])

        let oco = try XCTUnwrap(mock.requests.first { $0.url!.path == "/api/v3/orderList/oco" })
        let ocoQuery = try XCTUnwrap(oco.url!.query)
        XCTAssertTrue(ocoQuery.contains("quantity=0.00099"), ocoQuery) // 0,001 − 0,000001 de frais, arrondi au pas
        XCTAssertTrue(ocoQuery.contains("abovePrice=71000"))
        XCTAssertTrue(ocoQuery.contains("belowStopPrice=62000"))
        XCTAssertTrue(ocoQuery.contains("belowPrice=61690"))
    }

    func testBinanceRejectsInvalidOrderBeforeSending() async throws {
        let mock = MockTransport()
        mock.routes = [
            "GET /api/v3/exchangeInfo": (200, Self.exchangeInfo),
            "GET /api/v3/ticker/price": (200, #"{"price":"65000.00"}"#),
        ]
        let broker = BinanceBroker(apiKey: "key", secret: "secret", environment: .live, transport: mock)
        let order = OrderRequest(symbol: "BTCUSDT", side: .buy, type: .market, quantity: Decimal(string: "0.00001")!)
        do {
            _ = try await broker.place(order)
            XCTFail("L'ordre aurait dû être refusé")
        } catch let BrokerError.validation(issues) {
            XCTAssertFalse(issues.isEmpty)
        }
        XCTAssertFalse(mock.requests.contains { $0.url!.path == "/api/v3/order" })
    }

    func testBinanceErrorIsExplained() async throws {
        let mock = MockTransport()
        mock.routes = [
            "GET /api/v3/time": (200, #"{"serverTime":1700000000000}"#),
            "GET /api/v3/account": (401, #"{"code":-2015,"msg":"Invalid API-key, IP, or permissions for action."}"#),
        ]
        let broker = BinanceBroker(apiKey: "k", secret: "s", environment: .live, transport: mock)
        do {
            _ = try await broker.balances()
            XCTFail("Erreur attendue")
        } catch {
            XCTAssertTrue(error.localizedDescription.contains("Clé API invalide"))
        }
    }

    func testMissingCredentials() async {
        let broker = BinanceBroker(apiKey: " ", secret: "", environment: .live, transport: MockTransport())
        do {
            try await broker.test(OrderRequest(symbol: "BTCUSDT", side: .buy, type: .market, quantity: 1))
            XCTFail("Erreur attendue")
        } catch {
            XCTAssertEqual(error as? APIError, .missingCredentials)
        }
    }

    func testPaperBroker() async throws {
        let broker = PaperBroker(startingCash: 1_000) { _ in 100 }
        let buy = OrderRequest(symbol: "SOLUSDT", side: .buy, type: .market, quantity: 5)
        let r = try await broker.place(buy)
        XCTAssertTrue(r.isSimulated)
        var balances = try await broker.balances()
        XCTAssertEqual(balances.first { $0.asset == "SOL" }?.free, 5)
        XCTAssertEqual(balances.first { $0.asset == "USDT" }?.free, Decimal(string: "499.5")!)
        _ = try await broker.place(OrderRequest(symbol: "SOLUSDT", side: .sell, type: .market, quantity: 5))
        balances = try await broker.balances()
        XCTAssertEqual(balances.first { $0.asset == "USDT" }?.free, Decimal(string: "999")!)
        do {
            _ = try await broker.place(OrderRequest(symbol: "SOLUSDT", side: .buy, type: .market, quantity: 50))
            XCTFail("Solde insuffisant attendu")
        } catch {}
    }

    func testAlpacaBracketOrderBody() async throws {
        let mock = MockTransport()
        mock.routes = [
            "GET /v2/assets/AAPL": (200, #"{"symbol":"AAPL","tradable":true,"status":"active","price_increment":"0.01"}"#),
            "POST /v2/orders": (200, #"{"id":"abc","status":"accepted","filled_qty":"0"}"#),
        ]
        let broker = AlpacaBroker(keyId: "id", secret: "sec", environment: .test, transport: mock)
        let result = try await broker.place(OrderRequest(symbol: "AAPL", side: .buy, type: .market, quantity: 3,
                                                         stopLoss: 180.004, takeProfit: 210))
        XCTAssertEqual(result.status, "ACCEPTED")
        let req = try XCTUnwrap(mock.requests.first { $0.httpMethod == "POST" })
        XCTAssertEqual(req.url!.host, "paper-api.alpaca.markets")
        let body = try JSONSerialization.jsonObject(with: req.httpBody!) as! [String: Any]
        XCTAssertEqual(body["order_class"] as? String, "bracket")
        XCTAssertEqual((body["stop_loss"] as? [String: Any])?["stop_price"] as? String, "180")
        XCTAssertEqual(body["qty"] as? String, "3")
    }
}
