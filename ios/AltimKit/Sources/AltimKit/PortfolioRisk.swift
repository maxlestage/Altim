import Foundation

// Risk of the portfolio the user holds: market stress scenarios through each line's beta, checks against the user's
// risk settings (risk per line, weight per line, crypto share, correlated clusters, daily loss) and the positions that
// became dangerous (stop broken or close, loss beyond the risk accepted per idea).
// Exact port of web/src/engine/portfolio-risk.ts (and the parts of holdings.ts it needs): same inputs, same results,
// same French texts. Pure, deterministic functions.

// MARK: - Settings

/// The user's risk limits (Réglages → Prudence des conseils), same defaults and bounds as the web.
public struct RiskSettings: Codable, Sendable, Equatable {
    /// Share of the portfolio a buy idea accepts to lose if its stop is hit, %.
    public var riskPerTradePercent: Double
    /// Maximum weight of one line, %.
    public var maxPositionPercent: Double
    /// Loss since the previous close beyond which no new position should be opened today, %.
    public var dailyLossLimitPercent: Double
    /// Cap of the crypto share of the portfolio, %.
    public var maxCryptoPercent: Double

    public init(riskPerTradePercent: Double = 1, maxPositionPercent: Double = 20, dailyLossLimitPercent: Double = 3, maxCryptoPercent: Double = 60) {
        self.riskPerTradePercent = riskPerTradePercent
        self.maxPositionPercent = maxPositionPercent
        self.dailyLossLimitPercent = dailyLossLimitPercent
        self.maxCryptoPercent = maxCryptoPercent
    }

    public static let defaults = RiskSettings()

    /// Steppers of the settings screen: key path, label, bounds, step, unit (the web's RISK_FIELDS).
    public struct Field {
        public let key: WritableKeyPath<RiskSettings, Double>
        public let label: String
        public let min: Double
        public let max: Double
        public let step: Double
        public let unit: String
    }

    public static let fields: [Field] = [
        Field(key: \.riskPerTradePercent, label: "Risque accepté par idée", min: 0.25, max: 5, step: 0.25, unit: " %"),
        Field(key: \.maxPositionPercent, label: "Taille max d'une ligne", min: 5, max: 100, step: 5, unit: " %"),
        Field(key: \.dailyLossLimitPercent, label: "Perte max du jour", min: 0.5, max: 10, step: 0.5, unit: " %"),
        Field(key: \.maxCryptoPercent, label: "Part crypto max", min: 0, max: 100, step: 5, unit: " %"),
    ]

    /// A missing or out-of-range value (older or hand-edited settings) takes the default.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = RiskSettings.defaults
        func read(_ k: CodingKeys, _ fallback: Double) -> Double { ((try? c.decodeIfPresent(Double.self, forKey: k)) ?? nil) ?? fallback }
        self.init(riskPerTradePercent: read(.riskPerTradePercent, d.riskPerTradePercent), maxPositionPercent: read(.maxPositionPercent, d.maxPositionPercent),
                  dailyLossLimitPercent: read(.dailyLossLimitPercent, d.dailyLossLimitPercent), maxCryptoPercent: read(.maxCryptoPercent, d.maxCryptoPercent))
        self = validated
    }

    /// Each value within its bounds, else the default.
    public var validated: RiskSettings {
        var s = self
        for f in Self.fields {
            let v = s[keyPath: f.key]
            if !v.isFinite || v < f.min || v > f.max { s[keyPath: f.key] = Self.defaults[keyPath: f.key] }
        }
        return s
    }

    /// One step up or down, kept within the bounds (rounded to 2 decimals like the web).
    public func stepped(_ f: Field, up: Bool) -> RiskSettings {
        var s = self
        let v = ((s[keyPath: f.key] + (up ? f.step : -f.step)) * 100).rounded() / 100
        s[keyPath: f.key] = Swift.min(f.max, Swift.max(f.min, v))
        return s
    }
}

// MARK: - Portfolio seen by the risk engine

/// A line of the portfolio valued at the current price (web: `LineAnalysis`, the fields the risk engine reads).
public struct RiskLine: Sendable, Identifiable {
    public var id: String
    public var symbol: String
    public var kind: Kind
    public var name: String
    public var quantity: Double
    /// Current price; nil when unavailable (the line is then valued at its average cost, as on the web).
    public var price: Double?
    public var value: Double
    /// quantity × average cost; nil without an average cost (no unrealised loss then).
    public var invested: Double?
    /// % of the total (cash included).
    public var weight: Double
    /// Protective stop of the analysis: price − 2 × daily ATR (14), at least 1 % of the price.
    public var protectiveStop: Double?
    /// The user's own stop (USD), when set.
    public var userStop: Double?
    public var key: String { "\(kind.rawValue):\(symbol)" }
}

public struct RiskPortfolio: Sendable {
    public var lines: [RiskLine]
    public var cash: Double
    /// Lines only.
    public var marketValue: Double
    /// Lines + cash.
    public var total: Double
    /// Share of the cryptos in the total, %.
    public var cryptoShare: Double

    /// `prices` and `daily` keyed "kind:SYMBOL". Same valuation as `analyzePortfolio` of the web.
    public init(holdings: [Holding], cash: Double = 0, prices: [String: Double], daily: [String: [Candle]]) {
        let base = holdings.map { h -> (Holding, Double?, Double) in
            let p = prices[h.asset.id].flatMap { $0 > 0 && $0.isFinite ? $0 : nil }
            return (h, p, h.quantity * (p ?? h.averagePrice ?? 0))
        }
        marketValue = base.reduce(0) { $0 + $1.2 }
        self.cash = max(0, cash)
        total = marketValue + self.cash
        let t = total
        lines = base.map { h, price, value in
            let candles = daily[h.asset.id] ?? []
            let a = candles.isEmpty ? nil : RiskEngine.atr(candles).last ?? nil
            var stop: Double?
            if let price, let a, a > 0 { stop = max(price - 2 * a, price * 0.01) }
            return RiskLine(id: h.id.uuidString, symbol: h.asset.symbol, kind: h.asset.kind, name: h.asset.name, quantity: h.quantity, price: price,
                            value: value, invested: h.averagePrice.map { h.quantity * $0 }, weight: t > 0 ? value / t * 100 : 0,
                            protectiveStop: stop, userStop: h.stop.flatMap { $0 > 0 ? $0 : nil })
        }
        let crypto = lines.filter { $0.kind == .crypto }.reduce(0) { $0 + $1.value }
        cryptoShare = t > 0 ? crypto / t * 100 : 0
    }
}

// MARK: - Results

public struct BetaEstimate: Sendable, Equatable {
    public var beta: Double
    /// Shared daily returns used; 0 with the fallback.
    public var days: Int
    /// false: history too short or missing, beta 1 assumed.
    public var estimated: Bool
    /// The asset is the benchmark itself (beta 1 by definition).
    public var reference: Bool = false

    public static let fallback = BetaEstimate(beta: 1, days: 0, estimated: false)
}

/// Shock of each benchmark, % (negative = fall).
public struct StressScenario: Sendable, Equatable {
    public var key: String
    public var label: String
    public var crypto: Double
    public var stock: Double
}

public struct StressResult: Sendable {
    public struct Worst: Sendable { public var symbol: String; public var name: String; public var loss: Double; public var movePercent: Double }
    public var scenario: StressScenario
    /// Amount lost (USD, positive = loss; negative when the scenario would gain).
    public var loss: Double
    /// Of the whole portfolio (cash included).
    public var lossPercent: Double
    /// Of the invested part only: the difference is what the cash cushions.
    public var investedLossPercent: Double
    public var worst: Worst?
}

public struct Cluster: Sendable {
    public var symbols: [String]
    public var averageCorrelation: Double
    public var weight: Double
    public var days: Int
}

public struct DailyChange: Sendable {
    /// USD, negative = loss today.
    public var change: Double
    /// Of the portfolio value at the previous close (cash included).
    public var percent: Double
    /// Lines with a live price and a previous close / all lines.
    public var covered: Int
    public var lines: Int
}

public enum LimitLevel: String, Sendable { case danger, warning, ok, na }

public struct LimitCheck: Sendable, Identifiable {
    public var code: String
    public var level: LimitLevel
    public var label: String
    public var detail: String
    public var id: String { code }
}

public enum DangerCode: String, Codable, Sendable { case stopBroken = "stop_broken", nearStop = "near_stop", lossOverRisk = "loss_over_risk" }

public struct Danger: Codable, Sendable, Identifiable, Equatable {
    public struct Reason: Codable, Sendable, Equatable { public var code: DangerCode; public var text: String }
    public var id: String
    public var symbol: String
    public var kind: Kind
    public var name: String
    public var reasons: [Reason]
}

/// Last dangerous positions measured on Mes avoirs, kept on the iPhone so the Radar can show them with their time.
public struct DangerState: Codable, Sendable, Equatable {
    /// ms.
    public var at: Double
    public var items: [Danger]
    public init(at: Double, items: [Danger]) {
        self.at = at
        self.items = items
    }
}

// MARK: - Engine

public enum RiskEngine {
    /// Window of the beta and of the clusters (same as the correlations of the portfolio analysis), minimum shared days.
    public static let betaDays = 90
    public static let minBetaDays = 30
    public static let clusterCorrelation = 0.7
    public static let dailyLossReached = "Limite de perte du jour atteinte : n'ouvrez plus de position aujourd'hui."

    /// Benchmark of each asset class: Bitcoin for cryptos, the S&P 500 (via the SPY ETF) for stocks.
    public static func benchmark(_ kind: Kind) -> (asset: Asset, label: String) {
        kind == .crypto ? (Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), "Bitcoin") : (Asset(symbol: "SPY", kind: .stock, name: "SPDR S&P 500 ETF"), "S&P 500")
    }

    public static let scenarios: [StressScenario] = [
        StressScenario(key: "all5", label: "Marchés −5 %", crypto: -5, stock: -5),
        StressScenario(key: "all10", label: "Marchés −10 %", crypto: -10, stock: -10),
        StressScenario(key: "all20", label: "Marchés −20 %", crypto: -20, stock: -20),
        StressScenario(key: "all30", label: "Marchés −30 %", crypto: -30, stock: -30),
        StressScenario(key: "crypto20", label: "Crypto −20 %, actions stables", crypto: -20, stock: 0),
        StressScenario(key: "stock10crypto30", label: "Actions −10 %, crypto −30 %", crypto: -30, stock: -10),
    ]

    // MARK: Indicators

    /// Average true range (Wilder, 14): nil until the 14th candle.
    public static func atr(_ c: [Candle], period: Int = 14) -> [Double?] {
        var out = [Double?](repeating: nil, count: c.count)
        guard c.count >= period else { return out }
        let tr = c.indices.map { i in
            i == 0 ? c[i].high - c[i].low : max(c[i].high - c[i].low, abs(c[i].high - c[i - 1].close), abs(c[i].low - c[i - 1].close))
        }
        var prev = tr[0..<period].reduce(0, +) / Double(period)
        out[period - 1] = prev
        for i in period..<c.count {
            prev = (prev * Double(period - 1) + tr[i]) / Double(period)
            out[i] = prev
        }
        return out
    }

    private static func utcDay(_ ms: Double) -> Int { Int((ms / 86_400_000).rounded(.down)) }

    /// Daily closes by UTC calendar day (the last candle of a day wins), days sorted.
    private static func closesByDay(_ c: [Candle]) -> [Int: Double] {
        var m: [Int: Double] = [:]
        for x in c.sorted(by: { $0.time < $1.time }) { m[utcDay(x.time)] = x.close }
        return m
    }

    /// Daily returns over the last `days` shared calendar days, carrying the price forward when a market is closed.
    public static func alignedReturns(_ series: [[Candle]], days: Int = 90) -> [[Double]] {
        let maps = series.map(closesByDay)
        let all = Set(maps.flatMap { $0.keys }).sorted()
        let firsts = maps.map { $0.keys.min() }
        guard let start = all.firstIndex(where: { d in firsts.allSatisfy { f in f.map { $0 <= d } ?? false } }) else { return series.map { _ in [] } }
        let dates = Array(all[start...].suffix(days + 1))
        return maps.map { m -> [Double] in
            var last: Double? = nil
            for k in m.keys.sorted() where k <= dates[0] { last = m[k] }
            var closes: [Double] = []
            for d in dates {
                if let v = m[d] { last = v }
                closes.append(last ?? 0)
            }
            var out: [Double] = []
            for i in 1..<max(1, closes.count) {
                let prev = closes[i - 1]
                out.append(prev > 0 ? closes[i] / prev - 1 : 0)
            }
            return out
        }
    }

    private static func mean(_ v: [Double]) -> Double { v.isEmpty ? 0 : v.reduce(0, +) / Double(v.count) }

    /// Pearson correlation of the last common returns; nil under 10 of them or without variation.
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

    // MARK: Beta

    /// Beta of an asset to its benchmark = cov(asset, benchmark) ÷ var(benchmark) on the aligned daily returns.
    public static func estimateBeta(_ asset: [Candle], _ benchmark: [Candle], days: Int = betaDays) -> BetaEstimate {
        guard asset.count >= 2, benchmark.count >= 2 else { return .fallback }
        let r = alignedReturns([asset, benchmark], days: days)
        let n = min(r[0].count, r[1].count)
        guard n >= minBetaDays else { return .fallback }
        let x = Array(r[0].suffix(n)), y = Array(r[1].suffix(n))
        let mx = x.reduce(0, +) / Double(n), my = y.reduce(0, +) / Double(n)
        var cov = 0.0, vary = 0.0
        for i in 0..<n {
            cov += (x[i] - mx) * (y[i] - my)
            vary += (y[i] - my) * (y[i] - my)
        }
        return vary > 0 ? BetaEstimate(beta: cov / vary, days: n, estimated: true) : .fallback
    }

    /// Beta of each held asset (key "kind:SYMBOL"): the benchmark itself is 1 by definition.
    public static func betas(_ p: RiskPortfolio, daily: [String: [Candle]]) -> [String: BetaEstimate] {
        var b: [String: BetaEstimate] = [:]
        for l in p.lines {
            let ref = benchmark(l.kind).asset
            b[l.key] = l.symbol == ref.symbol && l.kind == ref.kind
                ? BetaEstimate(beta: 1, days: 0, estimated: false, reference: true)
                : estimateBeta(daily[l.key] ?? [], daily[ref.id] ?? [])
        }
        return b
    }

    // MARK: Stress scenarios

    /// Each line moves by beta × the shock of its benchmark (never below −100 %); cash does not move. Lines of the
    /// same asset share its beta (beta 1 when missing).
    public static func stressTest(_ p: RiskPortfolio, betas: [String: BetaEstimate], scenarios: [StressScenario] = scenarios) -> [StressResult] {
        scenarios.map { scenario in
            var order: [String] = []
            var byAsset: [String: StressResult.Worst] = [:]
            for l in p.lines {
                let beta = betas[l.key]?.beta ?? 1
                let move = max(-100, beta * (l.kind == .crypto ? scenario.crypto : scenario.stock))
                let loss = -l.value * move / 100
                if byAsset[l.key] == nil { order.append(l.key) }
                byAsset[l.key] = StressResult.Worst(symbol: l.symbol, name: l.name, loss: (byAsset[l.key]?.loss ?? 0) + loss, movePercent: move)
            }
            let items = order.compactMap { byAsset[$0] }
            let loss = items.reduce(0) { $0 + $1.loss }
            var worst: StressResult.Worst?
            for x in items where x.loss > 0 && (worst == nil || x.loss > worst!.loss) { worst = x }
            return StressResult(scenario: scenario, loss: loss, lossPercent: p.total > 0 ? loss / p.total * 100 : 0,
                                investedLossPercent: p.marketValue > 0 ? loss / p.marketValue * 100 : 0, worst: worst)
        }
    }

    // MARK: Correlated clusters

    /// Groups of assets linked by a correlation above 0.7 (connected by pairs), kept when their average pairwise
    /// correlation stays above 0.7. One entry per asset: weight in % of the portfolio and daily candles.
    public static func correlatedClusters(_ series: [(symbol: String, weight: Double, daily: [Candle])], days: Int = betaDays) -> [Cluster] {
        let usable = series.filter { $0.daily.count >= 20 }
        guard usable.count >= 2 else { return [] }
        let returns = alignedReturns(usable.map(\.daily), days: days)
        let n = usable.count
        var corr = [[Double?]](repeating: [Double?](repeating: nil, count: n), count: n)
        for i in 0..<n {
            for j in (i + 1)..<n {
                let c = correlation(returns[i], returns[j])
                corr[i][j] = c
                corr[j][i] = c
            }
        }
        var seen = Set<Int>()
        var clusters: [Cluster] = []
        for s in 0..<n where !seen.contains(s) {
            var group = [s]
            seen.insert(s)
            var k = 0
            while k < group.count {
                for j in 0..<n where !seen.contains(j) && (corr[group[k]][j] ?? -1) > clusterCorrelation {
                    seen.insert(j)
                    group.append(j)
                }
                k += 1
            }
            guard group.count >= 2 else { continue }
            var pairs: [Double] = []
            for x in 0..<group.count {
                for y in (x + 1)..<group.count {
                    if let c = corr[group[x]][group[y]] { pairs.append(c) }
                }
            }
            let average = pairs.reduce(0, +) / Double(pairs.count)
            guard average > clusterCorrelation else { continue }
            clusters.append(Cluster(symbols: group.map { usable[$0].symbol }, averageCorrelation: average,
                                    weight: group.reduce(0) { $0 + usable[$1].weight }, days: returns.map(\.count).min() ?? 0))
        }
        // Heaviest first (stable, like Array.prototype.sort).
        return clusters.enumerated().sorted { ($0.element.weight, -$0.offset) > ($1.element.weight, -$1.offset) }.map(\.element)
    }

    /// Clusters of the portfolio: lines of the same asset added up.
    public static func clusters(_ p: RiskPortfolio, daily: [String: [Candle]]) -> [Cluster] {
        var order: [String] = []
        var weights: [String: (symbol: String, weight: Double, daily: [Candle])] = [:]
        for l in p.lines {
            if weights[l.key] == nil { order.append(l.key) }
            weights[l.key] = (l.symbol, (weights[l.key]?.weight ?? 0) + l.weight, daily[l.key] ?? [])
        }
        return correlatedClusters(order.compactMap { weights[$0] })
    }

    // MARK: Today's change

    /// Previous close: the close of the last daily candle before today's (UTC calendar day; for stocks, the last
    /// session). nil without candles.
    public static func previousClose(_ daily: [Candle], now: Double) -> Double? {
        guard !daily.isEmpty else { return nil }
        let sorted = daily.sorted { $0.time < $1.time }
        let last = sorted[sorted.count - 1]
        if utcDay(last.time) == utcDay(now) { return sorted.count > 1 ? sorted[sorted.count - 2].close : nil }
        return last.close
    }

    /// Portfolio change since the previous close, from the current prices.
    public static func dailyChange(_ p: RiskPortfolio, daily: [String: [Candle]], now: Double) -> DailyChange? {
        var change = 0.0
        var covered = 0
        for l in p.lines {
            guard let price = l.price, let prev = previousClose(daily[l.key] ?? [], now: now), prev > 0 else { continue }
            change += l.quantity * (price - prev)
            covered += 1
        }
        guard covered > 0 else { return nil }
        let before = p.total - change
        return DailyChange(change: change, percent: before > 0 ? change / before * 100 : 0, covered: covered, lines: p.lines.count)
    }

    // MARK: Limits of the user's settings

    static func fr(_ v: Double, _ d: Int = 1) -> String { JSFormat.fr(v, max: d) }
    static func usd0(_ v: Double) -> String { "\(fr(abs(v), 0)) $" }

    /// Loss (USD) if the stop of the line is hit: the user's stop when set, else the protective stop of the analysis.
    private static func lossAtStop(_ l: RiskLine, userStop: Double?) -> Double? {
        let stop = (userStop ?? 0) > 0 ? userStop : l.protectiveStop
        guard let price = l.price, let stop else { return nil }
        return max(0, l.quantity * (price - stop))
    }

    /// Checks of the portfolio against the settings: loss at the stop per asset vs risk per idea, weight per asset vs
    /// maximum, crypto share vs cap, correlated clusters above twice the maximum weight, today's loss vs the daily limit.
    /// `stops`: the user's stop per line id (nil: each line's own `userStop`).
    public static func checkLimits(_ p: RiskPortfolio, _ s: RiskSettings, clusters: [Cluster], daily d: DailyChange?, stops: [String: Double]? = nil) -> [LimitCheck] {
        var out: [LimitCheck] = []
        guard !p.lines.isEmpty, p.total > 0 else { return out }
        var order: [String] = []
        var assets: [String: (symbol: String, weight: Double, loss: Double?)] = [:]
        for l in p.lines {
            let loss = lossAtStop(l, userStop: stops.map { $0[l.id] } ?? l.userStop)
            let prev = assets[l.key]
            if prev == nil { order.append(l.key) }
            let total: Double? = prev?.loss == nil && loss == nil ? nil : (prev?.loss ?? 0) + (loss ?? 0)
            assets[l.key] = (l.symbol, (prev?.weight ?? 0) + l.weight, total)
        }
        let list = order.compactMap { assets[$0] }

        let budget = p.total * s.riskPerTradePercent / 100
        let risky = list.filter { $0.loss.map { $0 > budget * 1.0001 } ?? false }
        let riskLabel = "Risque par ligne (max \(fr(s.riskPerTradePercent, 2)) %)"
        out.append(risky.isEmpty
            ? LimitCheck(code: "risk_per_trade", level: .ok, label: riskLabel, detail: "Aucune ligne ne perdrait plus de \(usd0(budget)) à son stop.")
            : LimitCheck(code: "risk_per_trade", level: .warning, label: riskLabel,
                         detail: "Si le stop était touché, \(risky.map { "\($0.symbol) coûterait \(usd0($0.loss!)) (\(fr($0.loss! / p.total * 100)) %)" }.joined(separator: ", ")) : plus que votre risque accepté par idée (\(usd0(budget))). Réduisez la ligne ou rapprochez le stop."))

        let heavy = list.filter { $0.weight > s.maxPositionPercent }
        let weightLabel = "Poids max d'une ligne (\(fr(s.maxPositionPercent, 0)) %)"
        out.append(!heavy.isEmpty && list.count > 1
            ? LimitCheck(code: "max_weight", level: .danger, label: weightLabel,
                         detail: "\(heavy.map { "\($0.symbol) \(fr($0.weight)) %" }.joined(separator: ", ")) : au-dessus de votre maximum. Surexposition à un seul actif.")
            : LimitCheck(code: "max_weight", level: .ok, label: weightLabel, detail: "Aucune ligne au-dessus de votre maximum."))

        let crypto = p.cryptoShare
        let cryptoLabel = "Part crypto (max \(fr(s.maxCryptoPercent, 0)) %)"
        out.append(crypto > s.maxCryptoPercent
            ? LimitCheck(code: "crypto_cap", level: .warning, label: cryptoLabel,
                         detail: "\(fr(crypto)) % du patrimoine en crypto, au-dessus de votre plafond : un repli des cryptos toucherait tout le portefeuille.")
            : LimitCheck(code: "crypto_cap", level: .ok, label: cryptoLabel, detail: "\(fr(crypto)) % du patrimoine en crypto."))

        let clusterMax = min(100, s.maxPositionPercent * 2)
        let over = clusters.filter { $0.weight > clusterMax }
        let clusterLabel = "Actifs corrélés (groupe max \(fr(clusterMax, 0)) %)"
        out.append(!over.isEmpty
            ? LimitCheck(code: "cluster", level: .warning, label: clusterLabel,
                         detail: over.map { "\($0.symbols.joined(separator: " + ")) : \(fr($0.weight)) % du patrimoine, corrélation moyenne \(String(format: "%.2f", $0.averageCorrelation)) sur \($0.days) jours" }.joined(separator: " ; ")
                            + ". Ils se comportent comme une seule grosse ligne.")
            : LimitCheck(code: "cluster", level: .ok, label: clusterLabel,
                         detail: clusters.isEmpty ? "Aucun groupe d'actifs corrélés à plus de 0,7."
                            : "Groupe(s) corrélé(s) sous le seuil : \(clusters.map { "\($0.symbols.joined(separator: " + ")) \(fr($0.weight)) %" }.joined(separator: " ; "))."))

        let label = "Perte du jour (max \(fr(s.dailyLossLimitPercent, 2)) %)"
        if let d {
            let partial = d.covered < d.lines ? " (\(d.covered) ligne(s) sur \(d.lines) mesurées)" : ""
            let text = "\(d.change >= 0 ? "+" : "−")\(usd0(d.change)) (\(d.percent >= 0 ? "+" : "−")\(fr(abs(d.percent), 2)) %) depuis la clôture de la veille\(partial)."
            out.append(d.percent <= -s.dailyLossLimitPercent
                ? LimitCheck(code: "daily_loss", level: .danger, label: label, detail: "\(dailyLossReached) \(text)")
                : LimitCheck(code: "daily_loss", level: .ok, label: label, detail: text))
        } else {
            out.append(LimitCheck(code: "daily_loss", level: .na, label: label, detail: "Clôture de la veille indisponible : variation du jour inconnue."))
        }
        return out
    }

    // MARK: Positions that became dangerous

    /// A line is dangerous when its price broke the user's stop, is within one daily ATR (14) of it, or when its
    /// unrealised loss exceeds the risk accepted per idea (% of the portfolio).
    /// `stops`: the user's stop per line id (nil: each line's own `userStop`).
    public static func dangerousPositions(_ p: RiskPortfolio, _ s: RiskSettings, daily: [String: [Candle]], stops: [String: Double]? = nil) -> [Danger] {
        var out: [Danger] = []
        let budget = p.total * s.riskPerTradePercent / 100
        for l in p.lines {
            var reasons: [Danger.Reason] = []
            let stop = stops.map { $0[l.id] } ?? l.userStop
            let candles = daily[l.key] ?? []
            let range = candles.isEmpty ? nil : atr(candles).last ?? nil
            if let price = l.price, let stop, stop > 0 {
                if price <= stop {
                    reasons.append(.init(code: .stopBroken, text: "Stop cassé : cours \(fr(price, 4)) $ sous votre stop \(fr(stop, 4)) $."))
                } else if let range, range > 0, price - stop <= range {
                    reasons.append(.init(code: .nearStop, text: "À moins d'une volatilité journalière (ATR \(fr(range, 4)) $) de votre stop \(fr(stop, 4)) $."))
                }
            }
            if l.price != nil, let invested = l.invested {
                let loss = invested - l.value
                if loss > budget && budget > 0 {
                    reasons.append(.init(code: .lossOverRisk,
                                         text: "Perte latente de \(usd0(loss)) (\(fr(loss / p.total * 100)) % du patrimoine), au-delà de votre risque accepté par idée (\(fr(s.riskPerTradePercent, 2)) %)."))
                }
            }
            if !reasons.isEmpty { out.append(Danger(id: l.id, symbol: l.symbol, kind: l.kind, name: l.name, reasons: reasons)) }
        }
        return out
    }
}
