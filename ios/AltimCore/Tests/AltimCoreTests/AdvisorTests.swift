import XCTest
@testable import AltimCore

final class AdvisorTests: XCTestCase {
    func signal(_ action: SignalAction) throws -> Signal {
        let s = try SignalEngine().analyze(Fixtures.candles(count: 300, drift: 0.002))
        return Signal(action: action, score: s.score, confidence: 60, price: s.price, time: s.time, factors: s.factors, plan: s.plan, warnings: [])
    }

    func testBuyAdviceWithPrudentAmount() throws {
        let s = try signal(.buy)
        let a = Advisor.advise(signal: s, reliability: .high, price: s.price, line: nil, capital: 20_000, risk: RiskSettings())
        XCTAssertEqual(a.tone, .buy)
        XCTAssertLessThan(a.stop!, a.entry!)
        XCTAssertGreaterThan(a.amount!, 0)
        XCTAssertLessThanOrEqual(a.amount!, 20_000 * 0.2 + 1e-6)
        XCTAssertTrue(a.points.joined().contains("n'y consacrez pas plus"))
    }

    func testWithoutWealthInvitesToFillHoldings() throws {
        let s = try signal(.strongBuy)
        let a = Advisor.advise(signal: s, reliability: .high, price: s.price, line: nil, capital: nil, risk: RiskSettings())
        XCTAssertTrue(a.title.contains("signal fort"))
        XCTAssertNil(a.amount)
        XCTAssertTrue(a.points.joined().contains("Mes avoirs"))
    }

    func testWaitAvoidAndNoAdviceOnUnreliableData() throws {
        XCTAssertEqual(Advisor.advise(signal: try signal(.hold), reliability: .high, price: 1, line: nil, capital: nil, risk: RiskSettings()).tone, .hold)
        XCTAssertEqual(Advisor.advise(signal: try signal(.strongSell), reliability: .high, price: 1, line: nil, capital: nil, risk: RiskSettings()).tone, .sell)
        let low = Advisor.advise(signal: try signal(.strongBuy), reliability: .low, price: 1, line: nil, capital: 1000, risk: RiskSettings())
        XCTAssertEqual(low.tone, .unknown)
        XCTAssertNil(low.amount)
    }

    func testHeldAssetUsesPortfolioAnalysis() {
        let h = Holding(symbol: "BTC", kind: .crypto, name: "Bitcoin", quantity: 1, averagePrice: 100)
        let other = Holding(symbol: "AAPL", kind: .stock, name: "Apple", quantity: 1, averagePrice: 10)
        let input = HoldingsAnalyzer.MarketInput(price: 120, daily: [], daySignal: .init(action: .hold, score: 0),
                                                 shortSignal: .init(action: .hold, score: 0), reliability: .high)
        let analysis = HoldingsAnalyzer.analyze(holdings: [h, other], cash: 0, market: ["crypto:BTC": input])
        let a = Advisor.advise(signal: nil, reliability: .high, price: 120, line: analysis.lines[0], capital: analysis.total, risk: RiskSettings())
        XCTAssertEqual(a.title, "Alléger") // BTC ≈ 92 % of the wealth
        XCTAssertTrue(a.points[0].hasPrefix("Vous en détenez 1"))
        XCTAssertTrue(a.points.contains { $0.contains("Montant à alléger") })
    }

    func testBuySignalThatMostlyFailedOnThisAssetBecomesWait() throws {
        let s = try signal(.strongBuy)
        let poor = Advisor.advise(signal: s, reliability: .high, price: s.price, line: nil, capital: 20_000, risk: RiskSettings(),
                                  track: .init(trades: 12, winRate: 25, avgReturn: -0.8))
        XCTAssertEqual(poor.tone, .hold)
        XCTAssertTrue(poor.title.contains("peu fiable"))
        XCTAssertNil(poor.amount)
        XCTAssertTrue(poor.points[0].contains("25 %"))
        XCTAssertEqual(Advisor.advise(signal: try signal(.buy), reliability: .high, price: s.price, line: nil, capital: nil, risk: RiskSettings(),
                                      track: .init(trades: 8, winRate: 55, avgReturn: -0.1)).tone, .hold)
        let good = Advisor.advise(signal: try signal(.buy), reliability: .high, price: s.price, line: nil, capital: nil, risk: RiskSettings(),
                                  track: .init(trades: 9, winRate: 56, avgReturn: 1.4))
        XCTAssertEqual(good.tone, .buy)
        XCTAssertTrue(good.points.joined(separator: " ").contains("9 signaux d'achat, 56 % gagnants, +1,4 %"))
        let few = Advisor.advise(signal: try signal(.buy), reliability: .high, price: s.price, line: nil, capital: nil, risk: RiskSettings(),
                                 track: .init(trades: 2, winRate: 0, avgReturn: -2))
        XCTAssertEqual(few.tone, .buy)
        XCTAssertTrue(few.points.joined(separator: " ").contains("pas assez pour juger"))
    }

    func testTrackRecordFromBacktest() throws {
        let r = Backtester().run(Fixtures.candles(count: 600, drift: 0.002))
        let t = Advisor.TrackRecord(r)
        XCTAssertEqual(t.trades, r.trades.count)
        if !r.trades.isEmpty {
            XCTAssertEqual(t.winRate, r.winRatePercent, accuracy: 1e-9)
            XCTAssertEqual(t.avgReturn, r.trades.map(\.returnPercent).reduce(0, +) / Double(r.trades.count), accuracy: 1e-9)
        }
    }

    func testPreciseQuantitiesAndPrices() throws {
        XCTAssertEqual(Advisor.quantity(amount: 1000, price: 84_871.45, kind: .crypto), 0.0117825, accuracy: 1e-12)
        XCTAssertEqual(Advisor.quantity(amount: 1000, price: 341.07, kind: .stock), 2)
        XCTAssertEqual(Advisor.quantity(amount: 100, price: 341.07, kind: .stock), 0)
        XCTAssertEqual(Advisor.quantity(amount: 50, price: 0.000009312, kind: .crypto), 5_369_410, accuracy: 1e-6)
        XCTAssertEqual(Advisor.quantityText(2, kind: .stock, symbol: "AAPL"), "2 actions AAPL")
        XCTAssertEqual(Advisor.quantityText(0.0117825, kind: .crypto, symbol: "BTC"), "0,0117825 BTC")
        XCTAssertEqual(Advisor.px(0.000009312), "0,000009312 $")
        let s = try signal(.buy)
        let a = Advisor.advise(signal: s, reliability: .high, price: s.price, line: nil, capital: 20_000, risk: RiskSettings(), symbol: "BTC", kind: .crypto)
        XCTAssertLessThanOrEqual(a.quantity! * s.price, a.amount! + 1e-9)
        XCTAssertTrue(a.points.joined(separator: " ").contains(" BTC :"))
    }

    func testWholeSharesAmountMatchesRoundedQuantity() throws {
        let s = try signal(.buy)
        let a = Advisor.advise(signal: s, reliability: .high, price: 341.07, line: nil, capital: 20_000, risk: RiskSettings(),
                               symbol: "AAPL", kind: .stock)
        XCTAssertEqual(a.quantity!, a.quantity!.rounded())
        XCTAssertEqual(a.amount!, a.quantity! * 341.07, accuracy: 1e-6)
        XCTAssertTrue(a.points.joined(separator: " ").contains("soit \(Int(a.quantity!)) actions AAPL"))
    }

    func testMarketGuardBlocksBuysInShockAndOnLikelyReversal() throws {
        let s = try signal(.strongBuy)
        let args = { (g: Advisor.GuardContext?) in
            Advisor.advise(signal: s, reliability: .high, price: s.price, line: nil, capital: 20_000, risk: RiskSettings(),
                           symbol: "BTC", kind: .crypto, guard: g)
        }
        XCTAssertTrue(args(.init(shock: .shock, reversalScore: 0, reversalDirection: nil)).title.contains("choc"))
        XCTAssertTrue(args(.init(shock: .calm, reversalScore: 60, reversalDirection: .down)).title.contains("retournement"))
        XCTAssertEqual(args(.init(shock: .calm, reversalScore: 80, reversalDirection: .up)).tone, .buy)
        let calm = args(.init(shock: .calm, reversalScore: 0, reversalDirection: nil))
        let agitated = args(.init(shock: .agitated, reversalScore: 0, reversalDirection: nil))
        XCTAssertEqual(agitated.tone, .buy)
        XCTAssertEqual(agitated.amount!, calm.amount! / 2, accuracy: 1)
        XCTAssertLessThanOrEqual(agitated.quantity! * s.price, agitated.amount! + 1e-9)
        XCTAssertTrue(agitated.points.joined(separator: " ").contains("divisé par deux"))
    }
}
