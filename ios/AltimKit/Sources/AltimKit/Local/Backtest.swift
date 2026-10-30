import Foundation

/// "Si j'avais suivi Altim": the technical signal replayed day by day on the asset's daily history, computed on the
/// iPhone. No look-ahead: each day the signal only sees the candles known that day, and an order is filled at the next
/// day's open. In position, the stop and the target of the entry signal are checked on every candle (the stop first
/// when both are touched the same day), and a SELL signal closes the position at the next open. 0.1 % fees per order.
public struct BacktestReport: Codable, Sendable {
    public struct Trade: Codable, Sendable, Equatable, Identifiable {
        public var entryTime: Double
        public var entryPrice: Double
        public var exitTime: Double
        public var exitPrice: Double
        /// "stop", "target", "signal" or "open" (still held at the end).
        public var exit: String
        /// Net of fees, in %.
        public var result: Double
        public var id: Double { entryTime }
    }

    public struct Point: Codable, Sendable, Equatable {
        public var time: Double
        /// Value of 100 invested at the start, following the signal / simply held.
        public var strategy: Double
        public var hold: Double
    }

    public var symbol: String
    public var kind: Kind
    public var from: Double
    public var to: Double
    public var days: Int
    /// Total return in %, following the signal and holding the asset.
    public var strategyReturn: Double
    public var holdReturn: Double
    /// Worst fall from a previous top, in % (negative).
    public var strategyDrawdown: Double
    public var holdDrawdown: Double
    public var trades: [Trade]
    public var winRate: Double?
    /// Share of the days spent invested, in %.
    public var exposure: Double
    public var curve: [Point]
    public var reliability: Reliability?
    public var error: String?

    public var beatsHolding: Bool { strategyReturn > holdReturn }
    /// The signal lost less in its worst moment than the asset held.
    public var safer: Bool { strategyDrawdown > holdDrawdown }

    public var verdict: String {
        if error != nil { return "Historique insuffisant" }
        if trades.isEmpty { return "Aucun achat sur la période" }
        switch (beatsHolding, safer) {
        case (true, true): return "Meilleur et plus prudent que garder"
        case (true, false): return "Meilleur que garder, plus agité"
        case (false, true): return "Moins de gain, mais beaucoup moins de risque"
        case (false, false): return "Garder aurait mieux marché"
        }
    }

    public var tone: Tone {
        if error != nil || trades.isEmpty { return .neutral }
        return beatsHolding ? .good : safer ? .warn : .bad
    }
}

public enum Backtest {
    static let fee = 0.001
    /// Candles the signal needs before its first day, and the window it sees each day (as the live signal: ≤ 300).
    static let warmup = 200
    static let window = 300

    public static func run(_ raw: [Bar], asset: Asset, reliability: Reliability? = nil) -> BacktestReport {
        let c = Indicators.sanitize(raw)
        func empty(_ e: String) -> BacktestReport {
            BacktestReport(symbol: asset.symbol, kind: asset.kind, from: c.first?.time ?? 0, to: c.last?.time ?? 0, days: c.count,
                           strategyReturn: 0, holdReturn: 0, strategyDrawdown: 0, holdDrawdown: 0, trades: [], winRate: nil,
                           exposure: 0, curve: [], reliability: reliability, error: e)
        }
        // A fixed start (not a share of the history): adding days never changes the past trades.
        let start = c.count >= warmup + 30 ? warmup : 60
        guard c.count >= start + 30 else { return empty("Il faut au moins \(start + 30) jours d'historique (\(c.count) disponibles).") }

        var cash = 100.0
        var units = 0.0
        var entry: (time: Double, price: Double, stop: Double, target: Double, cost: Double)?
        var pendingBuy: LocalSignal?
        var pendingSell = false
        var trades: [BacktestReport.Trade] = []
        var curve: [BacktestReport.Point] = []
        var invested = 0
        let holdUnits = 100 / c[start].open
        var peakS = 100.0, peakH = 100.0, ddS = 0.0, ddH = 0.0

        func close(at price: Double, time: Double, reason: String) {
            guard let e = entry else { return }
            cash = units * price * (1 - fee)
            trades.append(.init(entryTime: e.time, entryPrice: e.price, exitTime: time, exitPrice: price, exit: reason,
                                result: (cash / e.cost - 1) * 100))
            units = 0
            entry = nil
        }

        for i in start..<c.count {
            let bar = c[i]
            // Orders decided yesterday, filled at today's open.
            if pendingSell, entry != nil { close(at: bar.open, time: bar.time, reason: "signal") }
            if let s = pendingBuy, entry == nil {
                let cost = cash
                units = cash * (1 - fee) / bar.open
                cash = 0
                // The plan keeps its distances from the signal's price, applied to the fill price.
                let ratioStop = s.hasPlan ? s.stopLoss / s.price : 0.9
                let ratioTarget = s.hasPlan ? s.takeProfit / s.price : 1.2
                entry = (bar.time, bar.open, bar.open * ratioStop, bar.open * ratioTarget, cost)
            }
            pendingBuy = nil
            pendingSell = false
            // Stop, then target, during the day.
            if let e = entry {
                if bar.low <= e.stop { close(at: min(e.stop, bar.open), time: bar.time, reason: "stop") }
                else if bar.high >= e.target { close(at: max(e.target, bar.open), time: bar.time, reason: "target") }
            }
            if entry != nil { invested += 1 }
            // Tonight's signal, on the candles known tonight.
            if i < c.count - 1, let s = SignalEngine.analyze(Array(c[max(0, i + 1 - window)...i]), intervalMs: nil, now: bar.time) {
                if entry == nil && s.action.isBuy { pendingBuy = s }
                if entry != nil && s.action.isSell { pendingSell = true }
            }
            let valueS = cash + units * bar.close
            let valueH = holdUnits * bar.close
            peakS = max(peakS, valueS)
            peakH = max(peakH, valueH)
            ddS = min(ddS, (valueS / peakS - 1) * 100)
            ddH = min(ddH, (valueH / peakH - 1) * 100)
            curve.append(.init(time: bar.time, strategy: valueS, hold: valueH))
        }
        if let e = entry, let last = c.last {
            let value = units * last.close * (1 - fee)
            trades.append(.init(entryTime: e.time, entryPrice: e.price, exitTime: last.time, exitPrice: last.close, exit: "open",
                                result: (value / e.cost - 1) * 100))
        }
        let finalS = curve.last?.strategy ?? 100
        let finalH = curve.last?.hold ?? 100
        let wins = trades.filter { $0.result > 0 }.count
        return BacktestReport(symbol: asset.symbol, kind: asset.kind, from: c[start].time, to: c[c.count - 1].time, days: c.count - start,
                              strategyReturn: finalS - 100, holdReturn: finalH - 100, strategyDrawdown: ddS, holdDrawdown: ddH,
                              trades: trades, winRate: trades.isEmpty ? nil : Double(wins) / Double(trades.count) * 100,
                              exposure: Double(invested) / Double(c.count - start) * 100, curve: sample(curve, 90),
                              reliability: reliability, error: nil)
    }

    /// At most `n` points, the last one always kept.
    static func sample(_ p: [BacktestReport.Point], _ n: Int) -> [BacktestReport.Point] {
        guard p.count > n, n > 1 else { return p }
        let step = Double(p.count - 1) / Double(n - 1)
        return (0..<n).map { p[Int((Double($0) * step).rounded())] }
    }
}

extension AltimClient {
    /// The signal replayed on the asset's daily history, always computed on the iPhone.
    public func backtest(_ a: Asset) async throws -> BacktestReport {
        let (data, status) = try await getWithStatus("/api/backtest", ["symbol": a.symbol, "kind": a.kind.rawValue])
        return try decodeResponse(BacktestReport.self, data, status)
    }
}
