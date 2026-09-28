import Foundation

// Strategy comparator (GET /api/strategies?symbol=&kind=): JSON contract and the pure helpers of the card (colours,
// chart geometry, direct labels, list view), same as web/src/webapp/strategies.ts (same texts, same palette).

public enum StrategyID: String, Codable, Sendable, Hashable, CaseIterable {
    case trend, momentum, breakout, meanReversion, swing, dca, buyHold, value, unknown
    public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
}

public struct StrategyRegimeStat: Codable, Sendable, Hashable {
    /// "bull" | "bear" | "range" | "crisis" | "unknown"
    public var regime: String
    public var label: String
    public var trades: Int
    public var winRate: Double?
    public var avgReturn: Double?
    public var lowSample: Bool
}

public struct StrategyMetrics: Codable, Sendable, Hashable {
    /// % over the period, costs included (DCA: gain ÷ total invested).
    public var totalReturn: Double
    /// % per year, nil under one year (DCA: internal rate of return).
    public var cagr: Double?
    /// %, negative.
    public var maxDrawdown: Double
    public var sharpe: Double?
    public var sortino: Double?
    public var winRate: Double?
    public var profitFactor: Double?
    /// % per trade.
    public var expectancy: Double?
    public var avgR: Double?
    public var trades: Int
    /// % of the tested days invested.
    public var exposure: Double
}

/// [ms, value of 100 invested].
public struct EquityPoint: Codable, Sendable, Hashable {
    public var t: Double
    public var v: Double
    public init(t: Double, v: Double) {
        self.t = t
        self.v = v
    }
    public init(from decoder: Decoder) throws {
        var c = try decoder.unkeyedContainer()
        t = try c.decode(Double.self)
        v = try c.decode(Double.self)
    }
    public func encode(to encoder: Encoder) throws {
        var c = encoder.unkeyedContainer()
        try c.encode(t)
        try c.encode(v)
    }
    public var date: Date { Date(timeIntervalSince1970: t / 1000) }
}

public struct StrategyResult: Codable, Sendable, Hashable, Identifiable {
    public var id: StrategyID
    public var name: String
    public var rule: String
    public var params: String
    public var available: Bool
    public var unavailable: String?
    public var metrics: StrategyMetrics?
    public var regimes: [StrategyRegimeStat]
    /// ≤ 200 points, same dates for every strategy.
    public var equity: [EquityPoint]
    public var lowSample: Bool
    public var note: String?
}

public struct StrategiesReport: Codable, Sendable {
    public var symbol: String
    public var kind: Kind
    public var asOf: Double
    public var from: Double
    public var to: Double
    public var bars: Int
    public var period: String
    public var source: String
    public var feesPct: Double
    public var slippagePct: Double
    public var spreadPct: Double
    public var notes: [String]
    public var strategies: [StrategyResult]

    public func strategy(_ id: StrategyID) -> StrategyResult? { strategies.first { $0.id == id } }
}

public enum Strategies {
    /// Fixed order of the chips and of the cards.
    public static let order: [StrategyID] = [.trend, .momentum, .breakout, .meanReversion, .swing, .dca, .buyHold, .value]
    public static let defaultSelection: [StrategyID] = [.trend, .breakout, .meanReversion, .dca, .buyHold]

    /// One colour per strategy, whatever is selected (colour follows the entity). Palette validated on the card
    /// surface (dark): the four default colours pass all pairs (hence the direct labels too). Buy and hold is the
    /// neutral, dashed reference.
    public static let colorHex: [StrategyID: UInt32] = [
        .trend: 0x3987E5,
        .breakout: 0xC98500,
        .meanReversion: 0xD55181,
        .dca: 0x008300,
        .momentum: 0x9085E9,
        .swing: 0xD95926,
        .value: 0x199E70,
        .buyHold: 0x9AA0B4,
    ]
    public static let reference: StrategyID = .buyHold
    /// Direct labels up to 4 coloured curves (plus the reference).
    public static let maxLabeled = 4

    /// Short names of the direct labels.
    public static let short: [StrategyID: String] = [
        .trend: "Tendance", .momentum: "Momentum", .breakout: "Cassure", .meanReversion: "Moyenne",
        .swing: "Swing", .dca: "DCA", .buyHold: "Conserver", .value: "PER",
    ]

    public static func shortName(_ id: StrategyID) -> String { short[id] ?? id.rawValue }

    static let nnbsp = "\u{202F}"

    /// "+12,3 %" / "−4,0 %" / "—".
    public static func signedPct(_ v: Double?, digits: Int = 1) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(v > 0 ? "+" : v < 0 ? "−" : "")\(JSFormat.fr(abs(v), min: digits, max: digits))\(nnbsp)%"
    }

    public static func plainPct(_ v: Double?, digits: Int = 0) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(JSFormat.fr(v, min: digits, max: digits))\(nnbsp)%"
    }

    public static func ratio(_ v: Double?) -> String {
        guard let v, v.isFinite else { return "—" }
        return "\(v < 0 ? "−" : "")\(JSFormat.fr(abs(v), min: 2, max: 2))"
    }

    /// Value of 100 invested, whole: "1 234".
    public static func value100(_ v: Double) -> String { JSFormat.fr(v, max: 0) }

    /// "20 juil. 2024" (UTC day).
    public static func shortDate(_ t: Double) -> String {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC")!
        let c = cal.dateComponents([.year, .month, .day], from: Date(timeIntervalSince1970: t / 1000))
        return "\(c.day ?? 1) \(Agenda.months[((c.month ?? 1) - 1) % 12]) \(c.year ?? 1970)"
    }

    /// Query of /api/strategies.
    public static func query(symbol: String, kind: Kind) -> [String: String] { ["symbol": symbol, "kind": kind.rawValue] }

    /// Strategies to draw: selected, available, with a curve, in the fixed order.
    public static func drawable(_ report: StrategiesReport, selected: [StrategyID]) -> [StrategyResult] {
        order.compactMap { id in report.strategy(id) }.filter { selected.contains($0.id) && $0.available && $0.equity.count > 1 }
    }

    /// The selected strategies (available or not), in the fixed order: one card each.
    public static func chosen(_ report: StrategiesReport, selected: [StrategyID]) -> [StrategyResult] {
        order.filter { selected.contains($0) }.compactMap { report.strategy($0) }
    }

    /// Chip toggled: removed, or added back in the fixed order.
    public static func toggle(_ id: StrategyID, in selected: [StrategyID]) -> [StrategyID] {
        selected.contains(id) ? selected.filter { $0 != id } : order.filter { $0 == id || selected.contains($0) }
    }

    public struct Box: Sendable {
        public var w, h, l, r, t, b: Double
        public init(w: Double, h: Double, l: Double, r: Double, t: Double, b: Double) {
            self.w = w; self.h = h; self.l = l; self.r = r; self.t = t; self.b = b
        }
    }

    public struct Geometry: Sendable {
        public var lo: Double
        public var hi: Double
        public var n: Int
        let box: Box
        public func x(_ i: Int) -> Double { box.l + Double(i) / Double(n - 1) * (box.w - box.l - box.r) }
        public func y(_ v: Double) -> Double { box.t + (1 - (v - lo) / (hi - lo)) * (box.h - box.t - box.b) }
    }

    /// One axis (value of 100 invested), 100 always inside the range, a little air above and below.
    public static func geometry(_ series: [StrategyResult], box: Box) -> Geometry? {
        let n = series.map(\.equity.count).max() ?? 0
        guard n >= 2 else { return nil }
        let values = series.flatMap { $0.equity.map(\.v) }
        var lo = min(100, values.min() ?? 100)
        var hi = max(100, values.max() ?? 100)
        let spread = (hi - lo) * 0.06
        let pad = spread == 0 ? 5 : spread
        lo -= pad
        hi += pad
        return Geometry(lo: lo, hi: hi, n: n, box: box)
    }

    /// Vertical positions of the end labels, pushed apart by `gap` and kept inside [top, bottom]; same order as `ys`.
    public static func placeLabels(_ ys: [Double], gap: Double, top: Double, bottom: Double) -> [Double] {
        let order = ys.enumerated().sorted { $0.element < $1.element || ($0.element == $1.element && $0.offset < $1.offset) }
        var out = order.map { min(max($0.element, top), bottom) }
        if out.count > 1 { for k in 1..<out.count { out[k] = max(out[k], out[k - 1] + gap) } }
        // Overflow at the bottom: shift the stack up.
        let over = (out.last ?? bottom) - bottom
        if over > 0 { for k in out.indices { out[k] -= over } }
        if out.count > 1 { for k in stride(from: out.count - 2, through: 0, by: -1) { out[k] = min(out[k], out[k + 1] - gap) } }
        var res = [Double](repeating: 0, count: ys.count)
        for (k, o) in order.enumerated() { res[o.offset] = out[k] }
        return res
    }

    /// Point index under a pointer at `px` (box units).
    public static func indexAt(_ px: Double, _ g: Geometry, box: Box) -> Int {
        let raw = (px - box.l) / (box.w - box.l - box.r) * Double(g.n - 1)
        return max(0, min(g.n - 1, Int((raw + 0.5).rounded(.down))))
    }

    /// Index of the point nearest to the instant `t` (ms) in a curve (tap readout).
    public static func nearestIndex(_ equity: [EquityPoint], t: Double) -> Int? {
        guard !equity.isEmpty else { return nil }
        return equity.indices.min { abs(equity[$0].t - t) < abs(equity[$1].t - t) }
    }

    /// List view: value of 100 invested at `steps` evenly spaced dates (first and last included).
    public static func checkpoints(_ s: StrategyResult, steps: Int = 5) -> [EquityPoint] {
        let n = s.equity.count
        guard n > 0 else { return [] }
        let m = min(steps, n)
        var seen = Set<Int>()
        let idx = (0..<m).map { k in Int((Double(k * (n - 1)) / Double(max(1, m - 1)) + 0.5).rounded(.down)) }.filter { seen.insert($0).inserted }
        return idx.map { s.equity[$0] }
    }

    /// "échantillon trop faible (3 trades)" for the signal strategies with fewer than 10 trades.
    public static func lowSampleText(_ s: StrategyResult) -> String? {
        guard s.lowSample else { return nil }
        let n = s.metrics?.trades ?? 0
        return "échantillon trop faible (\(n) trade\(n > 1 ? "s" : ""))"
    }

    /// Metric rows of a strategy's card (label, value, tone: +1 up, −1 down, 0 neutral), same order as the web.
    public static func metricRows(_ s: StrategyResult) -> [(label: String, value: String, tone: Int)] {
        guard let m = s.metrics else { return [] }
        let dca = s.id == .dca
        func tone(_ v: Double?) -> Int { v.map { $0 >= 0 ? 1 : -1 } ?? 0 }
        var rows: [(String, String, Int)] = [
            (dca ? "Gain sur les sommes versées" : "Rendement total", signedPct(m.totalReturn), tone(m.totalReturn)),
            (dca ? "Rendement annuel (TRI)" : "Rendement annuel", m.cagr == nil ? "— (< 1 an)" : signedPct(m.cagr), tone(m.cagr)),
            ("Pire recul", signedPct(m.maxDrawdown), -1),
            ("Sharpe / Sortino", dca ? "non pertinent" : "\(ratio(m.sharpe)) / \(ratio(m.sortino))", 0),
            (dca ? "Achats" : "Trades", String(m.trades), 0),
            ("Temps investi", plainPct(m.exposure), 0),
        ]
        if !dca {
            rows.append(("Taux de réussite", plainPct(m.winRate), 0))
            rows.append(("Profit factor", ratio(m.profitFactor), 0))
            rows.append(("Espérance par trade", signedPct(m.expectancy, digits: 2), tone(m.expectancy)))
        }
        if let r = m.avgR { rows.append(("Multiple de R moyen", "\(ratio(r)) R", 0)) }
        return rows
    }

    /// "Marché haussier" (the label before its bracket) and "11 achats · moy. −17,4 %".
    public static func regimeRow(_ g: StrategyRegimeStat, dca: Bool) -> (label: String, value: String, lowSample: Bool) {
        let label = g.label.components(separatedBy: " (").first ?? g.label
        var value = "\(g.trades) \(dca ? "achat" : "trade")\(g.trades > 1 ? "s" : "")"
        if g.trades > 0 { value += " · moy. \(signedPct(g.avgReturn))" }
        return (label, value, g.trades > 0 && g.lowSample)
    }
}

extension AltimClient {
    /// Strategy comparator of one asset (fixed textbook parameters, daily history).
    public func strategies(_ asset: Asset) async throws -> StrategiesReport {
        try await getStrategies(Strategies.query(symbol: asset.symbol, kind: asset.kind))
    }
}
