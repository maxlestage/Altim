import Foundation
import XCTest
@testable import AltimKit

/// Same cases as web/test/portfolio-risk.test.ts, with the texts and figures the web engine gives for them
/// (computed by web/src/engine/portfolio-risk.ts on the same deterministic series).
final class PortfolioRiskTests: XCTestCase {
    static let day = 86_400_000.0
    static let t0 = 1_767_225_600_000.0 // Date.UTC(2026, 0, 1)

    /// Daily candles from closes (high/low ± 1 %).
    static func series(_ closes: [Double], start: Double = t0) -> [Candle] {
        closes.enumerated().map { i, c in Candle(time: start + Double(i) * day, open: c, high: c * 1.01, low: c * 0.99, close: c, volume: 1) }
    }

    /// Deterministic pseudo-random daily returns (same generator as the web test).
    static func returns(_ n: Int, seed: Double) -> [Double] {
        var x = seed
        return (0..<n).map { _ in
            x = (x * 16807).truncatingRemainder(dividingBy: 2_147_483_647)
            return (x / 2_147_483_647 - 0.5) * 0.06
        }
    }

    static func path(_ r: [Double], start: Double = 100) -> [Double] { r.reduce(into: [start]) { $0.append($0.last! * (1 + $1)) } }

    static func holding(_ symbol: String, _ kind: Kind, _ quantity: Double, _ average: Double, stop: Double? = nil) -> Holding {
        Holding(asset: Asset(symbol: symbol, kind: kind, name: symbol), quantity: quantity, averagePrice: average, stop: stop)
    }

    // MARK: Beta

    func testBetaOfTwiceTheBenchmark() {
        let bench = Self.returns(120, seed: 7)
        let b = RiskEngine.estimateBeta(Self.series(Self.path(bench.map { 2 * $0 })), Self.series(Self.path(bench)))
        XCTAssertTrue(b.estimated)
        XCTAssertEqual(b.days, 90)
        XCTAssertEqual(b.beta, 2, accuracy: 1e-6)
    }

    func testShortHistoryFallsBackToOne() {
        let bench = Self.returns(120, seed: 7)
        // A recent listing: 20 days shared with the benchmark.
        let b = RiskEngine.estimateBeta(Self.series(Self.path(Array(bench.prefix(20))), start: Self.t0 + 100 * Self.day), Self.series(Self.path(bench)))
        XCTAssertEqual(b, BetaEstimate(beta: 1, days: 0, estimated: false))
    }

    // MARK: Stress scenarios

    func testStressScenarios() throws {
        let eth = Self.holding("ETH", .crypto, 1, 1000), aapl = Self.holding("AAPL", .stock, 10, 100)
        let p = RiskPortfolio(holdings: [eth, aapl], cash: 1000, prices: ["crypto:ETH": 1000, "stock:AAPL": 100], daily: [:])
        let r = RiskEngine.stressTest(p, betas: ["crypto:ETH": BetaEstimate(beta: 1.5, days: 90, estimated: true)])
        let all10 = try XCTUnwrap(r.first { $0.scenario.key == "all10" })
        // ETH 1000 × 1.5 × 10 % + AAPL 1000 × 1 × 10 % (no beta: 1)
        XCTAssertEqual(all10.loss, 250, accuracy: 1e-9)
        XCTAssertEqual(all10.lossPercent, 250.0 / 3000 * 100, accuracy: 1e-9)
        XCTAssertEqual(all10.investedLossPercent, 250.0 / 2000 * 100, accuracy: 1e-9)
        XCTAssertEqual(all10.worst?.symbol, "ETH")
        XCTAssertEqual(try XCTUnwrap(r.first { $0.scenario.key == "crypto20" }).loss, 300, accuracy: 1e-9)
        XCTAssertEqual(try XCTUnwrap(r.first { $0.scenario.key == "stock10crypto30" }).loss, 450 + 100, accuracy: 1e-9)
        XCTAssertEqual(r.map(\.scenario.key), RiskEngine.scenarios.map(\.key))
        // A line never loses more than its value.
        let big = try XCTUnwrap(RiskEngine.stressTest(p, betas: ["crypto:ETH": BetaEstimate(beta: 5, days: 90, estimated: true)]).first { $0.scenario.key == "all30" })
        XCTAssertEqual(big.loss, 1000 + 300, accuracy: 1e-9)
    }

    // MARK: Limits

    struct Market {
        let eth: [Candle], sol: [Candle], aapl: [Candle]
        init() {
            let bench = PortfolioRiskTests.returns(120, seed: 11)
            eth = PortfolioRiskTests.series(PortfolioRiskTests.path(bench))
            sol = PortfolioRiskTests.series(PortfolioRiskTests.path(bench.enumerated().map { i, x in x * 1.2 + (i % 2 == 1 ? 0.001 : -0.001) }))
            aapl = PortfolioRiskTests.series(PortfolioRiskTests.path(PortfolioRiskTests.returns(120, seed: 99)))
        }
        var daily: [String: [Candle]] { ["crypto:ETH": eth, "crypto:SOL": sol, "stock:AAPL": aapl] }
    }

    func testCorrelatedCluster() throws {
        let m = Market()
        let c = RiskEngine.correlatedClusters([("ETH", 30, m.eth), ("SOL", 25, m.sol), ("AAPL", 20, m.aapl)])
        XCTAssertEqual(c.count, 1)
        XCTAssertEqual(c[0].symbols.sorted(), ["ETH", "SOL"])
        XCTAssertEqual(c[0].weight, 55)
        XCTAssertEqual(c[0].averageCorrelation, 0.998962977479984, accuracy: 1e-12)
        XCTAssertEqual(c[0].days, 90)
        // Same betas as the web engine.
        XCTAssertEqual(RiskEngine.estimateBeta(m.sol, m.eth).beta, 1.1917103637795416, accuracy: 1e-12)
        XCTAssertEqual(RiskEngine.estimateBeta(m.aapl, m.eth).beta, 0.1918897926271144, accuracy: 1e-12)
    }

    func testLimitsAgainstTheSettings() throws {
        let m = Market()
        let now = Self.t0 + 120 * Self.day + 3_600_000
        let holdings = [Self.holding("ETH", .crypto, 1, m.eth.last!.close), Self.holding("SOL", .crypto, 1, m.sol.last!.close), Self.holding("AAPL", .stock, 1, m.aapl.last!.close)]
        // Prices 10 % under the previous close (the last candle is today's): a big daily loss.
        let prices = ["crypto:ETH": m.eth[m.eth.count - 2].close * 0.9, "crypto:SOL": m.sol[m.sol.count - 2].close * 0.9, "stock:AAPL": m.aapl[m.aapl.count - 2].close * 0.9]
        let p = RiskPortfolio(holdings: holdings, prices: prices, daily: m.daily)
        XCTAssertEqual(p.total, 302.50238899038237, accuracy: 1e-9)
        XCTAssertEqual(try XCTUnwrap(p.lines[0].protectiveStop), 95.19283905075798, accuracy: 1e-9)
        let d = try XCTUnwrap(RiskEngine.dailyChange(p, daily: m.daily, now: now))
        XCTAssertEqual(d.percent, -10, accuracy: 1e-6)
        XCTAssertEqual(d.change, -33.61137655448694, accuracy: 1e-9)
        XCTAssertEqual(d.covered, 3)
        let clusters = RiskEngine.clusters(p, daily: m.daily)
        XCTAssertEqual(try XCTUnwrap(clusters.first).weight, 67.26007236529378, accuracy: 1e-9)
        let checks = RiskEngine.checkLimits(p, RiskSettings(maxPositionPercent: 20, maxCryptoPercent: 30), clusters: clusters, daily: d)
        let by = Dictionary(uniqueKeysWithValues: checks.map { ($0.code, $0) })
        XCTAssertEqual(checks.map(\.code), ["risk_per_trade", "max_weight", "crypto_cap", "cluster", "daily_loss"])
        XCTAssertEqual(by["max_weight"]?.level, .danger)
        XCTAssertEqual(by["crypto_cap"]?.level, .warning)
        XCTAssertEqual(by["cluster"]?.level, .warning)
        XCTAssertEqual(by["daily_loss"]?.level, .danger)
        XCTAssertTrue(by["daily_loss"]!.detail.contains(RiskEngine.dailyLossReached))
        // Texts identical to the web's.
        XCTAssertEqual(by["risk_per_trade"]?.label, "Risque par ligne (max 1 %)")
        XCTAssertEqual(by["risk_per_trade"]?.detail, "Si le stop était touché, ETH coûterait 6 $ (1,9 %), SOL coûterait 6 $ (2,1 %), AAPL coûterait 6 $ (2 %) : plus que votre risque accepté par idée (3 $). Réduisez la ligne ou rapprochez le stop.")
        XCTAssertEqual(by["max_weight"]?.detail, "ETH 33,3 %, SOL 33,9 %, AAPL 32,7 % : au-dessus de votre maximum. Surexposition à un seul actif.")
        XCTAssertEqual(by["crypto_cap"]?.label, "Part crypto (max 30 %)")
        XCTAssertEqual(by["crypto_cap"]?.detail, "67,3 % du patrimoine en crypto, au-dessus de votre plafond : un repli des cryptos toucherait tout le portefeuille.")
        XCTAssertEqual(by["cluster"]?.label, "Actifs corrélés (groupe max 40 %)")
        XCTAssertEqual(by["cluster"]?.detail, "ETH + SOL : 67,3 % du patrimoine, corrélation moyenne 1.00 sur 90 jours. Ils se comportent comme une seule grosse ligne.")
        XCTAssertEqual(by["daily_loss"]?.label, "Perte du jour (max 3 %)")
        XCTAssertEqual(by["daily_loss"]?.detail, "Limite de perte du jour atteinte : n'ouvrez plus de position aujourd'hui. −34 $ (−10 %) depuis la clôture de la veille.")

        let calm = RiskEngine.checkLimits(p, RiskSettings(riskPerTradePercent: 100, maxPositionPercent: 100, dailyLossLimitPercent: 50, maxCryptoPercent: 100), clusters: [], daily: d)
        XCTAssertTrue(calm.allSatisfy { $0.level == .ok })
        let na = RiskEngine.checkLimits(p, .defaults, clusters: [], daily: nil)
        XCTAssertEqual(na.first { $0.code == "daily_loss" }?.level, .na)
        XCTAssertEqual(na.first { $0.code == "daily_loss" }?.detail, "Clôture de la veille indisponible : variation du jour inconnue.")
        XCTAssertEqual(na.first { $0.code == "cluster" }?.detail, "Aucun groupe d'actifs corrélés à plus de 0,7.")
        XCTAssertEqual(na.first { $0.code == "crypto_cap" }?.label, "Part crypto (max 60 %)")
    }

    func testStressWithEstimatedBeta() throws {
        let m = Market()
        let holdings = [Self.holding("ETH", .crypto, 1, m.eth.last!.close), Self.holding("SOL", .crypto, 1, m.sol.last!.close), Self.holding("AAPL", .stock, 1, m.aapl.last!.close)]
        let prices = ["crypto:ETH": m.eth[m.eth.count - 2].close * 0.9, "crypto:SOL": m.sol[m.sol.count - 2].close * 0.9, "stock:AAPL": m.aapl[m.aapl.count - 2].close * 0.9]
        let p = RiskPortfolio(holdings: holdings, prices: prices, daily: m.daily)
        let r = RiskEngine.stressTest(p, betas: ["crypto:SOL": RiskEngine.estimateBeta(m.sol, m.eth)])
        let expected = [16.108742320431247, 32.217484640862494, 64.43496928172499, 96.65245392258748, 44.62715663198326, 76.84464127284576]
        for (x, e) in zip(r, expected) { XCTAssertEqual(x.loss, e, accuracy: 1e-9) }
        XCTAssertEqual(r[3].worst?.symbol, "SOL")
        XCTAssertEqual(try XCTUnwrap(r[3].worst).movePercent, -35.75131091338625, accuracy: 1e-9)
        XCTAssertEqual(r[3].lossPercent, 31.950972104772507, accuracy: 1e-9)
        // Texts of the card, as RiskCards.tsx writes them.
        XCTAssertEqual(RiskText.stressLine(r[3], cash: 0), "Perte ≈ −97 $, soit 32 % du patrimoine.")
        XCTAssertEqual(RiskText.worst(try XCTUnwrap(r[3].worst)), "Ligne la plus touchée : SOL (−35,8 %, −37 $)")
        XCTAssertEqual(RiskText.stressNote(p, betas: ["crypto:SOL": RiskEngine.estimateBeta(m.sol, m.eth)]),
                       "Hypothèse : choc instantané, bêta constant. Bêta estimé sur 90 jours de rendements journaliers (SOL 1.19). Historique trop court (moins de 30 jours communs) ou indisponible pour ETH, AAPL : bêta 1 retenu. Une vraie crise peut aller plus loin : les corrélations montent quand tout baisse.")
        let withCash = RiskPortfolio(holdings: [Self.holding("ETH", .crypto, 1, 1000), Self.holding("AAPL", .stock, 10, 100)], cash: 1000,
                                     prices: ["crypto:ETH": 1000, "stock:AAPL": 100], daily: [:])
        let all10 = RiskEngine.stressTest(withCash, betas: ["crypto:ETH": BetaEstimate(beta: 1.5, days: 90, estimated: true)])[1]
        XCTAssertEqual(RiskText.stressLine(all10, cash: 1000), "Perte ≈ −250 $, soit 8,3 % du patrimoine (12,5 % de vos placements : les liquidités amortissent 4,2 %).")
        XCTAssertEqual(RiskText.userStop(95, price: 101), "Votre stop : 95,00 $ (5,9 % sous le cours).")
    }

    func testBenchmarkBetas() {
        let m = Market()
        let p = RiskPortfolio(holdings: [Self.holding("BTC", .crypto, 1, 100), Self.holding("SOL", .crypto, 1, 100)], prices: ["crypto:BTC": 100, "crypto:SOL": 100], daily: [:])
        let b = RiskEngine.betas(p, daily: ["crypto:BTC": m.eth, "crypto:SOL": m.sol])
        XCTAssertEqual(b["crypto:BTC"], BetaEstimate(beta: 1, days: 0, estimated: false, reference: true))
        XCTAssertEqual(b["crypto:SOL"]?.beta ?? 0, 1.1917103637795416, accuracy: 1e-12)
    }

    func testPreviousClose() {
        let c = Self.series([10, 11, 12])
        XCTAssertEqual(RiskEngine.previousClose(c, now: Self.t0 + 2 * Self.day + 5000), 11)
        XCTAssertEqual(RiskEngine.previousClose(c, now: Self.t0 + 3 * Self.day + 5000), 12)
        XCTAssertNil(RiskEngine.previousClose([], now: Self.t0))
    }

    // MARK: Dangerous positions

    func testDangerousPositions() throws {
        let c = Self.series((0..<30).map { 100 + Double($0 % 2) })
        let holdings = [Self.holding("BTC", .crypto, 1, 100, stop: 101.5), Self.holding("ETH", .crypto, 1, 100, stop: 99.5), Self.holding("AAPL", .stock, 1, 200), Self.holding("MSFT", .stock, 1, 50, stop: 10)]
        let daily = ["crypto:BTC": c, "crypto:ETH": c, "stock:AAPL": c, "stock:MSFT": c]
        let p = RiskPortfolio(holdings: holdings, cash: 10_000, prices: ["crypto:BTC": 101, "crypto:ETH": 101, "stock:AAPL": 101, "stock:MSFT": 101], daily: daily)
        XCTAssertEqual(p.total, 10_404)
        let d = RiskEngine.dangerousPositions(p, .defaults, daily: daily)
        let by = Dictionary(uniqueKeysWithValues: d.map { ($0.symbol, $0) })
        XCTAssertEqual(by["BTC"]?.reasons.map(\.code), [.stopBroken])
        XCTAssertEqual(by["BTC"]?.reasons.first?.text, "Stop cassé : cours 101 $ sous votre stop 101,5 $.")
        XCTAssertEqual(by["ETH"]?.reasons.map(\.code), [.nearStop])
        XCTAssertEqual(by["ETH"]?.reasons.first?.text, "À moins d'une volatilité journalière (ATR 2,0103 $) de votre stop 99,5 $.")
        // AAPL: −99 $ latent on a 10 404 $ portfolio (1 % = 104 $): not yet; with 0.5 % it is.
        XCTAssertNil(by["AAPL"])
        XCTAssertNil(by["MSFT"])
        let strict = RiskEngine.dangerousPositions(p, RiskSettings(riskPerTradePercent: 0.5), daily: [:], stops: [:])
        XCTAssertEqual(strict.map(\.symbol), ["AAPL"])
        XCTAssertEqual(strict.first?.reasons.first?.text, "Perte latente de 99 $ (1 % du patrimoine), au-delà de votre risque accepté par idée (0,5 %).")
        let checks = RiskEngine.checkLimits(p, .defaults, clusters: [], daily: nil)
        XCTAssertEqual(checks.first?.detail, "Aucune ligne ne perdrait plus de 104 $ à son stop.")
        XCTAssertEqual(checks.first { $0.code == "crypto_cap" }?.detail, "1,9 % du patrimoine en crypto.")
    }

    // MARK: Settings and saved state

    func testSettingsBoundsAndStorage() throws {
        let s = RiskSettings.defaults
        XCTAssertEqual(s, RiskSettings(riskPerTradePercent: 1, maxPositionPercent: 20, dailyLossLimitPercent: 3, maxCryptoPercent: 60))
        let daily = RiskSettings.fields[2]
        XCTAssertEqual(s.stepped(daily, up: true).dailyLossLimitPercent, 3.5)
        var low = s
        for _ in 0..<20 { low = low.stepped(daily, up: false) }
        XCTAssertEqual(low.dailyLossLimitPercent, 0.5)
        // Older or damaged settings: missing and out-of-range values take the default.
        let decoded = try JSONDecoder().decode(RiskSettings.self, from: Data(#"{"riskPerTradePercent": 2, "maxCryptoPercent": 400}"#.utf8))
        XCTAssertEqual(decoded, RiskSettings(riskPerTradePercent: 2))
        let back = try JSONDecoder().decode(RiskSettings.self, from: JSONEncoder().encode(low))
        XCTAssertEqual(back, low)
    }

    func testHoldingStopIsOptionalAndCleaned() throws {
        // A line saved by an older version (no stop) still reads.
        let old = #"[{"id":"6F9619FF-8B86-D011-B42D-00C04FC964FF","asset":{"symbol":"BTC","kind":"crypto","name":"Bitcoin"},"quantity":0.5,"averagePrice":60000}]"#
        let h = try JSONDecoder().decode([Holding].self, from: Data(old.utf8))
        XCTAssertNil(h[0].stop)
        XCTAssertNil(Holding(asset: h[0].asset, quantity: 1, averagePrice: nil, stop: -3).stop)
        var bad = h[0]
        bad.stop = 0
        XCTAssertNil(bad.cleaned.stop)
        let dangers = DangerState(at: 42, items: [Danger(id: "1", symbol: "BTC", kind: .crypto, name: "Bitcoin", reasons: [.init(code: .stopBroken, text: "Stop cassé")])])
        XCTAssertEqual(try JSONDecoder().decode(DangerState.self, from: JSONEncoder().encode(dangers)), dangers)
        XCTAssertTrue(String(decoding: try JSONEncoder().encode(dangers), as: UTF8.self).contains("stop_broken"))
    }
}
