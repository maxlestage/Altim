import Foundation
import Observation
import AltimCore

/// Detailed analysis of an asset: multi-source data, signal, backtest, sentiment.
@MainActor
@Observable
final class AssetViewModel {
    let asset: Asset
    var timeframe: Timeframe
    private(set) var analysis: MarketAnalysis?
    private(set) var backtest: Backtester.Result?
    private(set) var fearGreed: FearGreedIndex?
    private(set) var social: SocialSentiment?
    private(set) var isLoading = false
    private(set) var error: String?
    /// Altim's advice, adapted to the user's holdings (SQLite).
    private(set) var advice: Advisor.Advice?
    /// Held line of this asset, if any.
    private(set) var heldLine: HoldingsAnalyzer.Line?

    var candles: [Candle] { analysis?.snapshot.candles ?? [] }
    var signal: Signal? { analysis?.signal }
    var snapshot: MarketSnapshot? { analysis?.snapshot }

    init(asset: Asset, timeframe: Timeframe) {
        self.asset = asset
        self.timeframe = timeframe
    }

    func load(services: AppServices) async {
        isLoading = true
        error = nil
        defer { isLoading = false }
        let market = services.market
        let sentiment = services.sentiment
        let asset = asset
        let timeframe = timeframe
        let isCrypto = asset.assetClass == .crypto
        async let socialTask = try? await sentiment.social(asset)
        async let fgTask: FearGreedIndex? = isCrypto ? (try? await sentiment.cryptoFearGreed()) : nil
        do {
            let analysis = try await MarketAnalysis.run(asset: asset, timeframe: timeframe, market: market)
            // Heavy computation off the main thread.
            let candles = analysis.snapshot.candles
            let backtest = await Task.detached(priority: .userInitiated) { Backtester().run(candles) }.value
            self.analysis = analysis
            self.backtest = backtest
        } catch {
            self.error = error.localizedDescription
        }
        social = await socialTask
        fearGreed = await fgTask
    }

    /// Advice based on the market and on what the user already owns.
    /// Every holding is valued at the consensus price so that weights match "Mes avoirs".
    func loadAdvice(services: AppServices, risk: RiskSettings) async {
        let holdings = (try? await services.holdingsDB?.all()) ?? []
        let cash = (try? await services.holdingsDB?.cash()) ?? 0
        let consensus = services.market
        let asset = asset
        let held = holdings.first { $0.kind == asset.assetClass && $0.asset.symbol == asset.symbol }
        let others = Dictionary(holdings.filter { $0.id != held?.id }.map { ($0.marketKey, $0) }, uniquingKeysWith: { a, _ in a })
        let quoted = await concurrentMap(Array(others)) { key, h -> (String, HoldingsAnalyzer.MarketInput?) in
            guard let q = try? await consensus.quote(for: h.asset) else { return (key, nil) }
            return (key, HoldingsAnalyzer.MarketInput(price: q.price, daily: [], daySignal: nil, shortSignal: nil, reliability: nil))
        }
        var market: [String: HoldingsAnalyzer.MarketInput] = [:]
        for (k, v) in quoted { if let v { market[k] = v } }
        var heldInput: HoldingsAnalyzer.MarketInput?
        if let held {
            heldInput = await HoldingsViewModel.marketInput(for: held.asset, market: consensus)
            if let heldInput { market[held.marketKey] = heldInput }
        }
        let portfolio = HoldingsAnalyzer.analyze(holdings: holdings, cash: cash, market: market)
        let heldPortfolioLine: HoldingsAnalyzer.Line? = held.flatMap { h in portfolio.lines.first { $0.id == h.id } }
        heldLine = heldPortfolioLine
        // Without market data for the held line, fall back on the market-only advice.
        let line: HoldingsAnalyzer.Line? = heldInput == nil ? nil : heldPortfolioLine
        guard let analysis else { advice = nil; return }
        advice = Advisor.advise(signal: analysis.signal, reliability: analysis.snapshot.reliability,
                                price: analysis.price, line: line,
                                capital: portfolio.total > 0 ? portfolio.total : nil, risk: risk,
                                track: backtest.map(Advisor.TrackRecord.init), symbol: asset.base, kind: asset.assetClass)
    }
}
