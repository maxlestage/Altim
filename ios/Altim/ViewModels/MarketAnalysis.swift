import Foundation
import AltimCore

/// Complete, validated analysis of an asset: multi-source data → signal → reliability gate.
struct MarketAnalysis: Sendable {
    let snapshot: MarketSnapshot
    let signal: Signal
    let price: Double
    let change24h: Double

    static func run(asset: Asset, timeframe: Timeframe, market: ConsensusMarketData, limit: Int = 500) async throws -> MarketAnalysis {
        async let mainTask = market.snapshot(for: asset, timeframe: timeframe, limit: limit)
        let snapshot = try await mainTask
        var higher: [Candle]?
        if let h = timeframe.higher {
            higher = try? await market.snapshot(for: asset, timeframe: h, limit: 300).candles
        }
        let raw = try SignalEngine().analyze(snapshot.candles, higherTimeframe: higher, timeframe: timeframe)
        let signal = ReliabilityGate.apply(raw, snapshot: snapshot)
        let price = snapshot.consensusPrice ?? snapshot.candles.last?.close ?? raw.price
        return MarketAnalysis(snapshot: snapshot, signal: signal, price: price,
                              change24h: change(snapshot.candles, price: price))
    }

    /// Change over 24 h computed from the candles (no extra request).
    static func change(_ candles: [Candle], price: Double) -> Double {
        guard let last = candles.last else { return 0 }
        let target = last.time.addingTimeInterval(-86_400)
        let ref = candles.last { $0.time <= target }?.close ?? candles.first!.close
        return ref > 0 ? (price / ref - 1) * 100 : 0
    }
}

/// Runs `work` on every item, at most `limit` at a time: a long radar or portfolio
/// must not flood the exchanges (rate limits). Results keep the order of `items`.
func concurrentMap<T: Sendable, R: Sendable>(_ items: [T], limit: Int = 6, _ work: @escaping @Sendable (T) async -> R) async -> [R] {
    await withTaskGroup(of: (Int, R).self) { group in
        var results = [R?](repeating: nil, count: items.count)
        var next = 0
        while next < min(limit, items.count) {
            let i = next
            group.addTask { (i, await work(items[i])) }
            next += 1
        }
        while let (i, r) = await group.next() {
            results[i] = r
            if next < items.count {
                let j = next
                group.addTask { (j, await work(items[j])) }
                next += 1
            }
        }
        return results.compactMap { $0 }
    }
}
