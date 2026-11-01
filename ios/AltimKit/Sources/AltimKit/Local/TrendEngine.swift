import Foundation

/// Altim's verdict engine on the iPhone, chosen on measured results (Tests/AltimKitTests/StrategyLab*): on 26 assets
/// (12 cryptos over 1 000 days, 14 stocks over 3.5 years), each history cut in two independent halves, the former
/// composite signal (trend + oscillators, fixed target) lost to holding on 21 assets out of 26. What held up:
///
/// - Per asset: be in only while the background trend is up (close above its 200-day EMA) and the momentum positive
///   (60-day return for cryptos, 90 days — 62 sessions — for stocks). It does not beat holding in a bull market, but
///   it cut the worst outcomes in half (worst quarter of the assets −30 % instead of −54 %).
/// - Best moment to enter: a short pull-back inside that trend (2-day RSI under 10); alone, the rule beat holding
///   risk-adjusted in 46 cases out of 50.
/// - Across assets (`Rotation.sleeves`): each asset a fixed share, invested only in its trend. On two different lists
///   of assets: cryptos +23 % / +15 % against −14 % / −20 % held, worst fall −44 % against −73 %; stocks, in a strong
///   bull market, +35 % / +31 % against +85 % / +80 %, worst fall −13 % against −20 %. A protection, not a promise. Keeping
///   only the 3 strongest looked better on one list and failed on the other: not used.
public enum TrendEngine {
    public enum State: String, Codable, Sendable {
        /// Trend up, momentum positive, and a short pull-back: the best moment to buy.
        case pullback
        /// Trend up and momentum positive: buy or keep.
        case trend
        /// Mixed: above the trend but momentum negative, or the reverse. Keep a position, do not open one.
        case mixed
        /// Below the trend and momentum negative: stay out, or exit.
        case down
        /// Fewer than 210 daily candles.
        case unknown
    }

    public struct Verdict: Codable, Sendable, Equatable {
        public var state: State
        public var action: Action
        /// −100 … 100: the momentum (in % over the lookback) squashed, positive in an up trend.
        public var score: Double
        /// 0 … 100: how many of the three conditions agree (trend, momentum, short trend), as the radar's confidence.
        public var confidence: Double
        public var momentum: Double?
        public var lookback: Int
        public var ema200: Double?
        public var rsi2: Double?
        public var text: String
        /// Last daily close the verdict was computed on.
        public var price: Double? = nil
        /// Return over the next 20 days predicted by the trained bot (BotEngine), when it could be trained.
        public var bot: Double? = nil
        /// Worst fall within the next 20 days predicted by the risk bot (≤ 0), when it could be trained.
        public var botRisk: Double? = nil
        /// When the trend bot was trained (ms) and on how many examples, to show how fresh it is.
        public var botTrainedAt: Double? = nil
        public var botExamples: Int? = nil
        /// Whether the asset's market (Bitcoin for cryptos, SPY for stocks) is itself in an up trend with a positive
        /// momentum; nil when its candles could not be read.
        public var market: Bool? = nil
    }

    /// The asset that stands for a whole market in the market filter.
    public static func benchmark(_ kind: Kind) -> Asset {
        kind == .crypto ? Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin") : Asset(symbol: "SPY", kind: .stock, name: "S&P 500 (SPY)")
    }

    /// Whether a market is in an up trend (its benchmark above its 200-day average with a positive momentum), nil when
    /// its history is too short.
    public static func marketUp(_ raw: [Bar], kind: Kind) -> Bool? {
        let c = Indicators.sanitize(raw)
        guard c.count >= 210 else { return nil }
        let st = state(Series(c), c.count - 1, lookback: lookback(kind))
        return st == .unknown ? nil : st == .trend || st == .pullback
    }

    /// The market's state (`marketUp`) on each UTC day of its candles, in time order.
    static func marketDays(_ raw: [Bar], kind: Kind) -> [(day: Double, up: Bool)] {
        let c = Indicators.sanitize(raw)
        guard c.count >= 210 else { return [] }
        let s = Series(c), n = lookback(kind)
        return c.indices.compactMap { i in
            let st = state(s, i, lookback: n)
            return st == .unknown ? nil : ((c[i].time / 86_400_000).rounded(.down), st == .trend || st == .pullback)
        }
    }

    /// The market filter: an asset is bought or kept only while its whole market is in an up trend too. Measured in the
    /// lab (testMarketFilter, both lists, both halves): better or equal return and a smaller worst fall in all 8 cases,
    /// e.g. crypto +55 % / −37 % against +46 % / −48 %, average +30 % / +41 % against +24 % / +30 %. Never creates a buy.
    public static func applyMarket(_ up: Bool?, kind: Kind, to v: Verdict) -> Verdict {
        var v = v
        v.market = up
        guard up == false, v.state == .trend || v.state == .pullback, v.action == .buy || v.action == .strongBuy else { return v }
        let name = kind == .crypto ? "Le Bitcoin (le marché crypto)" : "Le S&P 500 (le marché actions, via SPY)"
        v.action = .sell
        v.text = "\(name) n'est pas en tendance haussière : sortir ou rester à l'écart malgré la tendance de l'actif (filtre de marché). " + v.text
        return v
    }

    /// 60 days for cryptos (open every day), 62 sessions (≈ 90 days) for stocks.
    public static func lookback(_ kind: Kind) -> Int { kind == .crypto ? 60 : 62 }

    struct Series {
        let close: [Double]
        let ema20: [Double?], ema50: [Double?], ema200: [Double?], rsi2: [Double?]
        init(_ c: [Bar]) {
            close = c.map(\.close)
            ema20 = Indicators.ema(close, 20)
            ema50 = Indicators.ema(close, 50)
            ema200 = Indicators.ema(close, 200)
            rsi2 = Indicators.rsi(close, 2)
        }
        func momentum(_ i: Int, _ n: Int) -> Double? { i >= n ? close[i] / close[i - n] - 1 : nil }
    }

    static func state(_ s: Series, _ i: Int, lookback: Int) -> State {
        guard let e = s.ema200[i], let m = s.momentum(i, lookback) else { return .unknown }
        let up = s.close[i] > e
        if up && m > 0 { return (s.rsi2[i] ?? 50) < 10 ? .pullback : .trend }
        if !up && m < 0 { return .down }
        return .mixed
    }

    /// Verdict on the last closed daily candle.
    public static func verdict(_ raw: [Bar], kind: Kind) -> Verdict {
        let c = Indicators.sanitize(raw)
        let n = lookback(kind)
        guard c.count >= 210 else {
            return Verdict(state: .unknown, action: .hold, score: 0, confidence: 0, momentum: nil, lookback: n, ema200: nil, rsi2: nil,
                           text: "Historique trop court (\(c.count) jours, il en faut 210) : pas de verdict.")
        }
        let s = Series(c)
        let i = c.count - 1
        let st = state(s, i, lookback: n)
        let m = s.momentum(i, n) ?? 0
        let e = s.ema200[i] ?? s.close[i]
        let shortUp = (s.ema20[i] ?? 0) > (s.ema50[i] ?? 0)
        let agree = [s.close[i] > e, m > 0, shortUp].filter { $0 }.count
        let confidence = st == .down ? Double(3 - agree) / 3 * 100 : Double(agree) / 3 * 100
        let score = 100 * tanh(m * 2.5) * (st == .trend || st == .pullback ? 1 : st == .down ? 1 : 0.5)
        let days = kind == .crypto ? "\(n) jours" : "3 mois"
        let gap = (s.close[i] / e - 1) * 100
        let mom = "\(m >= 0 ? "+" : "−")\(JSFormat.fr(abs(m) * 100, min: 0, max: 1)) % sur \(days)"
        let trend = "\(gap >= 0 ? "au-dessus" : "en dessous") de sa moyenne 200 jours (\(gap >= 0 ? "+" : "−")\(JSFormat.fr(abs(gap), min: 0, max: 1)) %)"
        let text: String
        let action: Action
        switch st {
        case .pullback:
            action = .strongBuy
            text = "Tendance de fond haussière (prix \(trend)), momentum positif (\(mom)) et repli de court terme : en moyenne une meilleure entrée sur la semaine suivante (mesuré sur les deux listes d'actifs du laboratoire), sans avantage net à un mois."
        case .trend:
            action = .buy
            text = "Tendance de fond haussière (prix \(trend)) et momentum positif (\(mom)) : acheter ou conserver. Un repli de quelques jours serait une meilleure entrée."
        case .mixed:
            action = .hold
            text = "Signaux contraires : prix \(trend), momentum \(mom). Conserver une position, ne pas en ouvrir."
        case .down:
            action = .sell
            text = "Tendance de fond baissière (prix \(trend)) et momentum négatif (\(mom)) : rester à l'écart ou sortir."
        case .unknown:
            action = .hold
            text = "Pas de verdict."
        }
        return Verdict(state: st, action: action, score: score, confidence: confidence, momentum: m, lookback: n, ema200: s.ema200[i],
                       rsi2: s.rsi2[i], text: text, price: s.close[i])
    }

    // MARK: Backtest of the per-asset rule

    /// "Si j'avais suivi Altim": in while the state is `trend` or `pullback`, out on `down` or when the trend or the
    /// momentum turns; orders filled at the next open, 0.1 % fees. Starts after 200 days (the 200-day average).
    public static func backtest(_ raw: [Bar], asset: Asset, reliability: Reliability? = nil, market: [Bar]? = nil) -> BacktestReport {
        let c = Indicators.sanitize(raw)
        let n = lookback(asset.kind)
        let up = market.map { marketDays($0, kind: asset.kind) } ?? []
        let start = 200
        func empty(_ e: String) -> BacktestReport {
            BacktestReport(symbol: asset.symbol, kind: asset.kind, from: c.first?.time ?? 0, to: c.last?.time ?? 0, days: c.count,
                           strategyReturn: 0, holdReturn: 0, strategyDrawdown: 0, holdDrawdown: 0, trades: [], winRate: nil,
                           exposure: 0, curve: [], reliability: reliability, error: e)
        }
        guard c.count >= start + 60 else { return empty("Il faut au moins \(start + 60) jours d'historique (\(c.count) disponibles).") }
        let s = Series(c)
        var cash = 100.0, units = 0.0
        var entry: (time: Double, price: Double, cost: Double)?
        var pendingBuy = false, pendingSell = false
        var trades: [BacktestReport.Trade] = [], curve: [BacktestReport.Point] = []
        var invested = 0
        let holdUnits = 100 / c[start].open
        var peakS = 100.0, peakH = 100.0, ddS = 0.0, ddH = 0.0
        var m = 0
        for i in start..<c.count {
            let bar = c[i]
            // The market's state on the last market day up to this one (no market candles: the rule alone).
            let day = (bar.time / 86_400_000).rounded(.down)
            while m + 1 < up.count && up[m + 1].day <= day { m += 1 }
            let marketOK = up.isEmpty || up[m].day > day || up[m].up
            if pendingSell, let e = entry {
                cash = units * bar.open * (1 - Backtest.fee)
                trades.append(.init(entryTime: e.time, entryPrice: e.price, exitTime: bar.time, exitPrice: bar.open, exit: "signal", result: (cash / e.cost - 1) * 100))
                units = 0
                entry = nil
            }
            if pendingBuy, entry == nil {
                entry = (bar.time, bar.open, cash)
                units = cash * (1 - Backtest.fee) / bar.open
                cash = 0
            }
            pendingBuy = false
            pendingSell = false
            if entry != nil { invested += 1 }
            if i < c.count - 1 {
                let st = state(s, i, lookback: n)
                let inTrend = (st == .trend || st == .pullback) && marketOK
                if entry == nil && inTrend { pendingBuy = true }
                if entry != nil && !inTrend && st != .unknown {
                    // Out when the trend or the momentum turns (both are needed to be in).
                    pendingSell = true
                }
            }
            let vS = cash + units * bar.close, vH = holdUnits * bar.close
            peakS = max(peakS, vS); peakH = max(peakH, vH)
            ddS = min(ddS, (vS / peakS - 1) * 100); ddH = min(ddH, (vH / peakH - 1) * 100)
            curve.append(.init(time: bar.time, strategy: vS, hold: vH))
        }
        if let e = entry, let last = c.last {
            trades.append(.init(entryTime: e.time, entryPrice: e.price, exitTime: last.time, exitPrice: last.close, exit: "open",
                                result: (units * last.close * (1 - Backtest.fee) / e.cost - 1) * 100))
        }
        let wins = trades.filter { $0.result > 0 }.count
        return BacktestReport(symbol: asset.symbol, kind: asset.kind, from: c[start].time, to: c[c.count - 1].time, days: c.count - start,
                              strategyReturn: (curve.last?.strategy ?? 100) - 100, holdReturn: (curve.last?.hold ?? 100) - 100,
                              strategyDrawdown: ddS, holdDrawdown: ddH, trades: trades,
                              winRate: trades.isEmpty ? nil : Double(wins) / Double(trades.count) * 100,
                              exposure: Double(invested) / Double(c.count - start) * 100, curve: Backtest.sample(curve, 90),
                              reliability: reliability, error: nil)
    }
}

// MARK: - Rotation ("Sélection Altim")

/// « Sélection Altim » of a market: every asset of a fixed list with its verdict; those in their up trend are to hold,
/// each for a fixed share (1 / `top` of the money), the rest in cash; `test` replays it (`Rotation.sleeves`).
public struct RotationReport: Codable, Sendable {
    public struct Pick: Codable, Sendable, Identifiable {
        public var symbol: String
        public var kind: Kind
        public var name: String
        public var price: Double?
        public var momentum: Double?
        /// Close vs its 200-day EMA, in %.
        public var trendGap: Double?
        public var rank: Int
        public var selected: Bool
        /// "pullback", "trend", "mixed", "down", "unknown" (TrendEngine.State).
        public var state: String
        public var reason: String
        public var id: String { "\(kind.rawValue):\(symbol)" }
        public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name) }
    }

    public struct Test: Codable, Sendable {
        public var from: Double
        public var to: Double
        /// The rotation and the equal-weight basket of the same assets, in % (whole period and its two halves).
        public var ret: Double, basket: Double, drawdown: Double, basketDrawdown: Double
        public var firstHalf: Double, firstHalfBasket: Double, secondHalf: Double, secondHalfBasket: Double
        public var curve: [BacktestReport.Point]
    }

    public var kind: Kind
    public var asOf: Double
    public var top: Int
    public var lookback: Int
    public var picks: [Pick]
    public var scanned: Int
    public var test: Test?
    public var notes: [String]

    public var selected: [Pick] { picks.filter(\.selected) }
}

public enum Rotation {
    public static let top = 3
    /// Days between two reviews.
    public static let every = 7

    /// Liquid assets of each market, fixed in advance (not chosen on their past results).
    public static let universe: [Kind: [Asset]] = [
        .crypto: [("BTC", "Bitcoin"), ("ETH", "Ethereum"), ("SOL", "Solana"), ("XRP", "XRP"), ("BNB", "BNB"), ("ADA", "Cardano"),
                  ("DOGE", "Dogecoin"), ("LINK", "Chainlink"), ("AVAX", "Avalanche"), ("LTC", "Litecoin"), ("DOT", "Polkadot"),
                  ("ATOM", "Cosmos"), ("TRX", "TRON"), ("XLM", "Stellar"), ("BCH", "Bitcoin Cash")].map { Asset(symbol: $0.0, kind: .crypto, name: $0.1) },
        .stock: [("AAPL", "Apple"), ("MSFT", "Microsoft"), ("NVDA", "NVIDIA"), ("AMZN", "Amazon"), ("GOOGL", "Alphabet"), ("META", "Meta"),
                 ("TSLA", "Tesla"), ("AVGO", "Broadcom"), ("JPM", "JPMorgan Chase"), ("V", "Visa"), ("UNH", "UnitedHealth"),
                 ("XOM", "ExxonMobil"), ("JNJ", "Johnson & Johnson"), ("PG", "Procter & Gamble"), ("KO", "Coca-Cola"),
                 ("COST", "Costco"), ("WMT", "Walmart"), ("NFLX", "Netflix"), ("AMD", "AMD"), ("LLY", "Eli Lilly")].map { Asset(symbol: $0.0, kind: .stock, name: $0.1) },
    ]

    /// Ranking on the last candle of each history.
    /// `verdicts` (by asset id): the app's verdicts with the trend bot's veto, so the Selection never holds an asset
    /// its own page tells to leave; the rule alone for an asset without one.
    public static func picks(_ data: [(Asset, [Bar])], kind: Kind, prices: [String: Double] = [:],
                             verdicts: [String: TrendEngine.Verdict] = [:]) -> [RotationReport.Pick] {
        let n = TrendEngine.lookback(kind)
        let rows = data.map { a, bars -> (Asset, TrendEngine.Verdict, Double?) in
            let v = verdicts[a.id] ?? TrendEngine.verdict(bars, kind: kind)
            let last = Indicators.sanitize(bars).last?.close
            return (a, v, v.ema200.flatMap { e in last.map { ($0 / e - 1) * 100 } })
        }
        // Every asset in its up trend is held (fixed share each): picking only the strongest concentrated the money and did
        // not hold up when the list of assets changed (StrategyLabTests.testReviewFrequency).
        let chosen = Set(rows.filter { $0.1.effectiveState == .trend || $0.1.effectiveState == .pullback }.map(\.0.id))
        let ranked = rows.sorted { a, b in
            let ea = a.1.effectiveState == .trend || a.1.effectiveState == .pullback
            let eb = b.1.effectiveState == .trend || b.1.effectiveState == .pullback
            if ea != eb { return ea }
            return (a.1.momentum ?? -.infinity) > (b.1.momentum ?? -.infinity)
        }
        let days = kind == .crypto ? "\(n) jours" : "3 mois"
        return ranked.enumerated().map { i, r in
            let (a, v, gap) = r
            let selected = chosen.contains(a.id)
            let mom = v.momentum.map { "\($0 >= 0 ? "+" : "−")\(JSFormat.fr(abs($0) * 100, min: 0, max: 0)) % sur \(days)" } ?? "momentum inconnu"
            let reason: String
            let vetoed = (v.state == .trend || v.state == .pullback) && v.effectiveState != v.state
            let bot = v.bot.map { "\($0 >= 0 ? "+" : "−")\(JSFormat.fr(abs($0) * 100, min: 1, max: 1)) %" } ?? ""
            switch vetoed ? .unknown : v.state {
            case .unknown where vetoed && v.market == false:
                reason = "Tendance haussière (\(mom)), mais \(kind == .crypto ? "le Bitcoin" : "le S&P 500 (SPY)") n'est pas en tendance haussière : à sortir (filtre de marché)."
            case .unknown where vetoed:
                reason = v.effectiveState == .down
                    ? "Tendance haussière (\(mom)), mais le bot Tendance prévoit \(bot) sur 20 jours : à sortir."
                    : "Tendance haussière (\(mom)), mais le bot Tendance prévoit \(bot) sur 20 jours : pas de nouvel achat."
            case .pullback, .trend:
                reason = v.state == .pullback ? "Tendance de fond haussière (\(mom)) et léger repli : bon moment pour entrer." : "Tendance de fond haussière (\(mom)) : à détenir."
            case .mixed: reason = "Signaux contraires (\(mom)) : pas retenu."
            case .down: reason = "Tendance de fond baissière (\(mom)) : écarté."
            case .unknown: reason = "Historique insuffisant."
            }
            return RotationReport.Pick(symbol: a.symbol, kind: a.kind, name: a.name, price: prices[a.id] ?? Indicators.sanitize(data.first { $0.0.id == a.id }?.1 ?? []).last?.close,
                                       momentum: v.momentum, trendGap: gap, rank: i + 1, selected: selected, state: v.state.rawValue, reason: reason)
        }
    }

    /// The rule replayed on the histories (weekly review, 0.1 % per change), against the equal-weight basket.
    public static func test(_ data: [(Asset, [Bar])], kind: Kind, top: Int = Rotation.top, every: Int = Rotation.every) -> RotationReport.Test? {
        let series = data.map { Indicators.sanitize($0.1) }.filter { $0.count >= 260 }
        guard series.count >= min(top, 3) + 2 else { return nil }
        let n = TrendEngine.lookback(kind)
        let s = series.map { TrendEngine.Series($0) }
        let index = series.map { c in Dictionary(c.enumerated().map { ($0.element.time, $0.offset) }, uniquingKeysWith: { a, _ in a }) }
        // An asset joins the rule and the basket once it has its 200-day average; the test starts when half of them
        // have (one young asset must not cut the period of all the others).
        let ready = series.map { $0[200].time }
        let start = ready.sorted()[ready.count / 2]
        let dates = Array(Set(series.flatMap { $0.map(\.time) })).filter { $0 >= start }.sorted()
        guard dates.count >= 60 else { return nil }
        let mid = dates[dates.count / 2]
        var value = 100.0, basket = 100.0, peak = 100.0, peakB = 100.0, dd = 0.0, ddB = 0.0
        var valueMid = 100.0, basketMid = 100.0
        var held: [Int] = []
        var last = series.map { _ in -1 }
        var curve: [BacktestReport.Point] = []
        for (k, t) in dates.enumerated() {
            var rets: [Int: Double] = [:]
            for a in series.indices {
                guard let i = index[a][t] else { continue }
                if last[a] >= 0 { rets[a] = series[a][i].close / series[a][last[a]].close - 1 }
                last[a] = i
            }
            let live = series.indices.filter { last[$0] >= 0 && ready[$0] <= t }
            if !held.isEmpty { value *= 1 + held.map { rets[$0] ?? 0 }.reduce(0, +) / Double(held.count) }
            basket *= 1 + live.map { rets[$0] ?? 0 }.reduce(0, +) / Double(max(1, live.count))
            if k % every == 0 {
                let chosen = series.indices.compactMap { a -> (Int, Double)? in
                    let i = last[a]
                    guard i >= 0, ready[a] <= t, TrendEngine.state(s[a], i, lookback: n) == .trend || TrendEngine.state(s[a], i, lookback: n) == .pullback,
                          let m = s[a].momentum(i, n) else { return nil }
                    return (a, m)
                }.sorted { $0.1 > $1.1 }.prefix(top).map(\.0)
                value *= 1 - Backtest.fee * Double(Set(chosen).symmetricDifference(Set(held)).count) / Double(max(1, max(chosen.count, held.count)))
                held = chosen
            }
            if t == mid { valueMid = value; basketMid = basket }
            peak = max(peak, value); peakB = max(peakB, basket)
            dd = min(dd, (value / peak - 1) * 100); ddB = min(ddB, (basket / peakB - 1) * 100)
            curve.append(.init(time: t, strategy: value, hold: basket))
        }
        return .init(from: dates[0], to: dates[dates.count - 1], ret: value - 100, basket: basket - 100, drawdown: dd, basketDrawdown: ddB,
                     firstHalf: valueMid - 100, firstHalfBasket: basketMid - 100,
                     secondHalf: (value / valueMid - 1) * 100, secondHalfBasket: (basket / basketMid - 1) * 100,
                     curve: Backtest.sample(curve, 90))
    }

    /// Fixed shares: each asset gets 1/N of the money, invested only while its own verdict is in the trend (daily, the
    /// per-asset rule), in cash otherwise; against the same assets all held (1/N each, bought at the start). The assets
    /// with a 200-day average at the start take part.
    public static func sleeves(_ data: [(Asset, [Bar])], kind: Kind, market: [Bar]? = nil) -> RotationReport.Test? {
        let up = market.map { TrendEngine.marketDays($0, kind: kind) } ?? []
        var m = 0
        let series = data.map { Indicators.sanitize($0.1) }.filter { $0.count >= 260 }
        guard series.count >= 3 else { return nil }
        let n = TrendEngine.lookback(kind)
        let ready = series.map { $0[200].time }
        let start = ready.sorted()[ready.count / 2]
        let members = series.indices.filter { ready[$0] <= start }
        let s = members.map { TrendEngine.Series(series[$0]) }
        let c = members.map { series[$0] }
        let index = c.map { bars in Dictionary(bars.enumerated().map { ($0.element.time, $0.offset) }, uniquingKeysWith: { a, _ in a }) }
        let dates = Array(Set(c.flatMap { $0.map(\.time) })).filter { $0 >= start }.sorted()
        guard dates.count >= 60 else { return nil }
        let share = 100 / Double(members.count)
        var cash = [Double](repeating: share, count: members.count), units = [Double](repeating: 0, count: members.count)
        var pending = [Int](repeating: 0, count: members.count) // 1 buy, −1 sell at the next open
        var holdUnits = [Double](repeating: 0, count: members.count)
        var last = [Int](repeating: -1, count: members.count)
        var peak = 100.0, peakB = 100.0, dd = 0.0, ddB = 0.0, mid = (100.0, 100.0)
        var curve: [BacktestReport.Point] = []
        for t in dates {
            // The market filter (TrendEngine.applyMarket) on the last market day up to this one; none: the rule alone.
            let day = (t / 86_400_000).rounded(.down)
            while m + 1 < up.count && up[m + 1].day <= day { m += 1 }
            let marketOK = up.isEmpty || up[m].day > day || up[m].up
            for a in members.indices {
                guard let i = index[a][t] else { continue }
                let bar = c[a][i]
                if holdUnits[a] == 0 { holdUnits[a] = share / bar.open }
                if pending[a] == 1, units[a] == 0 { units[a] = cash[a] * (1 - Backtest.fee) / bar.open; cash[a] = 0 }
                if pending[a] == -1, units[a] > 0 { cash[a] = units[a] * bar.open * (1 - Backtest.fee); units[a] = 0 }
                pending[a] = 0
                let st = TrendEngine.state(s[a], i, lookback: n)
                let inTrend = (st == .trend || st == .pullback) && marketOK
                if units[a] == 0 && inTrend { pending[a] = 1 }
                if units[a] > 0 && !inTrend && st != .unknown { pending[a] = -1 }
                last[a] = i
            }
            let value = members.indices.reduce(0.0) { v, a in v + cash[a] + units[a] * (last[a] >= 0 ? c[a][last[a]].close : 0) }
            let basket = members.indices.reduce(0.0) { v, a in v + (last[a] >= 0 ? holdUnits[a] * c[a][last[a]].close : share) }
            if t == dates[dates.count / 2] { mid = (value, basket) }
            peak = max(peak, value); peakB = max(peakB, basket)
            dd = min(dd, (value / peak - 1) * 100); ddB = min(ddB, (basket / peakB - 1) * 100)
            curve.append(.init(time: t, strategy: value, hold: basket))
        }
        let end = curve.last!
        return .init(from: dates[0], to: dates[dates.count - 1], ret: end.strategy - 100, basket: end.hold - 100, drawdown: dd, basketDrawdown: ddB,
                     firstHalf: mid.0 - 100, firstHalfBasket: mid.1 - 100,
                     secondHalf: (end.strategy / mid.0 - 1) * 100, secondHalfBasket: (end.hold / mid.1 - 1) * 100,
                     curve: Backtest.sample(curve, 90))
    }

    public static let notes = [
        "Chaque actif de la liste a une part fixe du budget. Elle est investie seulement quand sa tendance de fond est haussière et son momentum positif, et que tout le marché l'est aussi (le Bitcoin pour les cryptos, le S&P 500 pour les actions) ; gardée en liquidités sinon. Vérifié chaque jour.",
        "C'est une règle de protection : elle a fortement réduit les pertes dans les marchés baissiers, mais elle gagne moins que tout garder dans une forte hausse.",
        "Le test rejoue la règle et son filtre de marché sur l'historique de ces mêmes actifs, sans voir le lendemain, avec 0,1 % de frais par ordre. Au labo, ce filtre a nettement aidé les cryptos (ex. +40 % contre +23 % sans lui, pire baisse −38 % contre −44 %) ; pour les actions, mieux sur la première moitié de l'historique mais un peu moins bien sur la seconde (+19 % contre +21 %).",
        "La liste d'actifs est fixée à l'avance, mais ce sont des actifs connus aujourd'hui : un test sur le passé ne garantit rien pour la suite. Altim ne passe aucun ordre.",
    ]
}

extension TrendEngine.Verdict {
    /// The background picture the advice really gives: an up trend vetoed by the market filter or the trend bot reads as mixed signals
    /// (ATTENDRE) or as a way out (VENTE), never as a buy.
    public var effectiveState: TrendEngine.State {
        guard state == .trend || state == .pullback else { return state }
        switch action {
        case .hold: return .mixed
        case .sell, .strongSell: return .down
        default: return state
        }
    }

    /// The verdict as the app says it.
    public var label: String {
        if (state == .trend || state == .pullback) && (action == .sell || action == .strongSell) {
            return market == false ? VerdictLog.marketExitLabel : VerdictLog.botExitLabel
        }
        switch state {
        case .pullback: return action == .hold ? "ATTENDRE" : "ACHETER (REPLI)"
        case .trend: return action == .hold ? "ATTENDRE" : "ACHETER / CONSERVER"
        case .mixed: return "CONSERVER SANS RENFORCER"
        case .down: return "RESTER À L'ÉCART"
        case .unknown: return "PAS DE VERDICT"
        }
    }
}

extension ConfigChanges {
    /// The verdict seen on the iPhone through the same "changement de configuration" diff as the server's decisions.
    /// A pull-back and the trend are one configuration (a short pull-back comes and goes within days, and the buy
    /// alerts already tell it): only a change of the background picture is a transition. Unknown verdicts are ignored.
    public static func apply(_ state: ConfigState, verdict v: TrendEngine.Verdict, asset: Asset, now: Double) -> (state: ConfigState, transition: ConfigTransition?) {
        let side: (Decision.Verdict, Decision.Level, String, String)
        // An up trend held back by the market filter or the trend bot: the asset's own trend is not what changed.
        let vetoed = (v.state == .trend || v.state == .pullback) && v.effectiveState != v.state
        let market = asset.kind == .crypto ? "le Bitcoin" : "le S&P 500"
        switch v.effectiveState {
        case .pullback, .trend: side = (.buyZone, .moderate, "ACHETER / CONSERVER", "Tendance haussière")
        case .mixed: side = (.wait, .waiting, "CONSERVER SANS RENFORCER", vetoed ? "Achat bloqué par le bot Tendance" : "Signaux contraires")
        case .down:
            side = (.noPosition, .exit, vetoed ? v.label : "RESTER À L'ÉCART",
                    !vetoed ? "Tendance baissière" : v.market == false ? "Marché hors tendance haussière" : "Sortie conseillée par le bot Tendance")
        case .unknown: return (state, nil)
        }
        let triggers: [String]
        if vetoed && v.market == false {
            triggers = ["Retour à l'achat quand \(market) repasse en tendance haussière (au-dessus de sa moyenne 200 jours, momentum positif), si l'actif y est encore"]
        } else if vetoed {
            triggers = ["Retour à l'achat quand le bot Tendance ne prévoit plus de baisse sur 20 jours, si la tendance de l'actif tient"]
        } else {
            triggers = v.ema200.map { e -> String in
                side.0 == .buyZone ? "Sortie si le prix repasse sous \(Money.money(e)) (moyenne 200 jours), si le momentum devient négatif ou si \(market) quitte sa tendance haussière"
                    : "Retour à l'achat quand le prix repasse au-dessus de \(Money.money(e)) (moyenne 200 jours) avec un momentum positif"
            }.map { [$0] } ?? []
        }
        let next = ConfigSnapshot(verdict: side.0, level: side.1, label: side.2, levelLabel: side.3, missing: [], triggers: triggers,
                                  at: now, rating: nil, ratingLabel: nil, metrics: nil)
        let k = key(kind: asset.kind, symbol: asset.symbol, personal: false)
        let t = diff(state.last[k], next, symbol: asset.symbol, kind: asset.kind, name: asset.name, personal: false)
        var s = state
        s.version = stateVersion
        s.last[k] = next
        if let t { s.transitions = Array(([t] + s.transitions).prefix(maxTransitions)) }
        return (s, t)
    }
}
