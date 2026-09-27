import XCTest
@testable import AltimCore

/// Parsers of the additional sources, on real responses (27/09/2026). Same samples as web/test/sources.test.ts.
final class MoreSourcesTests: XCTestCase {
    let now = Date(timeIntervalSince1970: 1_790_600_000)
    func data(_ s: String) -> Data { Data(s.utf8) }
    let bitstampSample = #"""
{"data": {"pair": "BTC/USD", "ohlc": [{"timestamp": "1790503200", "open": "84918.88", "high": "85089.89", "low": "84791.70", "close": "84865.17", "volume": "26.56202532"}, {"timestamp": "1790506800", "open": "84858.10", "high": "84934.09", "low": "84819.11", "close": "84842.79", "volume": "7.22360728"}]}}
"""#
    let geminiSample = #"""
[[1790503200000, 84912.04, 85085.62, 84791.47, 84871.45, 0.85068258], [1790499600000, 84778.14, 84934.12, 84590.71, 84912.04, 0.28386706]]
"""#
    let cryptocomSample = #"""
{"id": -1, "method": "public/get-candlestick", "code": 0, "result": {"interval": "1h", "data": [{"o": "84954.30", "h": "85129.60", "l": "84794.51", "c": "84884.22", "v": "30.51772", "t": 1790503200000}, {"o": "84876.74", "h": "84952.93", "l": "84837.01", "c": "84863.74", "v": "11.38151", "t": 1790506800000}], "instrument_name": "BTC_USDT"}}
"""#
    let bitgetSample = #"""
{"code":"00000","msg":"success","requestTime":1790508916710,"data":[["1790503200000","84955.72","85120","84800","84880.21","92.958017","7897444.53188482","7897444.53188482"],["1790506800000","84880.21","84946.41","84838.01","84860.67","17.608917","1494815.2912817","1494815.2912817"]]}
"""#
    let mexcSample = #"""
[[1790503200000,"84936.99","85080","84789.64","84867.09","276.41754731",1790506800000,"23478691.55"],[1790506800000,"84867.09","84939.97","84833.96","84849.99","88.52018559",1790510400000,"7513489.77"]]
"""#
    let htxSample = #"""
{"ch":"market.btcusdt.kline.60min","status":"ok","ts":1790508917481,"data":[{"id":1790506800,"open":84860.22,"close":84857.99,"low":84832.26,"high":84937.44,"amount":156.82517159741286,"vol":1.330886168618912E7,"count":231},{"id":1790503200,"open":84890.81,"close":84860.22,"low":84777.56,"high":85065.26,"amount":306.5102188604617,"vol":2.6017566588014267E7,"count":763}]}
"""#
    let rhSample = #"""
{"quote": "https://api.robinhood.com/quotes/450dfc6d-5510-4d40-abfb-f633b7d9be3e/", "symbol": "AAPL", "interval": "day", "span": "week", "bounds": "regular", "instrument": "https://api.robinhood.com/instruments/450dfc6d-5510-4d40-abfb-f633b7d9be3e/", "historicals": [{"begins_at": "2026-09-24T00:00:00Z", "open_price": "336.720000", "close_price": "335.920000", "high_price": "338.910000", "low_price": "334.300000", "volume": 24733098, "session": "reg", "interpolated": false}, {"begins_at": "2026-09-25T00:00:00Z", "open_price": "336.040000", "close_price": "341.070000", "high_price": "341.670000", "low_price": "334.530000", "volume": 30002507, "session": "reg", "interpolated": false}], "InstrumentID": "450dfc6d-5510-4d40-abfb-f633b7d9be3e"}
"""#
    let cboeSample = #"""
{"timestamp": "11:00:23", "data": [{"date": "2026-09-23", "volume": 31658823, "open": 340.93, "high": 341.8, "low": 335.5, "close": 337.02}, {"date": "2026-09-24", "volume": 24733098, "open": 336.65, "high": 338.91, "low": 334.3, "close": 335.92}], "symbol": "AAPL"}
"""#
    let rhqSample = #"""
{"results": [{"symbol": "AAPL", "last_trade_price": "341.040000", "previous_close": "335.920000", "adjusted_previous_close": "335.920000"}, {"symbol": "BRK.B", "last_trade_price": "505.430000", "previous_close": "505.180000", "adjusted_previous_close": "505.180000"}, null]}
"""#

    func testCryptoExchanges() throws {
        let t0 = Date(timeIntervalSince1970: 1_790_503_200), t1 = Date(timeIntervalSince1970: 1_790_506_800)

        let bitstamp = try BitstampMarketData.parse(data(bitstampSample), step: 3600, now: now)
        XCTAssertEqual(bitstamp.map(\.time), [t0, t1])
        XCTAssertEqual(bitstamp[0].close, 84865.17)
        XCTAssertEqual(bitstamp[1].high, 84934.09)

        let gemini = try GeminiMarketData.parse(data(geminiSample), step: 3600, now: now)
        XCTAssertEqual(gemini.map(\.time), [Date(timeIntervalSince1970: 1_790_499_600), t0]) // oldest first
        XCTAssertEqual(gemini[1].close, 84871.45)
        XCTAssertEqual(gemini[1].low, 84791.47)

        let cryptocom = try CryptoComMarketData.parse(data(cryptocomSample), step: 3600, now: now)
        XCTAssertEqual(cryptocom.map(\.time), [t0, t1])
        XCTAssertEqual(cryptocom[0].close, 84884.22)

        let bitget = try BitgetMarketData.parse(data(bitgetSample), step: 3600, now: now)
        XCTAssertEqual(bitget.map(\.time), [t0, t1])
        XCTAssertEqual(bitget[0].open, 84955.72)
        XCTAssertEqual(bitget[0].close, 84880.21)

        let mexc = try MEXCMarketData.parse(data(mexcSample), step: 3600, now: now)
        XCTAssertEqual(mexc.map(\.time), [t0, t1])
        XCTAssertEqual(mexc[1].close, 84849.99)

        let htx = try HTXMarketData.parse(data(htxSample), step: 3600, now: now)
        XCTAssertEqual(htx.map(\.time), [t0, t1]) // oldest first
        XCTAssertEqual(htx[0].close, 84860.22)
        XCTAssertEqual(htx[0].high, 85065.26)
        XCTAssertFalse(HTXMarketData().supports(.d1))

        XCTAssertTrue(try BitstampMarketData.parse(data(bitstampSample), step: 3600, now: t1.addingTimeInterval(10)).last!.isClosed == false)
        XCTAssertThrowsError(try BitgetMarketData.parse(data(#"{"code":"40034","msg":"Parameter does not exist","data":null}"#), step: 3600, now: now))
        XCTAssertThrowsError(try HTXMarketData.parse(data(#"{"status":"error","err-msg":"invalid symbol"}"#), step: 3600, now: now))
    }

    func testStockSources() throws {
        let sep24 = newYorkOpen(year: 2026, month: 9, day: 24)!, sep25 = newYorkOpen(year: 2026, month: 9, day: 25)!
        let rh = try RobinhoodMarketData.parse(data(rhSample), now: now)
        XCTAssertEqual(rh.map(\.time), [sep24, sep25])
        XCTAssertEqual(rh[1].close, 341.07)
        XCTAssertEqual(rh[1].volume, 30002507)

        let cboe = try CboeMarketData.parse(data(cboeSample), now: now)
        XCTAssertEqual(cboe.last?.time, sep24)
        XCTAssertEqual(cboe.last?.close, 335.92)
        // Same session, same close at Robinhood and Cboe.
        XCTAssertEqual(rh[0].close, cboe.last!.close)

        let q = try RobinhoodQuotes.parse(data(rhqSample))
        XCTAssertEqual(q.price, 341.04)
        XCTAssertEqual(q.changePercent24h, (341.04 / 335.92 - 1) * 100, accuracy: 1e-9)
        XCTAssertThrowsError(try RobinhoodQuotes.parse(data(#"{"results":[null]}"#)))

        let tv = try TradingViewQuotes.parse(data(#"{"totalCount":1,"data":[{"s":"NYSE:BRK.B","d":["BRK.B","NYSE",505.48,0.0594]}]}"#))
        XCTAssertEqual(tv.price, 505.48)
        XCTAssertThrowsError(try TradingViewQuotes.parse(data(#"{"totalCount":0,"data":[]}"#)))
        XCTAssertEqual(dottedSymbol("BRK-B"), "BRK.B")
        XCTAssertFalse(RobinhoodMarketData().supports(.h1))
    }

    func testStandardConfigurationQueriesEverySource() {
        let c = ConsensusMarketData.standard(transport: URLSessionTransport())
        let btc = Asset.defaults[0], aapl = Asset(symbol: "AAPL", name: "Apple", assetClass: .stock, quote: "USD")
        XCTAssertEqual(c.targetSources, .max)
        XCTAssertGreaterThanOrEqual(c.sources.filter { $0.supports(btc) && $0.providesCandles && $0.supports(.h1) }.count, 13)
        XCTAssertGreaterThanOrEqual(c.sources.filter { $0.supports(aapl) && $0.providesCandles && $0.supports(.d1) }.count, 5)
        XCTAssertGreaterThanOrEqual(c.sources.filter { $0.supports(aapl) && !$0.providesCandles }.count, 3)
        // Distinct names: a SwiftUI list uses them as identifiers.
        XCTAssertEqual(Set(c.sources.map(\.name)).count, c.sources.count)
    }
}
