import Foundation
import Observation
import AltimCore

/// Suivi de la liste : cours + signal de chaque actif, rafraîchis périodiquement.
@MainActor
@Observable
final class RadarViewModel {
    struct Row {
        var quote: Quote?
        var signal: Signal?
        var sparkline: [Double] = []
        var error: String?
    }

    private(set) var rows: [String: Row] = [:]
    private(set) var isRefreshing = false
    private(set) var lastUpdate: Date?
    @ObservationIgnored private var lastActions: [String: SignalAction] = [:]

    func refresh(assets: [Asset], timeframe: Timeframe, services: AppServices, notify: Bool) async {
        guard !isRefreshing else { return }
        isRefreshing = true
        defer { isRefreshing = false }
        let market = services.market

        let results = await withTaskGroup(of: (Asset, Row).self) { group in
            for asset in assets {
                group.addTask {
                    var row = Row()
                    do {
                        async let quote = market.quote(for: asset)
                        async let candles = market.candles(for: asset, timeframe: timeframe, limit: 400)
                        let (q, c) = try await (quote, candles)
                        row.quote = q
                        row.sparkline = c.suffix(48).map(\.close)
                        var higher: [Candle]?
                        if let h = timeframe.higher {
                            higher = try? await market.candles(for: asset, timeframe: h, limit: 300)
                        }
                        row.signal = try SignalEngine().analyze(c, higherTimeframe: higher, timeframe: timeframe)
                    } catch {
                        row.error = error.localizedDescription
                    }
                    return (asset, row)
                }
            }
            var collected: [(Asset, Row)] = []
            for await item in group { collected.append(item) }
            return collected
        }

        for (asset, row) in results {
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
