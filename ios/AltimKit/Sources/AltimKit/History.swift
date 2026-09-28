import Foundation

/// Daily closes served by /api/history: [time, close] pairs of each held asset, plus Bitcoin and SPY.
public struct HistoryResponse: Codable, Sendable {
    public struct Series: Codable, Sendable {
        public var symbol: String
        public var kind: Kind
        public var closes: [[Double]]
        public var error: String?
        public var id: String { "\(kind.rawValue):\(symbol)" }
    }

    public var asOf: Double
    public var days: Int
    public var series: [Series]

    public var byId: [String: [(Double, Double)]] {
        Dictionary(series.map { s in (s.id, s.closes.filter { $0.count == 2 }.map { ($0[0], $0[1]) }) }, uniquingKeysWith: { a, _ in a })
    }
}

/// History of the portfolio: what the lines held today were worth each day of the period, compared with Bitcoin and
/// the S&P 500 held over the same days. Past purchases and sales are not known: it answers "how did what I own now
/// behave", not "how did my account do". Same computation as the site (web/src/engine/history.ts) and Android.
public struct PortfolioHistory: Sendable {
    public struct Point: Sendable, Identifiable { public var t: Double; public var value: Double; public var id: Double { t } }
    public struct Day: Sendable { public var t: Double; public var change: Double }
    public struct Benchmark: Sendable, Identifiable {
        public var id: String
        public var label: String
        public var change: Double
        public var pct: [Double]
    }

    public var points: [Point]
    public var change: Double
    public var maxDrawdown: Double
    public var best: Day?
    public var worst: Day?
    public var benchmarks: [Benchmark]
    public var missing: [String]
    public var shortened: Bool

    /// Change in % since the first day, one value per point.
    public var pct: [Double] { points.map { ($0.value / points[0].value - 1) * 100 } }

    public static let benchmarkList = [("crypto:BTC", "Bitcoin"), ("stock:SPY", "S&P 500 (SPY)")]
    private static let day = 86_400_000.0
    private static func dayOf(_ t: Double) -> Double { (t / day).rounded(.down) * day }

    /// Last close known at the end of each day of the grid (weekends and holidays keep the Friday close).
    private static func onGrid(_ closes: [(Double, Double)], _ grid: [Double]) -> [Double?] {
        let sorted = closes.filter { $0.1.isFinite && $0.1 > 0 }.sorted { $0.0 < $1.0 }
        var i = 0
        var last: Double?
        return grid.map { d in
            while i < sorted.count && dayOf(sorted[i].0) <= d { last = sorted[i].1; i += 1 }
            return last
        }
    }

    public static func compute(_ lines: [(String, Double)], series: [String: [(Double, Double)]], days: Int, now: Date = Date()) -> PortfolioHistory? {
        let nowMs = now.timeIntervalSince1970 * 1000
        let held = lines.filter { $0.1 > 0 }
        // The curve ends on the last closed day of the held assets (only closed candles are served).
        let lastTimes = held.flatMap { series[$0.0]?.map(\.0) ?? [] }.filter { $0 <= nowMs }
        let end = dayOf(lastTimes.max() ?? nowMs)
        let grid = (0...days).map { end - Double(days - $0) * day }
        var missing: [String] = []
        var valued: [(Double, [Double?])] = []
        for (id, q) in held {
            let g = onGrid(series[id] ?? [], grid)
            if g.last! == nil { missing.append(id) } else { valued.append((q, g)) }
        }
        guard !valued.isEmpty else { return nil }
        // The curve starts on the first day every line has a price: an asset listed later would look like a gain.
        guard let start = grid.indices.first(where: { i in valued.allSatisfy { $0.1[i] != nil } }), grid.count - start >= 2 else { return nil }
        let points = (start..<grid.count).map { i in Point(t: grid[i], value: valued.reduce(0) { $0 + $1.0 * $1.1[i]! }) }

        var peak = points[0].value
        var maxDrawdown = 0.0
        var best: Day?
        var worst: Day?
        for (i, p) in points.enumerated() {
            peak = max(peak, p.value)
            maxDrawdown = min(maxDrawdown, (p.value / peak - 1) * 100)
            if i > 0 {
                let c = (p.value / points[i - 1].value - 1) * 100
                if best == nil || c > best!.change { best = Day(t: p.t, change: c) }
                if worst == nil || c < worst!.change { worst = Day(t: p.t, change: c) }
            }
        }
        let benchmarks: [Benchmark] = benchmarkList.compactMap { id, label in
            let g = Array(onGrid(series[id] ?? [], grid)[start...])
            guard let base = g.first ?? nil, !g.contains(where: { $0 == nil }) else { return nil }
            let pct = g.map { ($0! / base - 1) * 100 }
            return Benchmark(id: id, label: label, change: pct.last!, pct: pct)
        }
        return PortfolioHistory(
            points: points,
            change: (points.last!.value / points[0].value - 1) * 100,
            maxDrawdown: maxDrawdown,
            best: best.flatMap { $0.change > 0 ? $0 : nil },
            worst: worst.flatMap { $0.change < 0 ? $0 : nil },
            benchmarks: benchmarks,
            missing: missing,
            shortened: start > 0
        )
    }
}

extension AltimClient {
    /// Daily closes of these assets over 30, 90 or 365 days, plus Bitcoin and SPY (the quantities stay on the phone).
    public func history(_ assets: [Asset], days: Int) async throws -> HistoryResponse {
        var seen = Set<String>()
        return try await getHistory(Array(assets.filter { seen.insert($0.id).inserted }.prefix(20)), days: days)
    }
}

/// News worth a notification: a serious escalation (war declared, invasion, bank run…) told by at least 2 sources (an
/// opinion piece that "could" trigger a bank run is told by one), or a story about one of the user's assets told by at
/// least 3 sources. Only stories of the last 6 hours; a story already notified (same article,
/// or a title with the same words told by another source) never notifies again for 48 hours. Same rule on Android.
public struct NewsAlertTracker: Codable, Sendable {
    public struct Seen: Codable, Sendable { public var id: String; public var words: [String]; public var time: Date }

    public var seen: [Seen] = []
    public init() {}

    public static let freshness: TimeInterval = 6 * 3600
    public static let memory: TimeInterval = 48 * 3600
    /// Sources telling a story about one of the user's assets before it is notified (the first one + 2).
    public static let minOtherSources = 2

    static func words(_ title: String) -> Set<String> {
        Set(title.lowercased().components(separatedBy: CharacterSet.alphanumerics.inverted).filter { $0.count >= 4 })
    }

    private static func similar(_ a: Set<String>, _ b: Set<String>) -> Bool {
        guard !a.isEmpty, !b.isEmpty else { return false }
        return Double(a.intersection(b).count) / Double(a.union(b).count) >= 0.5
    }

    /// Why this story is worth a notification ("alert" or "asset"), or nil.
    public static func reason(_ item: NewsItem, owned: Set<String>) -> String? {
        if item.alert && !item.alsoIn.isEmpty { return "alert" }
        if item.alsoIn.count >= minOtherSources && item.assets.contains(where: owned.contains) { return "asset" }
        return nil
    }

    public mutating func newAlerts(_ items: [NewsItem], owned: Set<String>, now: Date = Date()) -> [NewsItem] {
        seen.removeAll { now.timeIntervalSince($0.time) >= Self.memory }
        var out: [NewsItem] = []
        for item in items {
            guard now.timeIntervalSince1970 * 1000 - item.time <= Self.freshness * 1000, Self.reason(item, owned: owned) != nil else { continue }
            let w = Self.words(item.title)
            if seen.contains(where: { $0.id == item.id || Self.similar(Set($0.words), w) }) { continue }
            seen.append(Seen(id: item.id, words: w.sorted(), time: now))
            out.append(item)
        }
        return out
    }
}
