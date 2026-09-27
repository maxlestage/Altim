import XCTest
@testable import AltimCore

final class EngineTests: XCTestCase {
    let engine = SignalEngine()

    func testNotEnoughData() {
        XCTAssertThrowsError(try engine.analyze(Fixtures.candles(count: 30, drift: 0.01))) { error in
            XCTAssertEqual(error as? SignalEngine.EngineError, .notEnoughData(have: 30, need: 60))
        }
    }

    func testUptrendIsBullishDowntrendIsBearish() throws {
        let up = try engine.analyze(Fixtures.candles(count: 300, drift: 0.004))
        let down = try engine.analyze(Fixtures.candles(count: 300, drift: -0.004))
        XCTAssertGreaterThan(up.score, 0)
        XCTAssertLessThan(down.score, 0)
        XCTAssertFalse(up.action.isSell)
        XCTAssertFalse(down.action.isBuy)
        XCTAssertTrue((-100...100).contains(up.score))
        XCTAssertTrue((0...100).contains(up.confidence))
    }

    func testBuyPlanHasStopBelowAndTargetAbove() throws {
        let s = try engine.analyze(Fixtures.candles(count: 300, drift: 0.004))
        let plan = try XCTUnwrap(s.plan)
        XCTAssertLessThan(plan.stopLoss, plan.entry)
        XCTAssertGreaterThan(plan.takeProfit, plan.entry)
        XCTAssertEqual(plan.riskReward, 2, accuracy: 1e-6)
    }

    func testIgnoresUnclosedCandle() throws {
        var candles = Fixtures.candles(count: 300, drift: 0.004)
        let reference = try engine.analyze(candles)
        let last = candles.last!
        // Une bougie en formation avec un krach ne doit pas changer le signal.
        candles.append(Candle(time: last.time + 3600, open: last.close, high: last.close, low: last.close * 0.5,
                              close: last.close * 0.5, volume: 99_999, isClosed: false))
        let s = try engine.analyze(candles)
        XCTAssertEqual(s.score, reference.score)
        XCTAssertEqual(s.price, reference.price)
    }

    func testRejectsInvalidCandles() throws {
        var candles = Fixtures.candles(count: 300, drift: 0.004)
        let reference = try engine.analyze(candles)
        candles.insert(Candle(time: candles[10].time + 1, open: .nan, high: 1, low: 2, close: 1, volume: 1), at: 11)
        XCTAssertEqual(try engine.analyze(candles).score, reference.score)
    }

    func testHigherTimeframeDowngradesCounterTrendBuy() throws {
        let lower = Fixtures.candles(count: 300, drift: 0.004)
        let base = try engine.analyze(lower)
        let higherDown = Fixtures.candles(count: 120, drift: -0.01)
        let s = try engine.analyze(lower, higherTimeframe: higherDown)
        XCTAssertNotNil(s.factors.first { $0.name == "UT supérieure" && $0.score < 0 })
        if base.action.isBuy {
            XCTAssertNotEqual(s.action, .strongBuy)
        }
        XCTAssertLessThan(s.score, base.score)
    }

    func testFactorsExplainSignal() throws {
        let s = try engine.analyze(Fixtures.candles(count: 300, drift: 0.002))
        let names = Set(s.factors.map(\.name))
        XCTAssertTrue(names.isSuperset(of: ["Tendance", "MACD", "RSI", "Stochastique", "Bollinger", "Volume (OBV)"]))
    }

    func testStaleDataWarning() throws {
        let candles = Fixtures.candles(count: 300, drift: 0.002)
        let s = try engine.analyze(candles, timeframe: .h1, now: candles.last!.time + 86_400)
        XCTAssertTrue(s.warnings.contains { $0.contains("périmées") })
    }
}

final class RiskAndBacktestTests: XCTestCase {
    func testPositionSizing() throws {
        let rm = RiskManager()
        let size = try XCTUnwrap(rm.positionSize(equity: 10_000, plan: TradePlan(entry: 100, stopLoss: 95, takeProfit: 110)))
        // Budget 100 / (5 + 0,2 de frais) = 19,23 unités.
        XCTAssertEqual(size.quantity, 100 / 5.2, accuracy: 1e-9)
        XCTAssertEqual(size.riskAmount, 100, accuracy: 1e-9)
        XCTAssertFalse(size.capped)
    }

    func testPositionSizingIsCapped() throws {
        let rm = RiskManager()
        let size = try XCTUnwrap(rm.positionSize(equity: 10_000, plan: TradePlan(entry: 100, stopLoss: 99.9, takeProfit: 101)))
        XCTAssertTrue(size.capped)
        XCTAssertEqual(size.notional, 2_000, accuracy: 1e-6)
    }

    func testPreTradeChecks() {
        let rm = RiskManager()
        let good = TradePlan(entry: 100, stopLoss: 95, takeProfit: 110)
        XCTAssertTrue(rm.preTradeIssues(side: .buy, notional: 1_000, equity: 10_000, plan: good, realizedPnLToday: 0).isEmpty)
        XCTAssertFalse(rm.preTradeIssues(side: .buy, notional: 1_000, equity: 10_000, plan: nil, realizedPnLToday: 0).isEmpty)
        XCTAssertFalse(rm.preTradeIssues(side: .buy, notional: 5_000, equity: 10_000, plan: good, realizedPnLToday: 0).isEmpty)
        XCTAssertFalse(rm.preTradeIssues(side: .buy, notional: 1_000, equity: 10_000, plan: good, realizedPnLToday: -400).isEmpty)
        let badRR = TradePlan(entry: 100, stopLoss: 95, takeProfit: 102)
        XCTAssertFalse(rm.preTradeIssues(side: .buy, notional: 1_000, equity: 10_000, plan: badRR, realizedPnLToday: 0).isEmpty)
        XCTAssertTrue(rm.preTradeIssues(side: .sell, notional: 9_000, equity: 10_000, plan: nil, realizedPnLToday: -900).isEmpty)
    }

    func testBacktestProducesConsistentMetrics() {
        let result = Backtester().run(Fixtures.candles(count: 400, drift: 0.002, amplitude: 4))
        XCTAssertFalse(result.equityCurve.isEmpty)
        XCTAssertTrue(result.totalReturnPercent.isFinite)
        XCTAssertTrue((0...100).contains(result.maxDrawdownPercent))
        XCTAssertTrue((0...100).contains(result.winRatePercent))
        for trade in result.trades {
            XCTAssertLessThanOrEqual(trade.entryTime, trade.exitTime)
        }
        let compounded = result.trades.reduce(1.0) { $0 * (1 + $1.returnPercent / 100) }
        XCTAssertEqual((compounded - 1) * 100, result.totalReturnPercent, accuracy: 1e-6)
    }

    /// Le résultat au temps t ne doit pas dépendre des bougies futures.
    func testBacktestHasNoLookAhead() {
        let all = Fixtures.candles(count: 400, drift: 0.001, amplitude: 4)
        let short = Backtester().run(Array(all.prefix(300)))
        let long = Backtester().run(all)
        let cutoff = all[298].time
        let a = short.trades.filter { $0.exitTime < cutoff }
        let b = long.trades.filter { $0.exitTime < cutoff }
        XCTAssertEqual(a, b)
    }
}
