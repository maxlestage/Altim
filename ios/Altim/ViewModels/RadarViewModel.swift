import Foundation
import Observation
import AltimCore

/// Suivi de la liste : cours consolidé + signal validé de chaque actif, rafraîchis périodiquement.
@MainActor
@Observable
final class RadarViewModel {
    struct Row {
        var price: Double?
        var change24h: Double = 0
        var signal: Signal?
        var reliability: ReliabilityLevel?
        var sourcesSummary: String?
        var sparkline: [Double] = []
        var error: String?
    }

    private(set) var rows: [String: Row] = [:]
    private(set) var isRefreshing = false
    private(set) var lastUpdate: Date?
    private(set) var fearGreed: FearGreedIndex?
    @ObservationIgnored private var lastActions: [String: SignalAction] = [:]

    func refresh(assets: [Asset], timeframe: Timeframe, services: AppServices, notify: Bool) async {
        guard !isRefreshing else { return }
        isRefreshing = true
        defer { isRefreshing = false }
        let market = services.market
        let sentiment = services.sentiment

        async let fg = try? await sentiment.cryptoFearGreed()
        let results = await concurrentMap(assets) { asset -> (Asset, Row) in
            var row = Row()
            do {
                let a = try await MarketAnalysis.run(asset: asset, timeframe: timeframe, market: market, limit: 400)
                row.price = a.price
                row.change24h = a.change24h
                row.signal = a.signal
                row.reliability = a.snapshot.reliability
                row.sourcesSummary = a.snapshot.summary
                row.sparkline = a.snapshot.candles.suffix(48).map(\.close)
            } catch {
                row.error = error.localizedDescription
            }
            return (asset, row)
        }
        if let value = await fg { fearGreed = value }

        for (asset, row) in results {
            // An error keeps the last good data rather than blanking the display.
            if row.error != nil, var previous = rows[asset.id], previous.signal != nil {
                previous.error = row.error
                rows[asset.id] = previous
                continue
            }
            rows[asset.id] = row
            guard let signal = row.signal else { continue }
            let previous = lastActions[asset.id]
            lastActions[asset.id] = signal.action
            if notify, let previous, previous != signal.action, signal.action != .hold {
                services.notify(asset: asset, signal: signal)
            }
        }
        lastUpdate = Date()
    }
}
