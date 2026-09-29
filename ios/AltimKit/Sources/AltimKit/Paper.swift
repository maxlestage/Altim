import Foundation

// Paper trading ("Simulation"): a virtual portfolio that follows Altim's decisions with no real money and no order
// ever placed. Exact port of web/src/engine/paper.ts, checked on the same reference scenario
// (Tests/AltimKitTests/Fixtures/paper-fixture.json, written by web/test/paper-scenario.ts).
//
// Rules (the same on the web, iPhone and Android):
// - every order pays fees (0,1 %) and slippage (0,05 %, against you) on each side;
// - a position with a stop or a target is closed automatically when a daily candle AFTER the opening reaches it;
//   a candle that reaches both counts as the stop (worst case); a gap below the stop is filled at the open,
//   a target at the target (never better than planned);
// - the candle of the opening day is not used (its low may be earlier than the purchase);
// - results are grouped by the decision shown at the purchase.

public enum PaperReason: String, Codable, Sendable, CaseIterable {
    case stop, target, manual
    public var label: String {
        switch self {
        case .stop: return "stop touché"
        case .target: return "objectif atteint"
        case .manual: return "vente manuelle"
        }
    }
}

/// The decision shown when the position was opened.
public struct PaperDecision: Codable, Sendable, Hashable {
    public var verdict: String
    public var label: String
    public var confidence: Double
    public var asOf: Double
    public init(verdict: String, label: String, confidence: Double, asOf: Double) {
        self.verdict = verdict
        self.label = label
        self.confidence = confidence
        self.asOf = asOf
    }
}

public struct PaperPosition: Codable, Sendable, Hashable, Identifiable {
    public var id: String
    public var symbol: String
    public var kind: Kind
    public var name: String
    public var openedAt: Double
    /// Fill price (market price + slippage).
    public var entry: Double
    public var quantity: Double
    /// Amount taken from the cash, fees included.
    public var invested: Double
    public var stop: Double?
    public var target: Double?
    public var decision: PaperDecision?

    public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name) }
    /// "kind:SYMBOL", the key of the prices and candles.
    public var key: String { "\(kind.rawValue):\(symbol)" }

    public init(id: String, symbol: String, kind: Kind, name: String, openedAt: Double, entry: Double, quantity: Double,
                invested: Double, stop: Double?, target: Double?, decision: PaperDecision?) {
        self.id = id
        self.symbol = symbol
        self.kind = kind
        self.name = name
        self.openedAt = openedAt
        self.entry = entry
        self.quantity = quantity
        self.invested = invested
        self.stop = stop
        self.target = target
        self.decision = decision
    }

    enum CodingKeys: String, CodingKey { case id, symbol, kind, name, openedAt, entry, quantity, invested, stop, target, decision }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(symbol, forKey: .symbol)
        try c.encode(kind, forKey: .kind)
        try c.encode(name, forKey: .name)
        try c.encode(openedAt, forKey: .openedAt)
        try c.encode(entry, forKey: .entry)
        try c.encode(quantity, forKey: .quantity)
        try c.encode(invested, forKey: .invested)
        // Written as null like the web (same saved shape everywhere).
        try c.encode(stop, forKey: .stop)
        try c.encode(target, forKey: .target)
        try c.encode(decision, forKey: .decision)
    }
}

/// A closed position: the position's fields plus the exit.
public struct PaperTrade: Codable, Sendable, Hashable, Identifiable {
    public var id: String
    public var symbol: String
    public var kind: Kind
    public var name: String
    public var openedAt: Double
    public var entry: Double
    public var quantity: Double
    public var invested: Double
    public var stop: Double?
    public var target: Double?
    public var decision: PaperDecision?
    public var closedAt: Double
    /// Fill price (market price − slippage, or the stop / gap open).
    public var exit: Double
    public var reason: PaperReason
    /// Cash received, fees deducted.
    public var proceeds: Double
    public var pnl: Double
    public var pnlPct: Double

    public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name) }

    public init(_ p: PaperPosition, closedAt: Double, exit: Double, reason: PaperReason, proceeds: Double, pnl: Double, pnlPct: Double) {
        id = p.id
        symbol = p.symbol
        kind = p.kind
        name = p.name
        openedAt = p.openedAt
        entry = p.entry
        quantity = p.quantity
        invested = p.invested
        stop = p.stop
        target = p.target
        decision = p.decision
        self.closedAt = closedAt
        self.exit = exit
        self.reason = reason
        self.proceeds = proceeds
        self.pnl = pnl
        self.pnlPct = pnlPct
    }

    enum CodingKeys: String, CodingKey {
        case id, symbol, kind, name, openedAt, entry, quantity, invested, stop, target, decision, closedAt, exit, reason, proceeds, pnl, pnlPct
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(symbol, forKey: .symbol)
        try c.encode(kind, forKey: .kind)
        try c.encode(name, forKey: .name)
        try c.encode(openedAt, forKey: .openedAt)
        try c.encode(entry, forKey: .entry)
        try c.encode(quantity, forKey: .quantity)
        try c.encode(invested, forKey: .invested)
        try c.encode(stop, forKey: .stop)
        try c.encode(target, forKey: .target)
        try c.encode(decision, forKey: .decision)
        try c.encode(closedAt, forKey: .closedAt)
        try c.encode(exit, forKey: .exit)
        try c.encode(reason, forKey: .reason)
        try c.encode(proceeds, forKey: .proceeds)
        try c.encode(pnl, forKey: .pnl)
        try c.encode(pnlPct, forKey: .pnlPct)
    }
}

public struct PaperState: Codable, Sendable, Hashable {
    public var version: Int
    public var startCapital: Double
    public var cash: Double
    public var startedAt: Double
    public var positions: [PaperPosition]
    public var trades: [PaperTrade]

    public init(version: Int = 1, startCapital: Double, cash: Double, startedAt: Double, positions: [PaperPosition], trades: [PaperTrade]) {
        self.version = version
        self.startCapital = startCapital
        self.cash = cash
        self.startedAt = startedAt
        self.positions = positions
        self.trades = trades
    }
}

public struct PaperOrder: Codable, Sendable {
    public var id: String
    public var symbol: String
    public var kind: Kind
    public var name: String
    /// Market price now.
    public var price: Double
    /// Amount to invest, fees included.
    public var amount: Double
    public var stop: Double?
    public var target: Double?
    public var decision: PaperDecision?

    public init(id: String, symbol: String, kind: Kind, name: String, price: Double, amount: Double,
                stop: Double? = nil, target: Double? = nil, decision: PaperDecision? = nil) {
        self.id = id
        self.symbol = symbol
        self.kind = kind
        self.name = name
        self.price = price
        self.amount = amount
        self.stop = stop
        self.target = target
        self.decision = decision
    }
}

/// An open position valued at a price (nil fields: no price, valued at cost).
public struct PaperLine: Codable, Sendable, Identifiable {
    public var position: PaperPosition
    public var price: Double?
    public var value: Double?
    public var pnl: Double?
    public var pnlPct: Double?
    public var id: String { position.id }

    enum CodingKeys: String, CodingKey { case price, value, pnl, pnlPct }

    public init(position: PaperPosition, price: Double?, value: Double?, pnl: Double?, pnlPct: Double?) {
        self.position = position
        self.price = price
        self.value = value
        self.pnl = pnl
        self.pnlPct = pnlPct
    }

    /// Flat, like the web's `{ ...position, price, value, pnl, pnlPct }`.
    public init(from decoder: Decoder) throws {
        position = try PaperPosition(from: decoder)
        let c = try decoder.container(keyedBy: CodingKeys.self)
        price = try c.decodeIfPresent(Double.self, forKey: .price)
        value = try c.decodeIfPresent(Double.self, forKey: .value)
        pnl = try c.decodeIfPresent(Double.self, forKey: .pnl)
        pnlPct = try c.decodeIfPresent(Double.self, forKey: .pnlPct)
    }

    public func encode(to encoder: Encoder) throws {
        try position.encode(to: encoder)
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(price, forKey: .price)
        try c.encode(value, forKey: .value)
        try c.encode(pnl, forKey: .pnl)
        try c.encode(pnlPct, forKey: .pnlPct)
    }
}

public struct PaperValuation: Codable, Sendable {
    public var cash: Double
    /// Value of the open positions at the given prices (sale fees and slippage deducted, as if sold now).
    public var positionsValue: Double
    public var equity: Double
    public var pnl: Double
    public var pnlPct: Double
    public var lines: [PaperLine]
    /// Positions without a price: valued at their cost.
    public var unpriced: Int
}

public struct PaperVerdictStats: Codable, Sendable, Hashable {
    public var verdict: String
    public var label: String
    public var trades: Int
    public var wins: Int
    public var winRate: Double
    public var avgPnlPct: Double
}

public struct PaperReasonCounts: Codable, Sendable, Hashable {
    public var stop: Int
    public var target: Int
    public var manual: Int
}

public struct PaperStats: Codable, Sendable {
    public var trades: Int
    public var wins: Int
    public var winRate: Double
    public var avgWinPct: Double?
    public var avgLossPct: Double?
    /// Sum of gains ÷ sum of losses (nil without a losing trade).
    public var profitFactor: Double?
    public var realizedPnl: Double
    public var best: PaperTrade?
    public var worst: PaperTrade?
    /// Worst fall of the realized capital (start + closed trades in order), %.
    public var maxDrawdownPct: Double
    public var byReason: PaperReasonCounts
    /// Results grouped by the decision shown at the purchase ("none" / "Sans décision" when opened without one).
    public var byVerdict: [PaperVerdictStats]
}

public enum Paper {
    public static let feeRate = 0.001
    public static let slippage = 0.0005
    public static let defaultCapital = 10_000.0

    private static let powers: [Double] = [1, 10, 100, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10]

    /// JavaScript's `Math.round` (halves toward +∞, where Swift's `.rounded()` goes away from zero: −2.5 → −2, not −3).
    static func jsRound(_ x: Double) -> Double {
        guard x.isFinite else { return x }
        let f = x.rounded(.down)
        // x − floor(x) is exact in binary floating point.
        return x - f >= 0.5 ? f + 1 : f
    }

    /// `Math.round(v * 10 ** d) / 10 ** d`, as on the web.
    static func round(_ v: Double, _ d: Int = 8) -> Double {
        let p = powers[d]
        return jsRound(v * p) / p
    }

    /// A number written like JavaScript's `String(n)` (10000, 1234.5) for the error texts.
    static func jsString(_ v: Double) -> String {
        if v.isFinite, v == v.rounded(), abs(v) < 1e15 { return String(Int64(v)) }
        return "\(v)"
    }

    public static func new(capital: Double, now: Double) -> PaperState {
        let c = capital.isFinite && capital > 0 ? round(capital, 2) : defaultCapital
        return PaperState(startCapital: c, cash: c, startedAt: now, positions: [], trades: [])
    }

    /// Buys `amount` $ of the asset at `price` + slippage, fees deducted. Error text when refused.
    public static func open(_ s: PaperState, _ o: PaperOrder, now: Double) -> (state: PaperState, error: String?) {
        if !(o.price > 0) || !o.price.isFinite { return (s, "Prix indisponible.") }
        if !(o.amount > 0) || !o.amount.isFinite { return (s, "Montant invalide.") }
        if o.amount > s.cash + 1e-9 { return (s, "Liquidités simulées insuffisantes (\(Money.moneyFmt(s.cash, sep: " ") { jsString(round($0, 2)) }) disponibles).") }
        let entry = o.price * (1 + slippage)
        let stop = o.stop.flatMap { $0 > 0 && $0 < entry ? $0 : nil }
        let target = o.target.flatMap { $0 > entry ? $0 : nil }
        let fee = o.amount * feeRate
        let position = PaperPosition(
            id: o.id, symbol: o.symbol, kind: o.kind, name: o.name, openedAt: now,
            entry: round(entry), quantity: round((o.amount - fee) / entry, 10), invested: round(o.amount, 2),
            stop: stop, target: target, decision: o.decision
        )
        var next = s
        next.cash = round(s.cash - o.amount, 2)
        next.positions.append(position)
        return (next, nil)
    }

    private static func close(_ s: PaperState, _ p: PaperPosition, fill: Double, at: Double, reason: PaperReason) -> PaperState {
        let gross = p.quantity * fill
        let proceeds = round(gross - gross * feeRate, 2)
        let pnl = round(proceeds - p.invested, 2)
        let trade = PaperTrade(p, closedAt: at, exit: round(fill), reason: reason, proceeds: proceeds, pnl: pnl, pnlPct: round((pnl / p.invested) * 100, 4))
        var next = s
        next.cash = round(s.cash + proceeds, 2)
        next.positions = s.positions.filter { $0.id != p.id }
        next.trades.append(trade)
        return next
    }

    /// Sells the whole position at the market price − slippage, fees deducted.
    public static func close(_ s: PaperState, id: String, price: Double, now: Double) -> (state: PaperState, error: String?) {
        guard let p = s.positions.first(where: { $0.id == id }) else { return (s, "Position introuvable.") }
        if !(price > 0) || !price.isFinite { return (s, "Prix indisponible.") }
        return (close(s, p, fill: price * (1 - slippage), at: now, reason: .manual), nil)
    }

    /// Stable sort (like JavaScript's `Array.prototype.sort`).
    private static func stableSorted<T>(_ list: [T], by less: (T, T) -> Bool) -> [T] {
        list.enumerated().sorted { a, b in
            if less(a.element, b.element) { return true }
            if less(b.element, a.element) { return false }
            return a.offset < b.offset
        }.map(\.element)
    }

    /// Closes the positions whose stop or target was reached by a daily candle that started after the opening
    /// (candles by "kind:SYMBOL", any order). Returns the new state and the trades just closed.
    public static func checkExits(_ s: PaperState, candles: [String: [Candle]]) -> (state: PaperState, closed: [PaperTrade]) {
        var state = s
        var closed: [PaperTrade] = []
        for p in s.positions {
            if p.stop == nil && p.target == nil { continue }
            let list = stableSorted((candles[p.key] ?? []).filter { $0.time > p.openedAt && $0.low > 0 && $0.high >= $0.low }) { $0.time < $1.time }
            for c in list {
                var fill: Double?
                var reason: PaperReason?
                if let stop = p.stop, c.low <= stop {
                    fill = min(stop, c.open) * (1 - slippage)
                    reason = .stop
                } else if let target = p.target, c.high >= target {
                    fill = target * (1 - slippage)
                    reason = .target
                }
                if let fill, let reason {
                    state = close(state, p, fill: fill, at: c.time, reason: reason)
                    closed.append(state.trades[state.trades.count - 1])
                    break
                }
            }
        }
        return (state, closed)
    }

    /// Values the open positions at the given prices ("kind:SYMBOL"), as if sold now; without a price, at cost.
    public static func valuation(_ s: PaperState, prices: [String: Double]) -> PaperValuation {
        var positionsValue = 0.0
        var unpriced = 0
        let lines = s.positions.map { p -> PaperLine in
            guard let price = prices[p.key], price > 0 else {
                unpriced += 1
                positionsValue += p.invested
                return PaperLine(position: p, price: nil, value: nil, pnl: nil, pnlPct: nil)
            }
            let gross = p.quantity * price * (1 - slippage)
            let value = round(gross - gross * feeRate, 2)
            positionsValue += value
            let pnl = round(value - p.invested, 2)
            return PaperLine(position: p, price: price, value: value, pnl: pnl, pnlPct: round((pnl / p.invested) * 100, 4))
        }
        let equity = round(s.cash + positionsValue, 2)
        let pnl = round(equity - s.startCapital, 2)
        return PaperValuation(cash: s.cash, positionsValue: round(positionsValue, 2), equity: equity, pnl: pnl,
                              pnlPct: round((pnl / s.startCapital) * 100, 4), lines: lines, unpriced: unpriced)
    }

    /// Like JavaScript's `localeCompare` on the verdict keys (ASCII identifiers): letters first compared without case.
    static func localeLess(_ a: String, _ b: String) -> Bool {
        let la = a.lowercased(), lb = b.lowercased()
        if la != lb { return la < lb }
        // Same letters: lower case first, as ICU does.
        return a > b
    }

    public static func stats(_ s: PaperState) -> PaperStats {
        let t = stableSorted(s.trades) { $0.closedAt < $1.closedAt }
        let wins = t.filter { $0.pnl > 0 }
        let losses = t.filter { $0.pnl <= 0 }
        func mean(_ v: [Double]) -> Double? { v.isEmpty ? nil : round(v.reduce(0, +) / Double(v.count), 4) }
        let gains = wins.reduce(0) { $0 + $1.pnl }
        let lost = -losses.reduce(0) { $0 + $1.pnl }
        var capital = s.startCapital
        var peak = capital
        var maxDd = 0.0
        for x in t {
            capital += x.pnl
            peak = max(peak, capital)
            maxDd = min(maxDd, (capital / peak - 1) * 100)
        }
        var order: [String] = []
        var groups: [String: [PaperTrade]] = [:]
        for x in t {
            let k = x.decision?.verdict ?? "none"
            if groups[k] == nil { order.append(k) }
            groups[k, default: []].append(x)
        }
        let unsorted = order.map { verdict -> PaperVerdictStats in
            let list = groups[verdict]!
            let w = list.filter { $0.pnl > 0 }.count
            return PaperVerdictStats(
                verdict: verdict, label: list[0].decision?.label ?? "Sans décision", trades: list.count, wins: w,
                winRate: round((Double(w) / Double(list.count)) * 100, 4), avgPnlPct: mean(list.map(\.pnlPct))!
            )
        }
        let byVerdict = stableSorted(unsorted) { a, b in a.trades != b.trades ? a.trades > b.trades : localeLess(a.verdict, b.verdict) }
        let byPct = stableSorted(t) { $0.pnlPct > $1.pnlPct }
        return PaperStats(
            trades: t.count,
            wins: wins.count,
            winRate: t.isEmpty ? 0 : round((Double(wins.count) / Double(t.count)) * 100, 4),
            avgWinPct: mean(wins.map(\.pnlPct)),
            avgLossPct: mean(losses.map(\.pnlPct)),
            profitFactor: lost > 0 ? round(gains / lost, 4) : nil,
            realizedPnl: round(t.reduce(0) { $0 + $1.pnl }, 2),
            best: byPct.first,
            worst: byPct.last,
            maxDrawdownPct: round(maxDd, 4),
            byReason: PaperReasonCounts(stop: t.filter { $0.reason == .stop }.count, target: t.filter { $0.reason == .target }.count,
                                        manual: t.filter { $0.reason == .manual }.count),
            byVerdict: byVerdict
        )
    }

    /// Accepts only a well-formed saved state (the file can be edited by hand), like the web's `isPaperState`.
    public static func isValid(_ s: PaperState) -> Bool {
        s.version == 1 && s.startCapital.isFinite && s.startCapital > 0 && s.cash.isFinite
            && s.positions.allSatisfy { $0.quantity > 0 && $0.entry > 0 && $0.invested > 0 }
    }

    /// Reads a saved state: nil when the JSON is not a valid state.
    public static func decode(_ data: Data) -> PaperState? {
        guard let s = try? JSONDecoder().decode(PaperState.self, from: data), isValid(s) else { return nil }
        return s
    }
}
