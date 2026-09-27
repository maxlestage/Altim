import Foundation
import Observation
import AltimCore

/// "Mes avoirs": holdings stored in SQLite + full analysis (prices, signals, risks).
@MainActor
@Observable
final class HoldingsViewModel {
    private(set) var holdings: [Holding] = []
    private(set) var cash: Double = 0
    private(set) var analysis: HoldingsAnalyzer.Analysis?
    private(set) var isLoading = false
    private(set) var error: String?
    @ObservationIgnored private var market: [String: HoldingsAnalyzer.MarketInput] = [:]

    func load(services: AppServices) async {
        guard let db = services.holdingsDB else {
            error = services.holdingsDBError ?? "Base de données indisponible."
            return
        }
        do {
            holdings = try await db.all()
            cash = try await db.cash()
            recompute()
        } catch {
            self.error = error.localizedDescription
        }
        await refreshMarket(services: services)
    }

    /// Prices, daily candles and signals (1 d and 4 h) from the multi-source consensus.
    func refreshMarket(services: AppServices) async {
        let assets = Dictionary(holdings.map { ($0.marketKey, $0) }, uniquingKeysWith: { a, _ in a })
        guard !assets.isEmpty else { market = [:]; recompute(); return }
        isLoading = true
        defer { isLoading = false }
        let consensus = services.market
        let results = await withTaskGroup(of: (String, HoldingsAnalyzer.MarketInput?).self) { group in
            for (key, h) in assets {
                group.addTask {
                    (key, await Self.marketInput(for: h.asset, market: consensus))
                }
            }
            var out: [String: HoldingsAnalyzer.MarketInput] = [:]
            for await (k, v) in group { if let v { out[k] = v } }
            return out
        }
        market = results
        error = results.count < assets.count ? "Certains cours sont indisponibles : analyse partielle." : nil
        recompute()
    }

    /// Full market input for a held asset: daily price and candles, 1 d and 4 h signals.
    nonisolated static func marketInput(for asset: Asset, market: ConsensusMarketData) async -> HoldingsAnalyzer.MarketInput? {
        guard let day = try? await MarketAnalysis.run(asset: asset, timeframe: .d1, market: market) else { return nil }
        let short = try? await MarketAnalysis.run(asset: asset, timeframe: .h4, market: market)
        // The least reliable of the two timeframes wins (caution).
        let levels = [day.snapshot.reliability, short?.snapshot.reliability].compactMap { $0 }
        let reliability: ReliabilityLevel = levels.contains(.low) ? .low : levels.contains(.medium) ? .medium : .high
        return HoldingsAnalyzer.MarketInput(
            price: day.price,
            daily: day.snapshot.candles,
            daySignal: .init(action: day.signal.action, score: day.signal.score),
            shortSignal: short.map { .init(action: $0.signal.action, score: $0.signal.score) },
            reliability: reliability)
    }

    func save(_ holding: Holding, merge: Bool, services: AppServices) async {
        guard let db = services.holdingsDB else { return }
        do {
            _ = try await db.upsert(holding, merge: merge)
            holdings = try await db.all()
            recompute()
            await refreshMarket(services: services)
        } catch {
            self.error = error.localizedDescription
        }
    }

    func delete(_ id: String, services: AppServices) async {
        guard let db = services.holdingsDB else { return }
        do {
            try await db.delete(id: id)
            holdings = try await db.all()
            recompute()
        } catch {
            self.error = error.localizedDescription
        }
    }

    func setCash(_ value: Double, services: AppServices) async {
        guard let db = services.holdingsDB else { return }
        do {
            try await db.setCash(value)
            cash = value
            recompute()
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// JSON file compatible with the Altim web app (export / import).
    func exportFile(services: AppServices) async -> URL? {
        guard let db = services.holdingsDB, let data = try? await db.exportJSON() else { return nil }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("altim-avoirs.json")
        return (try? data.write(to: url, options: .atomic)) != nil ? url : nil
    }

    func importFile(_ url: URL, services: AppServices) async {
        guard let db = services.holdingsDB else { return }
        let access = url.startAccessingSecurityScopedResource()
        defer { if access { url.stopAccessingSecurityScopedResource() } }
        do {
            try await db.importJSON(Data(contentsOf: url))
            await load(services: services)
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func recompute() {
        analysis = HoldingsAnalyzer.analyze(holdings: holdings, cash: cash, market: market)
    }
}

extension Holding {
    /// Market asset used to price this holding (crypto quoted in USDT, stocks in USD).
    var asset: Asset {
        Asset(symbol: kind == .crypto ? "\(symbol)USDT" : symbol, name: name, assetClass: kind,
              quote: kind == .crypto ? "USDT" : "USD")
    }

    var marketKey: String { "\(kind.rawValue):\(symbol)" }
}
