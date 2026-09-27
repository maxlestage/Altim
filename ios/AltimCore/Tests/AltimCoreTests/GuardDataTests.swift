import XCTest
@testable import AltimCore

/// Parsers of the guard data (same samples as web/test/guard-data.test.ts).
final class GuardDataTests: XCTestCase {
    func data(_ s: String) -> Data { Data(s.utf8) }
    let funding = #"""
{"code": "0", "data": [{"instId": "BTC-USDT-SWAP", "fundingRate": "-0.0000093022018830", "fundingTime": "1790524800000"}]}
"""#
    let ls = #"""
{"code": "0", "data": [["1790517600000", "1.2"], ["1790514000000", "1.27"], ["1790510400000", "1.29"]], "msg": ""}
"""#
    let oi = #"""
{"code": "0", "data": [["1790521200000", "3096187639.1516", "373921486.7371"], ["1790517600000", "3151244635.3541", "379272299.6517"], ["1790514000000", "3131575585.0841", "119727471.7871"]], "msg": ""}
"""#
    let fng = #"""
{"data": [{"value": "70", "value_classification": "Greed", "timestamp": "1790467200"}, {"value": "74", "value_classification": "Greed", "timestamp": "1790380800"}, {"value": "71", "value_classification": "Greed", "timestamp": "1790294400"}]}
"""#
    let vix = #"""
{"data": [{"date": "2026-09-23", "volume": "0.0", "open": "15.10", "high": "15.80", "low": "14.90", "close": "15.42"}, {"date": "2026-09-24", "volume": "0.0", "open": "15.40", "high": "16.20", "low": "15.00", "close": "15.61"}, {"date": "2026-09-25", "volume": "0.0", "open": "15.60", "high": "15.90", "low": "14.70", "close": "14.87"}]}
"""#
    let rss = #"""
<rss><channel><title>Google News</title><item><title>Is It Too Late to Buy Bitcoin After a 32% Rally in Two Months? - 24/7 Wall St.</title><pubDate>Sun, 27 Sep 2026 14:25:00 GMT</pubDate><source url="https://247wallst.com">24/7 Wall St.</source></item><item><title>Bitcoin Holders Are Selling, But This Time It’s Different: What You Need to Know - Yahoo Finance</title><pubDate>Sun, 27 Sep 2026 11:00:58 GMT</pubDate><source url="https://finance.yahoo.com">Yahoo Finance</source></item><item><title><![CDATA[S&amp;P 500 &amp; Bitcoin: “risk-on” returns]]></title><pubDate>Sat, 26 Sep 2026 22:15:03 GMT</pubDate></item><item><title>No date here</title></item></channel></rss>
"""#

    func testPositioning() {
        XCTAssertEqual(GuardDataProvider.funding(data(funding))!, -0.000009302201883, accuracy: 1e-15)
        XCTAssertNil(GuardDataProvider.funding(data(#"{"code":"51001","data":[]}"#)))
        XCTAssertEqual(GuardDataProvider.rubik(data(ls)), [1.29, 1.27, 1.2])
        XCTAssertEqual(GuardDataProvider.rubik(data(oi)), [3131575585.0841, 3151244635.3541, 3096187639.1516])
        XCTAssertEqual(GuardDataProvider.rubik(data(#"{"code":"50011","data":null}"#)), [])
    }

    func testSentimentVixAndNews() {
        XCTAssertEqual(GuardDataProvider.fearGreed(data(fng)), [71, 74, 70])
        XCTAssertEqual(GuardDataProvider.vix(data(vix)), [15.42, 15.61, 14.87])
        let items = GuardDataProvider.rss(rss)
        XCTAssertEqual(items.count, 3)
        XCTAssertTrue(items[0].title.contains("Is It Too Late to Buy Bitcoin"))
        XCTAssertEqual(items[0].time, Date(timeIntervalSince1970: 1_790_519_100))
        XCTAssertEqual(items[0].source, "24/7 Wall St.")
        XCTAssertEqual(items[2].title, "S&P 500 & Bitcoin: “risk-on” returns")
    }

    /// Real network (ALTIM_LIVE=1): full guard on BTC and AAPL with every source.
    func testLiveGuard() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1")
        let transport = ResilientTransport()
        let market = ConsensusMarketData.standard(transport: transport)
        for asset in [Asset.defaults[0], Asset(symbol: "AAPL", name: "Apple", assetClass: .stock, quote: "USD")] {
            async let d = market.snapshot(for: asset, timeframe: .d1, limit: 500)
            async let h4 = market.snapshot(for: asset, timeframe: .h4, limit: 500)
            async let h1 = market.snapshot(for: asset, timeframe: .h1, limit: 500)
            let extra = await GuardDataProvider(transport: transport).inputs(for: asset)
            let g = MarketGuard.evaluate(.init(kind: asset.assetClass, daily: try await d.candles, h4: try await h4.candles, h1: try await h1.candles,
                                               positioning: extra.positioning, sentiment: extra.sentiment, news: extra.news, vix: extra.vix))
            print(asset.symbol, g.regime.trend, g.regime.strength, "| choc", g.shock.score, g.shock.level,
                  g.shock.factors.map { "\($0.code):\($0.points):\($0.status)" }, "| retournement", g.reversal.score,
                  g.reversal.direction.map(\.rawValue) ?? "-", g.reversal.factors.map { "\($0.code):\($0.points):\($0.status)" },
                  "| bots", g.policy.scalping, "| financement", extra.positioning?.fundingRate ?? .nan, "F&G", extra.sentiment.fearGreed.last ?? .nan,
                  "social", extra.sentiment.socialBullish ?? .nan, "news", extra.news.count, "vix", extra.vix.last ?? .nan)
            XCTAssertFalse(extra.news.isEmpty, asset.symbol)
        }
    }
}
