import Foundation

/// Backtest "long uniquement" (spot) du moteur de signaux, sans biais d'anticipation :
/// le signal est calculé sur la bougie clôturée `i` et exécuté à l'ouverture de `i + 1`.
public struct Backtester: Sendable {

    public struct Trade: Hashable, Sendable, Identifiable {
        public let entryTime: Date
        public let exitTime: Date
        public let entryPrice: Double
        public let exitPrice: Double
        /// Rendement net de frais.
        public let returnPercent: Double
        public let exitReason: String

        public var id: Date { entryTime }
    }

    public struct EquityPoint: Hashable, Sendable {
        public let time: Date
        public let equity: Double
    }

    public struct Result: Sendable {
        public let trades: [Trade]
        public let equityCurve: [EquityPoint]
        public let totalReturnPercent: Double
        public let buyAndHoldPercent: Double
        public let winRatePercent: Double
        public let maxDrawdownPercent: Double
        public let profitFactor: Double
        public let exposurePercent: Double

        /// La stratégie bat-elle le simple achat-conservation, avec un risque raisonnable ?
        public var beatsBuyAndHold: Bool { totalReturnPercent > buyAndHoldPercent }
    }

    public var engine: SignalEngine
    public var feeRate: Double
    /// Nombre de bougies d'historique passées au moteur à chaque pas.
    public var lookback: Int

    public init(engine: SignalEngine = SignalEngine(), feeRate: Double = 0.001, lookback: Int = 250) {
        self.engine = engine
        self.feeRate = feeRate
        self.lookback = lookback
    }

    public func run(_ rawCandles: [Candle]) -> Result {
        let candles = rawCandles.sanitized()
        let warmup = engine.configuration.minimumCandles
        var equity = 1.0
        var curve: [EquityPoint] = []
        var trades: [Trade] = []
        var barsInMarket = 0

        struct Position { let entryTime: Date; let entry: Double; let stop: Double; let target: Double; let units: Double }
        var position: Position?
        var pendingEntry: TradePlan?
        var pendingExit = false

        func close(at price: Double, time: Date, reason: String) {
            guard let p = position else { return }
            let proceeds = p.units * price * (1 - feeRate)
            let cost = p.units * p.entry
            equity = proceeds
            trades.append(Trade(entryTime: p.entryTime, exitTime: time, entryPrice: p.entry, exitPrice: price,
                                returnPercent: (proceeds / (cost / (1 - feeRate)) - 1) * 100, exitReason: reason))
            position = nil
        }

        guard candles.count > warmup + 1 else {
            return Result(trades: [], equityCurve: [], totalReturnPercent: 0, buyAndHoldPercent: 0,
                          winRatePercent: 0, maxDrawdownPercent: 0, profitFactor: 0, exposurePercent: 0)
        }

        for i in warmup..<candles.count {
            let bar = candles[i]

            // 1. Exécutions décidées à la clôture précédente, au prix d'ouverture.
            if pendingExit {
                close(at: bar.open, time: bar.time, reason: "Signal de vente")
                pendingExit = false
            }
            if let plan = pendingEntry, position == nil {
                let entry = bar.open
                let distance = plan.entry - plan.stopLoss
                let units = equity * (1 - feeRate) / entry
                position = Position(entryTime: bar.time, entry: entry, stop: entry - distance,
                                    target: entry + distance * engine.configuration.rewardRisk, units: units)
                pendingEntry = nil
            }

            // 2. Stop / objectif pendant la bougie (hypothèse prudente : le stop est touché en premier).
            if let p = position {
                barsInMarket += 1
                if bar.low <= p.stop {
                    close(at: min(bar.open, p.stop), time: bar.time, reason: "Stop")
                } else if bar.high >= p.target {
                    close(at: max(bar.open, p.target), time: bar.time, reason: "Objectif")
                }
            }

            let markToMarket = position.map { $0.units * bar.close } ?? equity
            curve.append(EquityPoint(time: bar.time, equity: markToMarket))

            // 3. Signal à la clôture, exécuté à la bougie suivante.
            guard i < candles.count - 1 else { break }
            let window = Array(candles[max(0, i - lookback + 1)...i])
            guard let signal = try? engine.analyze(window) else { continue }
            if position == nil, signal.action.isBuy, let plan = signal.plan {
                pendingEntry = plan
            } else if position != nil, signal.action.isSell {
                pendingExit = true
            }
        }

        if let last = candles.last, position != nil {
            close(at: last.close, time: last.time, reason: "Fin du test")
            if !curve.isEmpty { curve[curve.count - 1] = EquityPoint(time: last.time, equity: equity) }
        }

        var peak = 0.0, maxDD = 0.0
        for point in curve {
            peak = max(peak, point.equity)
            if peak > 0 { maxDD = max(maxDD, (peak - point.equity) / peak) }
        }
        let wins = trades.filter { $0.returnPercent > 0 }
        let grossWin = wins.reduce(0) { $0 + $1.returnPercent }
        let grossLoss = trades.filter { $0.returnPercent <= 0 }.reduce(0) { $0 - $1.returnPercent }
        let first = candles[warmup].open
        let tested = candles.count - warmup

        return Result(
            trades: trades,
            equityCurve: curve,
            totalReturnPercent: (equity - 1) * 100,
            buyAndHoldPercent: (candles[candles.count - 1].close / first - 1) * 100,
            winRatePercent: trades.isEmpty ? 0 : Double(wins.count) / Double(trades.count) * 100,
            maxDrawdownPercent: maxDD * 100,
            profitFactor: grossLoss > 0 ? grossWin / grossLoss : (grossWin > 0 ? .infinity : 0),
            exposurePercent: tested > 0 ? Double(barsInMarket) / Double(tested) * 100 : 0
        )
    }
}
