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

extension MarketFilterTests {
    /// The time machine replays the market filter: an asset rising while its market falls stays in cash.
    func testBacktestReplaysTheMarketFilter() {
        let alone = TrendEngine.backtest(up, asset: eth)
        XCTAssertGreaterThan(alone.exposure, 50)
        let filtered = TrendEngine.backtest(up, asset: eth, market: down)
        XCTAssertEqual(filtered.exposure, 0, accuracy: 1e-9, "marché baissier tout du long : jamais investi")
        XCTAssertEqual(filtered.strategyReturn, 0, accuracy: 1e-9)
        let same = TrendEngine.backtest(up, asset: eth, market: BotEngineTests.series(drift: 0.002, seed: 2))
        XCTAssertEqual(same.exposure, alone.exposure, accuracy: 5, "marché haussier : presque comme la règle seule")
        XCTAssertEqual(TrendEngine.backtest(up, asset: eth, market: Array(down.prefix(100))).exposure, alone.exposure, accuracy: 1e-9,
                       "marché trop court : la règle seule")
    }
}

extension MarketFilterTests {
    /// The Selection's test replays the market filter: rising assets in a falling market stay in cash.
    func testSelectionTestReplaysTheMarketFilter() throws {
        let data = (0..<4).map { (Asset(symbol: "A\($0)", kind: .crypto, name: "A\($0)"), BotEngineTests.series(drift: 0.002, seed: Double($0))) }
        let alone = try XCTUnwrap(Rotation.sleeves(data, kind: .crypto))
        let filtered = try XCTUnwrap(Rotation.sleeves(data, kind: .crypto, market: down))
        XCTAssertGreaterThan(alone.ret, 0)
        XCTAssertEqual(filtered.ret, 0, accuracy: 1e-9, "marché baissier : tout en liquidités")
        XCTAssertEqual(filtered.basket, alone.basket, accuracy: 1e-9, "le panier de comparaison ne change pas")
    }
}

extension MarketFilterTests {
    /// A configuration change caused by the market says so, with the right way back (not the asset's own average).
    func testConfigurationChangeNamesTheMarket() {
        Money.setDisplay(.usd, nil)
        let v = TrendEngine.verdict(up, kind: .crypto)
        var (s, t) = ConfigChanges.apply(ConfigState(), verdict: TrendEngine.applyMarket(true, kind: .crypto, to: v), asset: eth, now: 1)
        XCTAssertNil(t)
        XCTAssertTrue(s.last.values.first?.triggers.first?.hasSuffix("ou si le Bitcoin quitte sa tendance haussière") ?? false)
        (s, t) = ConfigChanges.apply(s, verdict: TrendEngine.applyMarket(false, kind: .crypto, to: v), asset: eth, now: 2)
        XCTAssertEqual(t?.change, "ACHETER / CONSERVER → SORTIR (MARCHÉ)")
        XCTAssertEqual(t?.to.levelLabel, "Marché hors tendance haussière")
        XCTAssertEqual(t?.triggers.first, "Retour à l'achat quand le Bitcoin repasse en tendance haussière (au-dessus de sa moyenne 200 jours, momentum positif), si l'actif y est encore")
        let held = BotEngine.apply(-0.02, to: v)
        let h = ConfigChanges.apply(ConfigState(), verdict: held, asset: eth, now: 1).state.last.values.first
        XCTAssertEqual(h?.levelLabel, "Achat bloqué par le bot Tendance")
        XCTAssertEqual(h?.triggers.first, "Retour à l'achat quand le bot Tendance ne prévoit plus de baisse sur 20 jours, si la tendance de l'actif tient")
        let down = ConfigChanges.apply(ConfigState(), verdict: TrendEngine.verdict(self.down, kind: .crypto), asset: eth, now: 1).state.last.values.first
        XCTAssertEqual(down?.levelLabel, "Tendance baissière")
        XCTAssertTrue(down?.triggers.first?.hasPrefix("Retour à l'achat quand le prix repasse au-dessus de") ?? false)
    }
}

extension MarketFilterTests {
    /// Market candles that cannot be trusted (sources in disagreement, stale history) never decide: the rule alone.
    func testUntrustedMarketIsIgnored() {
        let low = Reliability(score: 20, level: "low", independent: 1, conflict: true)
        let high = Reliability(score: 90, level: "high", independent: 3, conflict: false)
        let last = down.last!.time
        XCTAssertEqual(TrendEngine.marketUp(down, kind: .crypto, reliability: high, assetTime: last), false)
        XCTAssertNil(TrendEngine.marketUp(down, kind: .crypto, reliability: low, assetTime: last), "sources en désaccord")
        XCTAssertNil(TrendEngine.marketUp(down, kind: .crypto, reliability: high, assetTime: last + 6 * 86_400_000), "historique du marché périmé")
        XCTAssertEqual(TrendEngine.marketUp(down, kind: .stock, reliability: nil, assetTime: last + 3 * 86_400_000), false, "week-end de 3 jours : encore valable")
        let v = TrendEngine.verdict(up, kind: .crypto)
        XCTAssertEqual(TrendEngine.applyMarket(TrendEngine.marketUp(down, kind: .crypto, reliability: low, assetTime: last), kind: .crypto, to: v).action, v.action)
    }
}

extension MarketFilterTests {
    /// The market filter's real exits are followed apart in the summary of the verdicts (its lab result is fragile).
    func testMarketExitsAreFollowedApart() {
        var v = TrendEngine.applyMarket(false, kind: .crypto, to: TrendEngine.verdict(up, kind: .crypto))
        v.price = 100
        let log = VerdictLog.record([], v, asset: eth, now: 1)
        let s = VerdictLog.summary(VerdictLog.outcomes(log, prices: ["crypto:ETH": 110]))
        XCTAssertEqual(s.marketExitCount, 1)
        XCTAssertEqual(s.marketExitMean ?? 0, 10, accuracy: 1e-9)
        XCTAssertEqual(s.botExitCount, 0)
        XCTAssertTrue(s.text.contains("Dont sorties décidées par le filtre de marché : +10,0 % en moyenne ensuite (1 cas), une hausse manquée."), s.text)
    }
}
