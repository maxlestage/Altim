import XCTest
@testable import AltimKit

/// The market filter: an asset is bought or kept only while its market (Bitcoin, SPY) is in an up trend too.
final class MarketFilterTests: XCTestCase {
    let eth = Asset(symbol: "ETH", kind: .crypto, name: "Ethereum")
    var up: [Bar] { BotEngineTests.series(drift: 0.002) }
    var down: [Bar] { BotEngineTests.series(drift: -0.002, seed: 5) }

    func testMarketState() {
        XCTAssertEqual(TrendEngine.marketUp(up, kind: .crypto), true)
        XCTAssertEqual(TrendEngine.marketUp(down, kind: .crypto), false)
        XCTAssertNil(TrendEngine.marketUp(Array(up.prefix(150)), kind: .crypto), "historique trop court : pas d'avis")
        XCTAssertEqual(TrendEngine.benchmark(.crypto).symbol, "BTC")
        XCTAssertEqual(TrendEngine.benchmark(.stock).symbol, "SPY")
    }

    func testFilterTurnsAnUpTrendIntoAnExit() {
        let v = TrendEngine.verdict(up, kind: .crypto)
        XCTAssertTrue(v.action == .buy || v.action == .strongBuy)
        let kept = TrendEngine.applyMarket(true, kind: .crypto, to: v)
        XCTAssertEqual(kept.action, v.action)
        XCTAssertEqual(kept.market, true)
        XCTAssertEqual(TrendEngine.applyMarket(nil, kind: .crypto, to: v).action, v.action, "marché illisible : la règle seule")
        var out = TrendEngine.applyMarket(false, kind: .crypto, to: v)
        XCTAssertEqual(out.action, .sell)
        XCTAssertEqual(out.effectiveState, .down)
        XCTAssertEqual(out.label, "SORTIR (MARCHÉ)")
        XCTAssertTrue(out.text.hasPrefix("Le Bitcoin (le marché crypto) n'est pas en tendance haussière"), out.text)
        XCTAssertEqual(BotEngine.apply(0.1, to: out).action, .sell, "le bot ne rachète pas ce que le marché fait sortir")
        out.price = 100
        let log = VerdictLog.record([], out, asset: eth, now: 1)
        XCTAssertEqual(log.last?.side, "out")
        XCTAssertEqual(log.last?.label, VerdictLog.marketExitLabel)
        XCTAssertEqual(VerdictLog.summary(VerdictLog.outcomes(log, prices: ["crypto:ETH": 90])).botExitCount, 0, "pas compté comme sortie du bot")
    }

    func testFilterNeverCreatesAPurchase() {
        let d = TrendEngine.verdict(down, kind: .crypto)
        XCTAssertEqual(TrendEngine.applyMarket(true, kind: .crypto, to: d).action, d.action)
        XCTAssertEqual(TrendEngine.applyMarket(false, kind: .crypto, to: d).action, d.action)
        XCTAssertEqual(TrendEngine.applyMarket(false, kind: .crypto, to: d).text, d.text)
    }

    func testBlockerAndSelectionSayTheMarket() {
        let v = TrendEngine.applyMarket(false, kind: .stock, to: TrendEngine.verdict(up, kind: .stock))
        let a = Asset(symbol: "AAPL", kind: .stock, name: "Apple")
        let o = LocalAlertRule.evaluate(.init(asset: a, price: 100, verdict: v, reliability: nil, zones: []))
        XCTAssertFalse(o.buy)
        XCTAssertEqual(o.blockers.first, "Tendance haussière, mais le S&P 500 (le marché actions) n'est pas en tendance haussière : pas d'achat (filtre de marché).")
        let pick = Rotation.picks([(a, up)], kind: .stock, verdicts: [a.id: v]).first
        XCTAssertEqual(pick?.selected, false)
        XCTAssertTrue(pick?.reason.contains("le S&P 500 (SPY) n'est pas en tendance haussière : à sortir") ?? false, pick?.reason ?? "")
    }

    func testBackendKeepsTheRuleWhenTheMarketCannotBeRead() async {
        struct Offline: MarketFetcher { func data(_ url: String, method: String, body: Data?) async throws -> Data { throw MarketError("hors ligne") } }
        let v = TrendEngine.verdict(up, kind: .crypto)
        let out = await LocalBackend(fetcher: Offline(), bot: false).marketVerdict(eth, v)
        XCTAssertEqual(out.action, v.action)
        XCTAssertNil(out.market)
    }
}
