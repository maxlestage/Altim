import Foundation

/// Analysis of the portfolio actually held (Swift port of `web/src/engine/holdings.ts`,
/// checked by the shared `holdings-fixture.json` file).
public enum HoldingsAnalyzer {
    public static let maxWeight = 35.0
    public static let targetWeight = 30.0

    public struct SignalSummary: Codable, Hashable, Sendable {
        public let action: SignalAction
        public let score: Double
        public init(action: SignalAction, score: Double) { self.action = action; self.score = score }
    }

    public struct MarketInput: Sendable {
        public var price: Double
        public var daily: [Candle]
        public var daySignal: SignalSummary?
        public var shortSignal: SignalSummary?
        public var reliability: ReliabilityLevel?
        public init(price: Double, daily: [Candle], daySignal: SignalSummary?, shortSignal: SignalSummary?, reliability: ReliabilityLevel?) {
            self.price = price; self.daily = daily; self.daySignal = daySignal; self.shortSignal = shortSignal; self.reliability = reliability
        }
    }

    public enum Recommendation: String, Codable, Sendable {
        case sell, protect, lighten, strengthen, hold, unknown

        public static let allCasesForUI: [Recommendation] = [.sell, .protect, .lighten, .strengthen, .hold, .unknown]

        public var label: String {
            switch self {
            case .sell: return "Vendre ou protéger"
            case .protect: return "Protéger (stop)"
            case .lighten: return "Alléger"
            case .strengthen: return "Renforcer possible"
            case .hold: return "Conserver"
            case .unknown: return "Données insuffisantes"
            }
        }
    }

    public enum InsightLevel: String, Codable, Sendable { case danger, warning, info, good }

    public enum InsightValue: Codable, Hashable, Sendable {
        case number(Double)
        case text(String)

        public init(from decoder: Decoder) throws {
            let c = try decoder.singleValueContainer()
            if let d = try? c.decode(Double.self) { self = .number(d) } else { self = .text(try c.decode(String.self)) }
        }
        public func encode(to encoder: Encoder) throws {
            var c = encoder.singleValueContainer()
            switch self {
            case let .number(d): try c.encode(d)
            case let .text(s): try c.encode(s)
            }
        }
        var number: Double { if case let .number(d) = self { return d }; return 0 }
        var text: String {
            switch self {
            case let .text(s): return s
            case let .number(d): return String(d)
            }
        }
    }

    public struct Insight: Codable, Hashable, Sendable {
        public let level: InsightLevel
        public let code: String
        public let values: [String: InsightValue]
    }

    public struct Line: Codable, Hashable, Sendable, Identifiable {
        public let id: String
        public let symbol: String
        public let kind: AssetClass
        public let name: String
        public let quantity: Double
        public let averagePrice: Double
        public let price: Double?
        public let value: Double
        public let invested: Double
        public let pnl: Double
        public let pnlPercent: Double
        public let weight: Double
        public let stop: Double?
        public let lossAtStop: Double?
        public let recommendation: Recommendation
        public let reasons: [String]
        public let trimValue: Double
    }

    public struct Allocation: Codable, Hashable, Sendable { public let crypto: Double; public let stock: Double; public let cash: Double }

    public struct Risk: Codable, Hashable, Sendable {
        public let maxWeight: Double
        public let effectiveAssets: Double
        public let volatilityAnnual: Double?
        public let var95Day: Double?
        public let var95DayPercent: Double?
        public let averageCorrelation: Double?
        public let lossAtStops: Double
    }

    public struct Analysis: Codable, Hashable, Sendable {
        public let total: Double
        public let cash: Double
        public let invested: Double
        public let marketValue: Double
        public let pnl: Double
        public let pnlPercent: Double
        public let allocation: Allocation
        public let lines: [Line]
        public let risk: Risk
        public let insights: [Insight]
    }

    // MARK: Statistics (same definitions as the TypeScript)

    static func mean(_ v: [Double]) -> Double { v.isEmpty ? 0 : v.reduce(0, +) / Double(v.count) }

    static func std(_ v: [Double]) -> Double {
        guard v.count >= 2 else { return 0 }
        let m = mean(v)
        return (v.reduce(0) { $0 + ($1 - m) * ($1 - m) } / Double(v.count - 1)).squareRoot()
    }

    public static func correlation(_ a: [Double], _ b: [Double]) -> Double? {
        let n = min(a.count, b.count)
        guard n >= 10 else { return nil }
        let x = Array(a.suffix(n)), y = Array(b.suffix(n))
        let mx = mean(x), my = mean(y)
        var sxy = 0.0, sxx = 0.0, syy = 0.0
        for i in 0..<n {
            sxy += (x[i] - mx) * (y[i] - my)
            sxx += (x[i] - mx) * (x[i] - mx)
            syy += (y[i] - my) * (y[i] - my)
        }
        return sxx > 0 && syy > 0 ? sxy / (sxx * syy).squareRoot() : nil
    }

    public static func percentile(_ v: [Double], _ p: Double) -> Double {
        let s = v.sorted()
        guard !s.isEmpty else { return 0 }
        let idx = Double(s.count - 1) * p
        let lo = Int(idx.rounded(.down)), hi = Int(idx.rounded(.up))
        return s[lo] + (s[hi] - s[lo]) * (idx - Double(lo))
    }

    /// Daily closes by UTC day; returns over the last `days` shared days (price carried forward when the market is closed).
    public static func alignedReturns(_ series: [[Candle]], days: Int = 90) -> [[Double]] {
        let maps: [[Int: Double]] = series.map { candles in
            var m: [Int: Double] = [:]
            for c in candles.sorted(by: { $0.time < $1.time }) {
                m[Int((c.time.timeIntervalSince1970 * 1000 / 86_400_000).rounded(.down))] = c.close
            }
            return m
        }
        let all = Set(maps.flatMap(\.keys)).sorted()
        let firsts = maps.map { $0.keys.min() ?? Int.max }
        guard let start = all.firstIndex(where: { d in firsts.allSatisfy { $0 <= d } }) else { return series.map { _ in [] } }
        let dates = Array(all[start...].suffix(days + 1))
        return maps.map { m in
            var last: Double?
            for k in m.keys.sorted() where k <= dates[0] { last = m[k] }
            let closes: [Double] = dates.map { d in
                if let v = m[d] { last = v }
                return last ?? 0
            }
            return closes.indices.dropFirst().map { i in closes[i - 1] > 0 ? closes[i] / closes[i - 1] - 1 : 0 }
        }
    }

    // MARK: Analysis

    public static func analyze(holdings: [Holding], cash: Double, market: [String: MarketInput]) -> Analysis {
        func key(_ h: Holding) -> String { "\(h.kind.rawValue):\(h.symbol)" }
        let base = holdings.map { h -> (h: Holding, m: MarketInput?, price: Double?, value: Double, invested: Double) in
            let m = market[key(h)]
            let price = (m?.price ?? 0) > 0 ? m!.price : nil
            return (h, m, price, h.quantity * (price ?? h.averagePrice), h.quantity * h.averagePrice)
        }
        let marketValue = base.reduce(0) { $0 + $1.value }
        let safeCash = max(0, cash)
        let total = marketValue + safeCash
        let investedTotal = base.reduce(0) { $0 + $1.invested }
        let pnl = marketValue - investedTotal
        let isBuy: (SignalAction?) -> Bool = { $0 == .buy || $0 == .strongBuy }
        let isSell: (SignalAction?) -> Bool = { $0 == .sell || $0 == .strongSell }

        let lines: [Line] = base.map { b in
            let h = b.h
            let weight = total > 0 ? b.value / total * 100 : 0
            let pnlL = b.value - b.invested
            let pnlPercent = b.invested > 0 ? pnlL / b.invested * 100 : 0
            var atrValue: Double?
            if let d = b.m?.daily, !d.isEmpty { atrValue = Indicators.atr(d)[d.count - 1] ?? nil }
            let stop: Double? = (b.price != nil && atrValue != nil && atrValue! != 0) ? max(b.price! - 2 * atrValue!, b.price! * 0.01) : nil
            let lossAtStop: Double? = (stop != nil && b.price != nil) ? h.quantity * (b.price! - stop!) : nil
            var reasons: [String] = []
            let rec: Recommendation
            let day = b.m?.daySignal?.action
            let short = b.m?.shortSignal?.action
            if b.price == nil || b.m == nil || b.m?.reliability == .low || (b.m?.daySignal == nil && b.m?.shortSignal == nil) {
                rec = .unknown
                reasons.append(b.price == nil ? "no_price" : "unreliable")
            } else if isSell(day) && isSell(short) {
                rec = .sell; reasons.append("bearish_day_and_short")
            } else if isSell(day) || short == .strongSell {
                rec = .protect; reasons.append(isSell(day) ? "bearish_day" : "strong_bearish_short")
            } else if weight > maxWeight && holdings.count > 1 {
                rec = .lighten; reasons.append("overweight")
            } else if pnlPercent > 50 && !isBuy(day) {
                rec = .lighten; reasons.append("take_profit")
            } else if isBuy(day) && isBuy(short) && weight < 20 {
                rec = .strengthen; reasons.append("bullish_day_and_short")
            } else {
                rec = .hold; reasons.append(isBuy(day) ? "bullish_day" : "neutral")
            }
            if rec != .lighten && weight > maxWeight && holdings.count > 1 { reasons.append("overweight") }
            if pnlPercent < -20 { reasons.append("deep_loss") }
            var trim = 0.0
            if rec == .lighten {
                trim = reasons[0] == "overweight" ? max(0, b.value - targetWeight / 100 * total) : b.value * 0.25
            }
            return Line(id: h.id, symbol: h.symbol, kind: h.kind, name: h.name, quantity: h.quantity, averagePrice: h.averagePrice,
                        price: b.price, value: b.value, invested: b.invested, pnl: pnlL, pnlPercent: pnlPercent, weight: weight,
                        stop: stop, lossAtStop: lossAtStop, recommendation: rec, reasons: reasons, trimValue: trim)
        }

        let crypto = lines.filter { $0.kind == .crypto }.reduce(0) { $0 + $1.value }
        let stock = lines.filter { $0.kind == .stock }.reduce(0) { $0 + $1.value }
        func pct(_ v: Double) -> Double { total > 0 ? v / total * 100 : 0 }
        let hhi = lines.reduce(0) { $0 + ($1.weight / 100) * ($1.weight / 100) }
        let maxW = lines.reduce(0) { max($0, $1.weight) }

        let withData = lines.indices.compactMap { i -> (Line, [Candle])? in
            let d = base[i].m?.daily ?? []
            return d.count >= 20 ? (lines[i], d) : nil
        }
        var vol: Double?, var95: Double?, avgCorr: Double?
        if !withData.isEmpty && total > 0 {
            let returns = alignedReturns(withData.map(\.1))
            let n = returns.map(\.count).min() ?? 0
            if n >= 20 {
                let port: [Double] = (0..<n).map { t in
                    withData.indices.reduce(0.0) { acc, i in acc + withData[i].0.value / total * returns[i][returns[i].count - n + t] }
                }
                vol = std(port) * 365.0.squareRoot() * 100
                var95 = max(0, -percentile(port, 0.05) * total)
                var pairs: [Double] = []
                for i in returns.indices { for j in returns.indices where j > i { if let c = correlation(returns[i], returns[j]) { pairs.append(c) } } }
                avgCorr = pairs.isEmpty ? nil : mean(pairs)
            }
        }
        let lossAtStops = lines.reduce(0) { $0 + ($1.lossAtStop ?? 0) }

        var insights: [Insight] = []
        func add(_ level: InsightLevel, _ code: String, _ values: [String: InsightValue] = [:]) {
            insights.append(Insight(level: level, code: code, values: values))
        }
        if lines.isEmpty {
            add(.info, "empty")
        } else {
            add(pnl >= 0 ? .good : .warning, "pnl", ["pnl": .number(pnl), "pnlPercent": .number(investedTotal > 0 ? pnl / investedTotal * 100 : 0)])
            let sells = lines.filter { $0.recommendation == .sell || $0.recommendation == .protect }
            if !sells.isEmpty { add(.danger, "act_bearish", ["count": .number(Double(sells.count)), "symbols": .text(sells.map(\.symbol).joined(separator: ", "))]) }
            let top = lines.reduce(lines[0]) { $1.weight > $0.weight ? $1 : $0 }
            if lines.count > 1 && top.weight > 40 { add(.danger, "concentration", ["symbol": .text(top.symbol), "weight": .number(top.weight)]) }
            else if lines.count > 1 && top.weight > 25 { add(.warning, "concentration", ["symbol": .text(top.symbol), "weight": .number(top.weight)]) }
            else if lines.count == 1 { add(.warning, "single_asset", ["symbol": .text(top.symbol)]) }
            if pct(crypto) > 60 { add(.warning, "crypto_heavy", ["weight": .number(pct(crypto))]) }
            if lines.count > 1 && hhi > 0 && 1 / hhi < 3 { add(.warning, "low_diversification", ["effective": .number(1 / hhi)]) }
            if let c = avgCorr, c > 0.7, lines.count > 1 { add(.warning, "correlated", ["correlation": .number(c)]) }
            if let v = var95 { add(.info, "var", ["amount": .number(v), "percent": .number(total > 0 ? v / total * 100 : 0)]) }
            if lossAtStops > 0 { add(.info, "stops", ["amount": .number(lossAtStops), "percent": .number(total > 0 ? lossAtStops / total * 100 : 0)]) }
            if pct(safeCash) < 5 { add(.info, "low_cash", ["weight": .number(pct(safeCash))]) }
            let deep = lines.filter { $0.pnlPercent < -20 }
            if !deep.isEmpty { add(.warning, "deep_loss", ["symbols": .text(deep.map(\.symbol).joined(separator: ", "))]) }
            let strong = lines.filter { $0.recommendation == .strengthen }
            if !strong.isEmpty { add(.good, "opportunities", ["symbols": .text(strong.map(\.symbol).joined(separator: ", "))]) }
            let unknown = lines.filter { $0.recommendation == .unknown }
            if !unknown.isEmpty { add(.warning, "unknown", ["symbols": .text(unknown.map(\.symbol).joined(separator: ", "))]) }
        }
        // Stable sort by severity (like Array.prototype.sort in JavaScript).
        let order: [InsightLevel] = [.danger, .warning, .good, .info]
        insights = insights.enumerated().sorted {
            let a = order.firstIndex(of: $0.element.level)!, b = order.firstIndex(of: $1.element.level)!
            return a != b ? a < b : $0.offset < $1.offset
        }.map(\.element)

        return Analysis(
            total: total, cash: safeCash, invested: investedTotal, marketValue: marketValue, pnl: pnl,
            pnlPercent: investedTotal > 0 ? pnl / investedTotal * 100 : 0,
            allocation: Allocation(crypto: pct(crypto), stock: pct(stock), cash: pct(safeCash)),
            lines: lines,
            risk: Risk(maxWeight: maxW, effectiveAssets: hhi > 0 ? 1 / hhi : 0, volatilityAnnual: vol, var95Day: var95,
                       var95DayPercent: (var95 != nil && total > 0) ? var95! / total * 100 : nil,
                       averageCorrelation: avgCorr, lossAtStops: lossAtStops),
            insights: insights
        )
    }

    // MARK: Plain-language texts

    public static let reasonText: [String: String] = [
        "bearish_day_and_short": "Signaux baissiers en journalier et en 4 h : la tendance s'est retournée.",
        "bearish_day": "Signal journalier baissier : placez ou remontez un stop pour limiter la baisse.",
        "strong_bearish_short": "Forte pression vendeuse à court terme (4 h) : protégez la position.",
        "overweight": "Cette ligne dépasse 35 % de votre patrimoine : trop dépendant d'un seul actif.",
        "take_profit": "Plus de 50 % de gain sans signal haussier : sécuriser une partie des gains.",
        "bullish_day_and_short": "Signaux haussiers en journalier et en 4 h, poids encore modéré.",
        "bullish_day": "Tendance journalière haussière : rien à faire.",
        "neutral": "Pas de signal fort : rien à faire pour l'instant.",
        "unreliable": "Sources de données absentes ou en désaccord : aucun conseil plutôt qu'un mauvais conseil.",
        "no_price": "Cours introuvable pour ce symbole.",
        "deep_loss": "Perte de plus de 20 % : évitez de moyenner à la baisse sans signal haussier.",
    ]

    public static func text(_ i: Insight) -> String {
        let v = i.values
        func usd(_ x: Double) -> String { x.formatted(.number.precision(.fractionLength(0)).locale(Locale(identifier: "fr_FR"))) + " $" }
        func pc(_ x: Double) -> String { x.formatted(.number.precision(.fractionLength(0...1)).locale(Locale(identifier: "fr_FR"))) + " %" }
        let n = { (k: String) in v[k]?.number ?? 0 }
        let s = { (k: String) in v[k]?.text ?? "" }
        switch i.code {
        case "empty": return "Ajoutez vos actifs pour obtenir une analyse complète."
        case "pnl": return "Plus-value latente : \(n("pnl") >= 0 ? "+" : "−")\(usd(abs(n("pnl")))) (\(n("pnlPercent") >= 0 ? "+" : "−")\(pc(abs(n("pnlPercent")))))."
        case "act_bearish": return "\(Int(n("count"))) ligne(s) à surveiller de près (signaux baissiers) : \(s("symbols"))."
        case "concentration": return "\(s("symbol")) pèse \(pc(n("weight"))) de votre patrimoine : une baisse de cet actif vous toucherait fortement."
        case "single_asset": return "Tout est investi sur \(s("symbol")) : aucune diversification."
        case "crypto_heavy": return "\(pc(n("weight"))) en crypto : portefeuille très volatil."
        case "low_diversification": return String(format: "Diversification faible : l'équivalent de %.1f actif(s) de même poids.", n("effective"))
        case "correlated": return String(format: "Vos actifs évoluent ensemble (corrélation moyenne %.2f) : ils baisseront probablement en même temps.", n("correlation"))
        case "var": return "Lors d'une mauvaise journée (1 sur 20), vous pourriez perdre environ \(usd(n("amount"))) (\(pc(n("percent"))))."
        case "stops": return "Si tous les stops conseillés étaient touchés : perte d'environ \(usd(n("amount"))) (\(pc(n("percent"))))."
        case "low_cash": return "Peu de liquidités (\(pc(n("weight")))) : aucune réserve pour saisir une opportunité."
        case "deep_loss": return "Lignes en perte de plus de 20 % : \(s("symbols"))."
        case "opportunities": return "Renforcement possible selon les signaux : \(s("symbols"))."
        case "unknown": return "Analyse impossible pour : \(s("symbols")) (données insuffisantes)."
        default: return i.code
        }
    }
}
