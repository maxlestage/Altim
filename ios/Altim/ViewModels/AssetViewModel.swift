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
}
