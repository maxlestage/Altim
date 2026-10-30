import Foundation
@testable import AltimKit

/// Precomputed daily series of one asset for the lab rules.
struct LabSeries {
    let c: [Bar]
    let close: [Double]
    let ema20: [Double?], ema50: [Double?], ema100: [Double?], ema200: [Double?]
    let sma50: [Double?], sma200: [Double?]
    let atr14: [Double?], rsi14: [Double?], rsi2: [Double?], adx14: [Double?]
    init(_ bars: [Bar]) {
        c = Indicators.sanitize(bars)
        close = c.map(\.close)
        ema20 = Indicators.ema(close, 20); ema50 = Indicators.ema(close, 50); ema100 = Indicators.ema(close, 100); ema200 = Indicators.ema(close, 200)
        sma50 = Indicators.sma(close, 50); sma200 = Indicators.sma(close, 200)
        atr14 = Indicators.atr(c, 14); rsi14 = Indicators.rsi(close, 14); rsi2 = Indicators.rsi(close, 2); adx14 = Indicators.adx(c, 14)
    }
    func highest(_ i: Int, _ n: Int) -> Double { c[max(0, i - n + 1)...i].map(\.high).max()! }
    func lowest(_ i: Int, _ n: Int) -> Double { c[max(0, i - n + 1)...i].map(\.low).min()! }
    func ret(_ i: Int, _ n: Int) -> Double? { i >= n ? close[i] / close[i - n] - 1 : nil }
}

/// A rule: tonight, flat → buy tomorrow? In position → sell tomorrow? Plus an optional trailing stop (ATR multiple).
struct LabRule {
    var name: String
    var enter: (LabSeries, Int) -> Bool
    var exit: (LabSeries, Int) -> Bool
    var trail: Double? = nil
}

struct LabResult { var ret: Double; var hold: Double; var dd: Double; var holdDD: Double; var trades: Int; var wins: Int; var expo: Double }

enum Lab {
    static let fee = 0.001

    static func run(_ s: LabSeries, _ rule: LabRule, from: Int, to: Int) -> LabResult {
        var cash = 100.0, units = 0.0, entryCost = 0.0
        var pendingBuy = false, pendingSell = false
        var stop = 0.0
        var trades = 0, wins = 0, invested = 0
        let holdUnits = 100 / s.c[from].open
        var peak = 100.0, peakH = 100.0, dd = 0.0, ddH = 0.0
        func sell(_ p: Double) {
            cash = units * p * (1 - fee)
            trades += 1
            if cash > entryCost { wins += 1 }
            units = 0
        }
        for i in from..<to {
            let b = s.c[i]
            if pendingSell && units > 0 { sell(b.open) }
            if pendingBuy && units == 0 {
                entryCost = cash
                units = cash * (1 - fee) / b.open
                cash = 0
                stop = rule.trail.map { b.open - $0 * (s.atr14[i - 1] ?? 0) } ?? 0
            }
            pendingBuy = false; pendingSell = false
            if units > 0, rule.trail != nil, b.low <= stop { sell(min(stop, b.open)) }
            if units > 0 { invested += 1 }
            if units > 0, let m = rule.trail, let a = s.atr14[i] { stop = max(stop, b.close - m * a) }
            if i < to - 1 {
                if units == 0 && rule.enter(s, i) { pendingBuy = true }
                if units > 0 && rule.exit(s, i) { pendingSell = true }
            }
            let v = cash + units * b.close, vh = holdUnits * b.close
            peak = max(peak, v); peakH = max(peakH, vh)
            dd = min(dd, v / peak - 1); ddH = min(ddH, vh / peakH - 1)
        }
        let final = cash + units * s.c[to - 1].close * (1 - (units > 0 ? fee : 0))
        if units > 0 { trades += 1; if final > entryCost { wins += 1 } }
        return LabResult(ret: final - 100, hold: holdUnits * s.c[to - 1].close - 100, dd: dd * 100, holdDD: ddH * 100, trades: trades, wins: wins,
                         expo: Double(invested) / Double(to - from) * 100)
    }

    static func median(_ v: [Double]) -> Double { Indicators.median(v) }

    /// One line per rule: on each period, the median excess return over holding, the share of assets where the rule
    /// did better than holding in return / in risk-adjusted terms, the median worst fall vs holding.
    static func report(_ data: [(Asset, LabSeries)], _ rules: [LabRule], kinds: [Kind] = [.crypto, .stock]) -> String {
        var lines: [String] = []
        for rule in rules {
            var parts: [String] = []
            for kind in kinds {
                for half in 0..<2 {
                    var ex: [Double] = [], beat = 0, calmar = 0, dds: [Double] = [], hdds: [Double] = [], rets: [Double] = [], n = 0
                    for (a, s) in data where a.kind == kind {
                        let start = 200, len = s.c.count - start
                        guard len > 200 else { continue }
                        let from = start + half * len / 2, to = start + (half + 1) * len / 2
                        let r = run(s, rule, from: from, to: to)
                        n += 1
                        ex.append(r.ret - r.hold); rets.append(r.ret)
                        if r.ret > r.hold { beat += 1 }
                        // Return per unit of worst fall (holding's fall 1 % at least).
                        let cs = (1 + r.ret / 100) / max(0.01, -r.dd / 100 + 0.05), ch = (1 + r.hold / 100) / max(0.01, -r.holdDD / 100 + 0.05)
                        if cs > ch { calmar += 1 }
                        dds.append(r.dd); hdds.append(r.holdDD)
                    }
                    parts.append("\(kind == .crypto ? "C" : "A")\(half + 1): ret \(Int(median(rets)))% exc \(Int(median(ex))) bat \(beat)/\(n) risq \(calmar)/\(n) dd \(Int(median(dds)))/\(Int(median(hdds)))")
                }
            }
            lines.append(rule.name.padding(toLength: 22, withPad: " ", startingAt: 0) + " | " + parts.joined(separator: " | "))
        }
        return lines.joined(separator: "\n")
    }

    /// Whole period per rule: mean and median return (rule vs holding), median worst fall, and the "regret":
    /// mean of the worst outcomes (the 25 % worst assets) — what a careful advisor must keep small.
    static func full(_ data: [(Asset, LabSeries)], _ rules: [LabRule]) -> String {
        rules.map { rule in
            var rs: [Double] = [], hs: [Double] = [], dds: [Double] = [], hdds: [Double] = []
            for (_, s) in data where s.c.count > 400 {
                let r = run(s, rule, from: 200, to: s.c.count)
                rs.append(r.ret); hs.append(r.hold); dds.append(r.dd); hdds.append(r.holdDD)
            }
            func mean(_ v: [Double]) -> Double { v.reduce(0, +) / Double(v.count) }
            let worst = rs.sorted().prefix(max(1, rs.count / 4)), worstH = hs.sorted().prefix(max(1, hs.count / 4))
            return rule.name.padding(toLength: 22, withPad: " ", startingAt: 0)
                + " | moy \(Int(mean(rs)))% (garder \(Int(mean(hs)))%) méd \(Int(median(rs)))% (\(Int(median(hs)))%) | pires 25% \(Int(mean(Array(worst))))% (\(Int(mean(Array(worstH))))%) | creux méd \(Int(median(dds)))% (\(Int(median(hdds)))%)"
        }.joined(separator: "\n")
    }
}

/// Portfolio of one market: every `every` days, the `top` assets with the best `lookback`-day return among those whose
/// close is above their EMA 200 (none → cash), equal weights; against all the assets held with equal weights.
enum PortfolioLab {
    struct Out { var ret: Double; var bench: Double; var dd: Double; var benchDD: Double; var expo: Double }

    static func run(_ data: [LabSeries], top: Int, lookback: Int, every: Int, trend: Bool, from: Double, to: Double) -> Out {
        let dates = Array(Set(data.flatMap { $0.c.map(\.time) })).filter { $0 >= from && $0 < to }.sorted()
        let index = data.map { s in Dictionary(uniqueKeysWithValues: s.c.enumerated().map { ($0.element.time, $0.offset) }) }
        var value = 100.0, bench = 100.0, peak = 100.0, peakB = 100.0, dd = 0.0, ddB = 0.0
        var held: [Int] = [], invested = 0
        var lastIdx = data.map { _ in -1 }
        for (k, t) in dates.enumerated() {
            // Daily returns of each asset that has today's candle (close to close).
            var rets: [Int: Double] = [:]
            for (a, s) in data.enumerated() {
                guard let i = index[a][t] else { continue }
                if lastIdx[a] >= 0 { rets[a] = s.close[i] / s.close[lastIdx[a]] - 1 }
                lastIdx[a] = i
            }
            let live = data.indices.filter { index[$0][t] != nil || lastIdx[$0] >= 0 }
            if !held.isEmpty { value *= 1 + held.map { rets[$0] ?? 0 }.reduce(0, +) / Double(held.count); invested += 1 }
            bench *= 1 + live.map { rets[$0] ?? 0 }.reduce(0, +) / Double(max(1, live.count))
            if k % every == 0 {
                let candidates = data.indices.compactMap { a -> (Int, Double)? in
                    let i = lastIdx[a]
                    guard i >= lookback, let e = data[a].ema200[i] else { return nil }
                    if trend && data[a].close[i] < e { return nil }
                    return (a, data[a].close[i] / data[a].close[i - lookback] - 1)
                }.filter { !trend || $0.1 > 0 }.sorted { $0.1 > $1.1 }.prefix(top).map(\.0)
                let changed = Set(candidates).symmetricDifference(Set(held)).count
                value *= 1 - 0.001 * Double(changed) / Double(max(1, top))
                held = candidates
            }
            peak = max(peak, value); peakB = max(peakB, bench)
            dd = min(dd, value / peak - 1); ddB = min(ddB, bench / peakB - 1)
        }
        return Out(ret: value - 100, bench: bench - 100, dd: dd * 100, benchDD: ddB * 100, expo: Double(invested) / Double(max(1, dates.count)) * 100)
    }
}
