import Foundation

/// Decision tools that only compute (no advice added): comparison of assets over the same days, position size for a
/// chosen risk, and rebalancing towards a target split. Same computations as the site (web/src/engine/tools.ts).
public enum Tools {
    private static let day = 86_400_000.0
    private static func dayOf(_ t: Double) -> Double { (t / day).rounded(.down) * day }

    // MARK: Comparison

    public struct Stat: Sendable, Identifiable {
        public var id: String
        public var change: Double
        public var volatility: Double
        public var maxDrawdown: Double
        public var pct: [Double]
    }

    public struct Comparison: Sendable {
        public var stats: [Stat]
        public var correlation: [[Double?]]
        public var days: [Double]
        public var missing: [String]
    }

    private static func returns(_ closes: [(Double, Double)]) -> [Double: Double] {
        var out: [Double: Double] = [:]
        for i in closes.indices.dropFirst() { out[dayOf(closes[i].0)] = closes[i].1 / closes[i - 1].1 - 1 }
        return out
    }

    private static func correlation(_ a: [Double: Double], _ b: [Double: Double]) -> Double? {
        let common = a.keys.filter { b[$0] != nil }
        guard common.count >= 20 else { return nil }
        let xs = common.map { a[$0]! }
        let ys = common.map { b[$0]! }
        let mx = xs.reduce(0, +) / Double(xs.count)
        let my = ys.reduce(0, +) / Double(ys.count)
        var sxy = 0.0, sxx = 0.0, syy = 0.0
        for i in xs.indices {
            sxy += (xs[i] - mx) * (ys[i] - my)
            sxx += (xs[i] - mx) * (xs[i] - mx)
            syy += (ys[i] - my) * (ys[i] - my)
        }
        return sxx > 0 && syy > 0 ? sxy / (sxx * syy).squareRoot() : nil
    }

    public static func compare(_ series: [String: [(Double, Double)]], ids: [String], days: Int, now: Date = Date()) -> Comparison? {
        let nowMs = now.timeIntervalSince1970 * 1000
        var clean: [String: [(Double, Double)]] = [:]
        var missing: [String] = []
        for id in ids {
            let c = (series[id] ?? []).filter { $0.1 > 0 && $0.0 <= nowMs }.sorted { $0.0 < $1.0 }
            if c.count < 2 { missing.append(id) } else { clean[id] = c }
        }
        let kept = ids.filter { clean[$0] != nil }
        guard !kept.isEmpty else { return nil }
        let end = dayOf(kept.map { clean[$0]!.last!.0 }.max()!)
        // Common period: from the latest first day among the assets (a younger asset shortens it) to the last close.
        let start = max(end - Double(days) * day, kept.map { dayOf(clean[$0]![0].0) }.max()!)
        guard end - start >= day else { return nil }
        let grid = Array(stride(from: start, through: end, by: day))
        func inPeriod(_ id: String) -> [(Double, Double)] { clean[id]!.filter { dayOf($0.0) >= start && dayOf($0.0) <= end } }
        let stats: [Stat] = kept.map { id in
            let c = inPeriod(id)
            let all = clean[id]!
            var i = 0
            var last = 0.0
            let onGrid: [Double] = grid.map { d in
                while i < all.count && dayOf(all[i].0) <= d { last = all[i].1; i += 1 }
                return last
            }
            var peak = c[0].1
            var dd = 0.0
            for (_, v) in c {
                peak = max(peak, v)
                dd = min(dd, (v / peak - 1) * 100)
            }
            let r = Array(returns(c).values)
            let mean = r.isEmpty ? 0 : r.reduce(0, +) / Double(r.count)
            let variance = r.reduce(0) { $0 + ($1 - mean) * ($1 - mean) } / Double(max(1, r.count - 1))
            let perYear = id.hasPrefix("crypto:") ? 365.0 : 252.0
            let base = onGrid[0]
            return Stat(id: id, change: (onGrid.last! / base - 1) * 100, volatility: variance.squareRoot() * perYear.squareRoot() * 100,
                        maxDrawdown: dd, pct: onGrid.map { ($0 / base - 1) * 100 })
        }
        let rets = kept.map { returns(inPeriod($0)) }
        let corr: [[Double?]] = kept.indices.map { a in kept.indices.map { b in a == b ? 1 : correlation(rets[a], rets[b]) } }
        return Comparison(stats: stats, correlation: corr, days: grid, missing: missing)
    }

    // MARK: Position size

    public struct Position: Sendable {
        public var quantity: Double
        public var amount: Double
        public var capitalShare: Double
        public var risk: Double
        public var stopDistance: Double
        public var reward: Double?
        public var ratio: Double?
        public var capped: Bool
    }

    /// Quantity such that hitting the stop costs `riskPct` % of the capital, never more than the capital itself.
    public static func positionSize(capital: Double, riskPct: Double, entry: Double, stop: Double, target: Double? = nil) -> Position? {
        guard capital > 0, riskPct > 0, entry > 0, stop > 0, stop < entry else { return nil }
        let perUnit = entry - stop
        var quantity = capital * riskPct / 100 / perUnit
        var capped = false
        if quantity * entry > capital {
            quantity = capital / entry
            capped = true
        }
        let risk = quantity * perUnit
        let reward = target.flatMap { $0 > entry ? quantity * ($0 - entry) : nil }
        return Position(quantity: quantity, amount: quantity * entry, capitalShare: quantity * entry / capital * 100, risk: risk,
                        stopDistance: perUnit / entry * 100, reward: reward, ratio: reward.map { $0 / risk }, capped: capped)
    }

    // MARK: Sale after fees and tax

    public struct Sale: Sendable, Identifiable {
        public var id: String
        public var gross: Double
        public var fees: Double
        public var gain: Double?
        public var tax: Double
        public var net: Double
    }

    public struct SaleTotal: Sendable {
        public var gross: Double
        public var fees: Double
        public var gain: Double?
        public var tax: Double
        public var net: Double
        public var lines: [Sale]
        public var unknownCost: Int
    }

    /// What a sale leaves once the fees and the tax on the gain are paid (France: flat tax "PFU" of 30 %). A loss pays
    /// no tax; selling everything, the losses of the year offset the gains. Line by line for cryptos is an estimate
    /// (France computes their gain on the whole portfolio at each sale).
    public static func saleTotal(_ lines: [(id: String, value: Double, cost: Double?)], taxPct: Double = 30, feePct: Double = 0.1) -> SaleTotal {
        let each: [Sale] = lines.map { l in
            let fees = max(0, l.value) * max(0, feePct) / 100
            let gain = l.cost.map { l.value - fees - $0 }
            let tax = (gain ?? 0) > 0 ? gain! * max(0, taxPct) / 100 : 0
            return Sale(id: l.id, gross: l.value, fees: fees, gain: gain, tax: tax, net: l.value - fees - tax)
        }
        let known = each.compactMap(\.gain)
        let gain: Double? = known.isEmpty ? nil : known.reduce(0, +)
        let gross = each.reduce(0) { $0 + $1.gross }
        let fees = each.reduce(0) { $0 + $1.fees }
        let tax = (gain ?? 0) > 0 ? gain! * max(0, taxPct) / 100 : 0
        return SaleTotal(gross: gross, fees: fees, gain: gain, tax: tax, net: gross - fees - tax, lines: each, unknownCost: each.count - known.count)
    }

    // MARK: Projection

    /// Yearly returns shown side by side: hypotheses to compare, not forecasts.
    public static let projectionRates: [Double] = [0, 4, 8]

    public struct YearPoint: Sendable { public var year: Int; public var value: Double; public var paid: Double }

    /// Value after `years` of `start` plus `monthly` at the end of each month, compounded monthly at `ratePct` a year.
    public static func projection(start: Double, monthly: Double, years: Int, ratePct: Double) -> [YearPoint] {
        let r = pow(1 + ratePct / 100, 1.0 / 12) - 1
        var v = max(0, start)
        var paid = v
        var out = [YearPoint(year: 0, value: v, paid: paid)]
        if years > 0 {
            for m in 1...(years * 12) {
                v = v * (1 + r) + max(0, monthly)
                paid += max(0, monthly)
                if m % 12 == 0 { out.append(YearPoint(year: m / 12, value: v, paid: paid)) }
            }
        }
        return out
    }

    // MARK: Rebalancing

    public struct Rebalance: Sendable {
        public var total: Double
        public var current: [Kind: Double]
        public var moves: [Kind: Double]
        public var lines: [(id: String, amount: Double)]
    }

    /// Buys and sells that bring the lines to the target split (the phone keeps no cash: crypto + stocks = 100 %).
    public static func rebalance(_ lines: [(id: String, kind: Kind, value: Double)], targetCrypto: Double) -> Rebalance? {
        guard targetCrypto >= 0, targetCrypto <= 100 else { return nil }
        var by: [Kind: Double] = [.crypto: 0, .stock: 0]
        for l in lines where l.value > 0 { by[l.kind, default: 0] += l.value }
        let total = by.values.reduce(0, +)
        guard total > 0 else { return nil }
        let target: [Kind: Double] = [.crypto: targetCrypto, .stock: 100 - targetCrypto]
        var moves: [Kind: Double] = [:]
        var current: [Kind: Double] = [:]
        for k in [Kind.crypto, .stock] {
            moves[k] = total * target[k]! / 100 - by[k]!
            current[k] = by[k]! / total * 100
        }
        let split = lines.filter { $0.value > 0 && by[$0.kind]! > 0 }.map { (id: $0.id, amount: moves[$0.kind]! * $0.value / by[$0.kind]!) }
        return Rebalance(total: total, current: current, moves: moves, lines: split)
    }
}
