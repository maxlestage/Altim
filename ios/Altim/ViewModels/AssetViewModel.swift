import Foundation
import Observation
import AltimCore

/// Analyse détaillée d'un actif : graphique, signal, backtest.
@MainActor
@Observable
final class AssetViewModel {
    let asset: Asset
    var timeframe: Timeframe
    private(set) var candles: [Candle] = []
    private(set) var signal: Signal?
    private(set) var backtest: Backtester.Result?
    private(set) var quote: Quote?
    private(set) var isLoading = false
    private(set) var error: String?

    init(asset: Asset, timeframe: Timeframe) {
        self.asset = asset
        self.timeframe = timeframe
    }

    func load(services: AppServices) async {
        isLoading = true
        error = nil
        defer { isLoading = false }
        let market = services.market
        let asset = asset
        let timeframe = timeframe
        do {
            async let candlesTask = market.candles(for: asset, timeframe: timeframe, limit: 500)
            async let quoteTask = market.quote(for: asset)
            let candles = try await candlesTask
            quote = try? await quoteTask
            var higher: [Candle]?
            if let h = timeframe.higher { higher = try? await market.candles(for: asset, timeframe: h, limit: 300) }

            // Calculs lourds hors du thread principal.
            let (signal, backtest) = try await Task.detached(priority: .userInitiated) {
                let signal = try SignalEngine().analyze(candles, higherTimeframe: higher, timeframe: timeframe)
                let backtest = Backtester().run(candles)
                return (signal, backtest)
            }.value
            self.candles = candles.sanitized()
            self.signal = signal
            self.backtest = backtest
        } catch {
            self.error = error.localizedDescription
        }
    }
}
