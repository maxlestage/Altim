import Foundation

/// "What if I had invested X $ every week / month": regular purchases at the daily close, over real past prices,
/// compared with the same total invested at once on the first day. No fees, no taxes; the past does not predict the
/// future. Same computation as the site (web/src/engine/dca.ts) and Android.
public struct DcaResult: Sendable {
    public struct Step: Sendable, Identifiable { public var t: Double; public var invested: Double; public var value: Double; public var id: Double { t } }

    public var buys: Int
    public var invested: Double
    public var units: Double
    public var value: Double
    public var gain: Double
    public var averagePrice: Double
    public var lastPrice: Double
    public var lumpValue: Double
    public var lumpGain: Double
    public var path: [Step]
    public var first: Double
    public var last: Double

    private static let day = 86_400_000.0

    public static func simulate(_ closes: [(Double, Double)], amount: Double, everyDays: Int, days: Int, now: Date = Date()) -> DcaResult? {
        guard amount > 0, everyDays > 0 else { return nil }
        let nowMs = now.timeIntervalSince1970 * 1000
        let sorted = closes.filter { $0.1 > 0 && $0.0 <= nowMs }.sorted { $0.0 < $1.0 }
        guard sorted.count >= 2 else { return nil }
        let end = sorted.last!.0
        let start = end - Double(days) * day
        let inRange = sorted.filter { $0.0 >= start }
        // The asset must have existed for the whole period, otherwise the comparison is meaningless.
        guard inRange.count >= 2, sorted[0].0 <= start + 7 * day else { return nil }
        var units = 0.0
        var invested = 0.0
        var next = inRange[0].0
        var path: [Step] = []
        for (t, c) in inRange {
            if t >= next {
                units += amount / c
                invested += amount
                next += Double(everyDays) * day
                // Weekends and holidays: the next purchase happens at the next close available.
                while next <= t { next += Double(everyDays) * day }
            }
            path.append(Step(t: t, invested: invested, value: units * c))
        }
        let lastPrice = inRange.last!.1
        let value = units * lastPrice
        let lump = invested / inRange[0].1 * lastPrice
        return DcaResult(buys: Int((invested / amount).rounded()), invested: invested, units: units, value: value, gain: (value / invested - 1) * 100,
                         averagePrice: invested / units, lastPrice: lastPrice, lumpValue: lump, lumpGain: (lump / invested - 1) * 100,
                         path: path, first: inRange[0].0, last: end)
    }
}

/// "Point du jour" (/api/brief): market climate, what can be bought now, moves since the last daily close, top stories.
public struct Brief: Codable, Sendable {
    public struct Market: Codable, Sendable { public var level: String; public var label: String; public var score: Double; public var themes: [String] }
    public struct Buy: Codable, Sendable, Identifiable {
        public var symbol: String
        public var kind: Kind
        public var strong: Bool
        public var title: String
        public var id: String { "\(kind.rawValue):\(symbol)" }
    }
    public struct Mover: Codable, Sendable, Identifiable {
        public var symbol: String
        public var kind: Kind
        public var price: Double
        public var change: Double
        public var id: String { "\(kind.rawValue):\(symbol)" }
        public var asset: Asset { Asset(symbol: symbol, kind: kind, name: symbol) }
    }

    public var asOf: Double
    public var headline: String
    public var market: Market?
    public var buyable: [Buy]
    public var movers: [Mover]
    public var news: [NewsItem]
}

extension AltimClient {
    /// "Point du jour" of these assets (the watch list and the holdings, 20 at most).
    public func brief(_ assets: [Asset]) async throws -> Brief {
        var seen = Set<String>()
        return try await getBrief(Array(assets.filter { seen.insert($0.id).inserted }.prefix(20)))
    }
}
