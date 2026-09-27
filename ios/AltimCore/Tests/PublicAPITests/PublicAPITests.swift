import XCTest
import AltimCore // volontairement sans @testable

/// Mirrors the calls made by the iOS app (ios/Altim): if an API is not public,
/// this file does not compile — the error is caught on Linux before the Xcode build.
final class PublicAPITests: XCTestCase {
    func testAppFacingAPIsArePublic() async throws {
        let asset = Asset.defaults[0]
        _ = (asset.base, asset.name, asset.symbol, asset.id, asset.quote, asset.assetClass.label)
        _ = Timeframe.allCases.map { ($0.label, $0.higher, $0.id) }

        let candles = (0..<120).map { i in
            Candle(time: Date(timeIntervalSince1970: Double(i) * 3600), open: 100, high: 110, low: 90,
                   close: 100 + Double(i % 7), volume: 10)
        }
        _ = (candles.closes, candles.sanitized(), candles[0].isClosed)
        _ = Indicators.ema(candles.closes, period: 20)

        let plan = TradePlan(entry: 100, stopLoss: 95, takeProfit: 110)
        _ = (plan.riskReward, plan.entry, plan.stopLoss, plan.takeProfit)
        let raw = try SignalEngine().analyze(candles, higherTimeframe: nil, timeframe: .h1)
        _ = (raw.action.label, raw.action.isBuy, raw.action.isSell, raw.score, raw.confidence, raw.price, raw.time,
             raw.factors.map { ($0.name, $0.score, $0.detail, $0.id) }, raw.plan, raw.warnings)
        _ = Signal(action: .hold, score: 0, confidence: 0, price: 1, time: Date(), factors: [], plan: nil, warnings: [])

        let bt = Backtester().run(candles)
        _ = (bt.totalReturnPercent, bt.buyAndHoldPercent, bt.winRatePercent, bt.maxDrawdownPercent, bt.beatsBuyAndHold,
             bt.trades.map { ($0.entryTime, $0.entryPrice, $0.id) })

        var risk = RiskSettings()
        risk.riskPerTradePercent = 1
        risk.maxPositionPercent = 20
        risk.dailyLossLimitPercent = 3
        risk.minRiskReward = 1.5
        let size = RiskManager(settings: risk).positionSize(equity: 1000, plan: plan)
        _ = size.map { ($0.quantity, $0.riskAmount, $0.percentOfEquity, $0.capped, $0.notional) }
        _ = RiskManager(settings: risk).preTradeIssues(side: .buy, notional: 10, equity: 100, plan: plan, realizedPnLToday: 0)

        let transport = ResilientTransport()
        var market = ConsensusMarketData.standard(transport: transport, alpaca: (key: "", secret: ""),
                                                  twelveDataKey: "", polygonKey: "", finnhubKey: "")
        market.targetSources = market.sources.count
        _ = market.sources.map(\.name)
        let sentiment = SentimentProvider(transport: transport)
        _ = sentiment

        let paper = PaperBroker(startingCash: 1000, quoteAsset: "USDT") { _ in 100 }
        let brokers: [Broker] = [paper,
                                 BinanceBroker(apiKey: "", secret: "", environment: .test),
                                 AlpacaBroker(keyId: "", secret: "", environment: .live)]
        _ = brokers.map { ($0.name, $0.environment.label) }
        let rules = try await paper.rules(for: "BTCUSDT")
        let order = OrderRequest(symbol: "BTCUSDT", side: .buy, type: .limit, quantity: 1, limitPrice: 100,
                                 stopLoss: 95, takeProfit: 110)
        _ = (rules.normalizeQuantity(1), rules.normalizePrice(1), rules.baseAsset, rules.quoteAsset,
             rules.issues(for: order, referencePrice: 100))
        _ = try await paper.lastPrice(for: "BTCUSDT")
        let balances = try await paper.balances()
        _ = balances.map { ($0.asset, $0.free, $0.locked, $0.total, $0.id) }
        let result = try await paper.place(OrderRequest(symbol: "BTCUSDT", side: .buy, type: .market, quantity: 1))
        _ = (result.orderId, result.status, result.executedQuantity, result.averagePrice, result.notes, result.isSimulated,
             result.symbol, result.side.label)
        _ = OrderType.allCases.map(\.label)
        _ = BrokerError.validation(["x"])
        _ = PriceGuard.issue(brokerPrice: 1, consensusPrice: 1, tolerancePercent: 1)
        _ = 1.5.decimal

        // Snapshot / reliability (via the types exposed to the views).
        func use(_ s: MarketSnapshot, _ fg: FearGreedIndex, _ social: SocialSentiment) {
            _ = (s.reliability.label, s.reliabilityScore, s.summary, s.primarySource, s.candles, s.consensusPrice,
                 s.quality.issues, s.agreeingSources, s.independentSources)
            for c in s.checks {
                switch c.status {
                case .primary, .agrees, .diverges, .skipped: break
                case .failed(let m): _ = m
                }
                _ = (c.name, c.lastPrice, c.deviationPercent, c.id)
            }
            _ = ReliabilityGate.apply(raw, snapshot: s)
            _ = (fg.value, fg.label, social.bullishPercent, social.sampleSize)
        }
        _ = use
        _ = ReliabilityLevel.high.label

        // Holdings (SQLite) and analysis, as used by the "Mes avoirs" screen.
        let hdb = try HoldingsDatabase(path: ":memory:")
        _ = try? HoldingsDatabase.defaultURL()
        let saved = try await hdb.upsert(Holding(symbol: "BTC", kind: .crypto, name: "Bitcoin", quantity: 1, averagePrice: 100), merge: true)
        _ = (saved.id, saved.symbol, saved.kind, saved.name, saved.quantity, saved.averagePrice, saved.isValid)
        let stored = try await hdb.all()
        try await hdb.setCash(10)
        _ = try await hdb.cash()
        let json = try await hdb.exportJSON()
        try await hdb.importJSON(json)
        try await hdb.delete(id: saved.id)
        let input = HoldingsAnalyzer.MarketInput(price: 110, daily: candles, daySignal: .init(action: .buy, score: 30),
                                                 shortSignal: nil, reliability: .high)
        let analysis = HoldingsAnalyzer.analyze(holdings: stored, cash: 10, market: ["crypto:BTC": input])
        _ = (analysis.total, analysis.cash, analysis.invested, analysis.pnl, analysis.pnlPercent,
             analysis.allocation.crypto, analysis.allocation.stock, analysis.allocation.cash,
             analysis.risk.volatilityAnnual, analysis.risk.var95Day, analysis.risk.var95DayPercent,
             analysis.risk.averageCorrelation, analysis.risk.lossAtStops, analysis.risk.maxWeight, analysis.risk.effectiveAssets)
        for l in analysis.lines {
            _ = (l.id, l.symbol, l.kind, l.name, l.quantity, l.averagePrice, l.price, l.value, l.pnl, l.pnlPercent,
                 l.weight, l.stop, l.lossAtStop, l.recommendation.label, l.reasons, l.trimValue)
            _ = l.reasons.map { HoldingsAnalyzer.reasonText[$0] }
        }
        for i in analysis.insights { _ = (i.level, i.code, HoldingsAnalyzer.text(i)) }
        _ = HoldingsAnalyzer.Recommendation.allCasesForUI

        // Full catalogue (asset picker) and advice.
        let entry = UniverseEntry(symbol: "BTC", name: "Bitcoin", kind: .crypto, rank: 1, flag: 5)
        _ = (entry.id, entry.symbol, entry.name, entry.kind, entry.rank, entry.flag, entry.isETF, entry.asset.symbol)
        _ = AssetUniverse.search([entry], "bit", limit: 10)
        _ = AssetUniverse.cleanStockName("Apple Inc. - Common Stock")
        _ = AssetUniverse.popularETFs
        let loader: (AssetClass, HTTPTransport) async throws -> [UniverseEntry] = { try await AssetUniverse.load($0, transport: $1) }
        _ = loader
        let advice = Advisor.advise(signal: nil, reliability: nil, price: nil, line: nil, capital: nil, risk: RiskSettings())
        _ = (advice.tone, advice.title, advice.points, advice.entry, advice.stop, advice.target, advice.amount)
    }
}
