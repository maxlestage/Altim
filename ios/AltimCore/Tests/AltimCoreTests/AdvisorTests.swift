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
}
