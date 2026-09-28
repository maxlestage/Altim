import Foundation

// « Et si… ? » (Mes avoirs): a shock on one market factor — the Nasdaq-100 (via QQQ), the S&P 500 (via SPY) or
// Bitcoin — passed through each line's beta to that factor, measured on the days where BOTH have a daily close.
// Port of web/src/engine/portfolio-risk.ts (`FACTORS`, `factorBeta`, `whatIf`), same rules and texts.

public enum FactorKey: String, CaseIterable, Sendable, Hashable {
    case qqq, spy, btc

    public var asset: Asset {
        switch self {
        case .qqq: return Asset(symbol: "QQQ", kind: .stock, name: "Invesco QQQ Trust")
        case .spy: return Asset(symbol: "SPY", kind: .stock, name: "SPDR S&P 500 ETF")
        case .btc: return Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
        }
    }

    public var label: String {
        switch self {
        case .qqq: return "Nasdaq-100 (QQQ)"
        case .spy: return "S&P 500 (SPY)"
        case .btc: return "Bitcoin"
        }
    }

    /// "le Nasdaq-100".
    public var name: String {
        switch self {
        case .qqq: return "le Nasdaq-100"
        case .spy: return "le S&P 500"
        case .btc: return "le Bitcoin"
        }
    }
}

public struct FactorBeta: Sendable, Equatable {
    public var beta: Double
    public var days: Int
    public var estimated: Bool
    /// Correlation of the daily returns with the factor (nil: too few days).
    public var correlation: Double?
    public init(beta: Double, days: Int, estimated: Bool, correlation: Double?) {
        self.beta = beta
        self.days = days
        self.estimated = estimated
        self.correlation = correlation
    }
}

public struct WhatIfLine: Sendable, Identifiable {
    public var key: String
    public var symbol: String
    public var name: String
    public var kind: Kind
    /// Value of the line in the simulated amount (USD).
    public var value: Double
    /// % of the simulated amount.
    public var weight: Double
    public var beta: Double?
    public var days: Int
    public var correlation: Double?
    /// The line is the factor itself (beta 1 by definition).
    public var reference: Bool
    /// Move of the line (%), nil when its beta could not be measured ("non couvert").
    public var movePercent: Double?
    /// USD, positive = loss; nil when not covered.
    public var loss: Double?
    public var id: String { key }
}

public struct WhatIfResult: Sendable {
    public var factor: FactorKey
    public var shock: Double
    /// Simulated amount: the portfolio's value, or the amount entered spread on the current weights.
    public var base: Double
    public var scaled: Bool
    public var cash: Double
    public var lines: [WhatIfLine]
    /// Total loss of the covered lines (USD, positive = loss) and its share of `base`.
    public var loss: Double
    public var lossPercent: Double
    /// Value of the lines whose beta could not be measured (left out of the total, never guessed).
    public var uncoveredValue: Double
    public var uncovered: [String]
    public var worst: WhatIfLine?
    /// Shortest and longest window of the betas used (days).
    public var minDays: Int?
    public var maxDays: Int?
}

public enum WhatIf {
    public static let shocks: [Double] = [-5, -10, -20, -30, -50]
    /// One year of shared sessions, steadier than 90 days for a shock on one market.
    public static let betaDays = 250
    /// Below this |correlation| the beta explains little of the line's moves: said next to the line.
    public static let weakCorrelation = 0.3

    private static func dayKey(_ ms: Double) -> Int { Int((ms / 86_400_000).rounded(.down)) }

    /// Beta of an asset to a factor on the days where BOTH have a daily close (no forward fill: a crypto's weekends do
    /// not become days where a stock "did not move"), returns between consecutive shared days, last `days` of them.
    /// Not estimated (estimated false, beta 1 only as a placeholder) under `RiskEngine.minBetaDays` shared returns.
    public static func factorBeta(_ asset: [Candle], _ factor: [Candle], days: Int = RiskEngine.betaDays) -> FactorBeta {
        var fa: [Int: Double] = [:]
        for c in factor where c.close > 0 { fa[dayKey(c.time)] = c.close }
        var own: [Int: Double] = [:]
        for c in asset where c.close > 0 { own[dayKey(c.time)] = c.close }
        let shared = Array(own.filter { fa[$0.key] != nil }.sorted { $0.key < $1.key }.suffix(days + 1))
        var x: [Double] = []
        var y: [Double] = []
        if shared.count > 1 {
            for i in 1..<shared.count {
                x.append(shared[i].value / shared[i - 1].value - 1)
                y.append(fa[shared[i].key]! / fa[shared[i - 1].key]! - 1)
            }
        }
        let n = x.count
        guard n >= RiskEngine.minBetaDays else { return FactorBeta(beta: 1, days: n, estimated: false, correlation: nil) }
        let mx = x.reduce(0, +) / Double(n)
        let my = y.reduce(0, +) / Double(n)
        var cov = 0.0, vx = 0.0, vy = 0.0
        for i in 0..<n {
            cov += (x[i] - mx) * (y[i] - my)
            vx += (x[i] - mx) * (x[i] - mx)
            vy += (y[i] - my) * (y[i] - my)
        }
        guard vy > 0 else { return FactorBeta(beta: 1, days: n, estimated: false, correlation: nil) }
        return FactorBeta(beta: cov / vy, days: n, estimated: true, correlation: vx > 0 ? cov / (vx * vy).squareRoot() : nil)
    }

    /// Each line moves by its beta to the factor × the shock (never below −100 %); cash does not move. Lines of the
    /// same asset are merged. With `amount` (> 0), the lines are rescaled to that amount on the current weights (cash
    /// included).
    public static func run(_ p: RiskPortfolio, factor: FactorKey, shock: Double, betas: [String: FactorBeta], amount: Double? = nil) -> WhatIfResult {
        let f = factor.asset
        let scaled = amount.map { $0.isFinite && $0 > 0 } == true && p.total > 0
        let k = scaled ? amount! / p.total : 1
        let base = scaled ? amount! : p.total
        var order: [String] = []
        var merged: [String: (symbol: String, name: String, kind: Kind, value: Double)] = [:]
        for l in p.lines {
            if merged[l.key] == nil { order.append(l.key) }
            let prev = merged[l.key]?.value ?? 0
            merged[l.key] = (l.symbol, l.name, l.kind, prev + l.value * k)
        }
        let lines = order.enumerated().map { (i, key) -> (Int, WhatIfLine) in
            let m = merged[key]!
            let reference = m.symbol == f.symbol && m.kind == f.kind
            let b = betas[key]
            let beta: Double? = reference ? 1 : (b?.estimated == true ? b?.beta : nil)
            let move = beta.map { max(-100, $0 * shock) }
            return (i, WhatIfLine(key: key, symbol: m.symbol, name: m.name, kind: m.kind, value: m.value, weight: base > 0 ? m.value / base * 100 : 0,
                                  beta: beta, days: reference ? 0 : b?.days ?? 0, correlation: reference ? 1 : b?.correlation, reference: reference,
                                  movePercent: move, loss: move.map { -m.value * $0 / 100 }))
        }
        .sorted { a, b in
            let la = a.1.loss ?? -.infinity, lb = b.1.loss ?? -.infinity
            if la != lb { return la > lb }
            if a.1.value != b.1.value { return a.1.value > b.1.value }
            return a.0 < b.0
        }
        .map(\.1)
        let covered = lines.filter { $0.loss != nil }
        let loss = covered.reduce(0) { $0 + $1.loss! }
        let worst = covered.reduce(nil as WhatIfLine?) { w, l in l.loss! > 0 && (w == nil || l.loss! > w!.loss!) ? l : w }
        let measured = covered.filter { !$0.reference }.map(\.days)
        let uncovered = lines.filter { $0.loss == nil }
        return WhatIfResult(factor: factor, shock: shock, base: base, scaled: scaled, cash: p.cash * k, lines: lines, loss: loss,
                            lossPercent: base > 0 ? loss / base * 100 : 0, uncoveredValue: uncovered.reduce(0) { $0 + $1.value },
                            uncovered: uncovered.map(\.symbol), worst: worst, minDays: measured.min(), maxDays: measured.max())
    }

    // MARK: Texts (web WhatIfCard.tsx)

    static func fr(_ v: Double, _ d: Int = 1) -> String { JSFormat.fr(v, max: d) }
    /// "1 234 $".
    public static func usd(_ v: Double) -> String { "\(JSFormat.fr(v, max: 0)) $" }
    /// A loss is written "−800 $", a gain "+200 $".
    public static func signedUsd(_ loss: Double) -> String { "\(loss > 0 ? "−" : loss < 0 ? "+" : "")\(usd(abs(loss)))" }
    public static func signedPct(_ v: Double) -> String { "\(v > 0 ? "+" : v < 0 ? "−" : "")\(fr(abs(v))) %" }
    /// JavaScript `toFixed(2)` of a beta or a correlation ("-0.30" keeps its hyphen and dot, as on the web).
    static func fixed2(_ v: Double) -> String {
        let s = JSFormat.fr(abs(v), min: 2, max: 2).replacingOccurrences(of: "\u{202F}", with: "").replacingOccurrences(of: ",", with: ".")
        return (v < 0 && s != "0.00" ? "-" : "") + s
    }

    public static func intro(_ f: FactorKey, shock: Double) -> String {
        "Comment le risque de ce portefeuille évolue si \(f.name) baisse de \(fr(abs(shock))) % ? Chaque ligne bouge selon son bêta face à ce marché."
    }

    /// "4 000 $ · 40 % · bêta 2.00, corrélation 0.80 · −20 %".
    public static func lineDetail(_ l: WhatIfLine, factor: FactorKey) -> String {
        var s = "\(usd(l.value)) · \(fr(l.weight)) %"
        if l.reference {
            s += " · c'est ce marché lui-même (bêta 1)"
        } else if let beta = l.beta {
            s += " · bêta \(fixed2(beta))\(l.correlation.map { ", corrélation \(fixed2($0))" } ?? "") · \(signedPct(l.movePercent ?? 0))"
            if let c = l.correlation, abs(c) < weakCorrelation {
                s += " · lien faible avec \(factor.label) : ce bêta explique mal les mouvements de la ligne, résultat peu fiable"
            }
        } else {
            s += " · moins de \(RiskEngine.minBetaDays) jours communs avec \(factor.label) : bêta non mesurable"
        }
        return s
    }

    public static func footnote(_ r: WhatIfResult) -> String {
        let days: String
        if let lo = r.minDays, let hi = r.maxDays {
            days = "Bêta estimé sur \(lo)\(hi != lo ? " à \(hi)" : "") jours de rendements journaliers communs"
        } else {
            days = "Aucun bêta mesuré"
        }
        return "\(days) ; hypothèse : choc instantané, relation stable — rarement vrai en crise (les corrélations montent quand tout baisse). Les liquidités ne bougent pas. Ce n'est pas une prévision."
    }
}
