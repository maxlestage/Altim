import Foundation

// "Opportunités du moment" (GET /api/opportunities?kind=, 202 {pending:true} while the first scan runs) and the
// anomalies of one asset (GET /api/anomalies?symbol=&kind=, with the OKX derivatives block for cryptos): JSON
// contracts and the pure filtering of the scan (category chips, market cap or rank, liquidity, volatility), same as
// web/src/engine/opportunities.ts (same labels, same rules).

public enum OppCategory: String, Codable, Sendable, Hashable, CaseIterable {
    case setup, reversal, breakout, volume, oversold, fundamentals, unknown
    public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }

    /// The categories offered, in order.
    public static let all: [OppCategory] = [.setup, .reversal, .breakout, .volume, .oversold, .fundamentals]

    public var short: String {
        switch self {
        case .setup: return "Configurations"
        case .reversal: return "Retournements"
        case .breakout: return "Cassures"
        case .volume: return "Volume anormal"
        case .oversold: return "Survendus"
        case .fundamentals: return "Fondamentaux"
        case .unknown: return "Autre"
        }
    }
}

public struct OppHit: Codable, Sendable, Hashable {
    public var category: OppCategory
    public var reason: String
    public var strength: Double
    public init(category: OppCategory, reason: String, strength: Double) {
        self.category = category
        self.reason = reason
        self.strength = strength
    }
}

public struct OppItem: Codable, Sendable, Hashable, Identifiable {
    public var symbol: String
    public var name: String
    public var sector: String
    /// Stocks: Nasdaq market cap, USD. Cryptos: nil (rank instead).
    public var marketCap: Double?
    /// Cryptos: CoinGecko market-cap rank.
    public var rank: Int?
    public var price: Double
    public var time: Double
    public var change1d: Double?
    public var rsi14: Double?
    public var volumeRatio: Double?
    /// ATR(14) ÷ price, % per day.
    public var volatility: Double?
    /// Average daily traded value over 20 sessions, USD.
    public var liquidity: Double?
    public var distanceAtr: Double?
    public var hits: [OppHit]
    public var id: String { symbol }

    public init(symbol: String, name: String, sector: String, marketCap: Double?, rank: Int?, price: Double, time: Double, change1d: Double?, rsi14: Double?,
                volumeRatio: Double?, volatility: Double?, liquidity: Double?, distanceAtr: Double?, hits: [OppHit]) {
        self.symbol = symbol
        self.name = name
        self.sector = sector
        self.marketCap = marketCap
        self.rank = rank
        self.price = price
        self.time = time
        self.change1d = change1d
        self.rsi14 = rsi14
        self.volumeRatio = volumeRatio
        self.volatility = volatility
        self.liquidity = liquidity
        self.distanceAtr = distanceAtr
        self.hits = hits
    }
}

public struct OppCategoryInfo: Codable, Sendable, Identifiable {
    public var id: OppCategory
    public var label: String
    public var rule: String
    public var analyzed: Int
    public var note: String?
    public var error: String?
}

public struct OppNotCovered: Codable, Sendable, Hashable {
    public var label: String
    public var reason: String
}

public struct OpportunityReport: Codable, Sendable {
    public var kind: Kind
    public var asOf: Double
    public var scanned: Int
    public var universe: String
    public var topN: Int
    public var categories: [OppCategoryInfo]
    public var items: [OppItem]
    public var notCovered: [OppNotCovered]
    public var source: String
}

public enum OpportunitiesResult: Sendable {
    case ready(OpportunityReport)
    /// The first scan is still running (≈ 30 s): ask again.
    case pending
}

public struct OppFilters: Codable, Sendable, Equatable {
    public var categories: [OppCategory]
    /// Stocks: minimum market cap (USD).
    public var minCap: Double?
    /// Cryptos: best rank allowed (top N by market cap).
    public var maxRank: Int?
    /// Minimum average daily traded value (USD).
    public var minLiquidity: Double?
    /// Maximum daily volatility (ATR %).
    public var maxVolatility: Double?

    public init(categories: [OppCategory] = OppCategory.all, minCap: Double? = nil, maxRank: Int? = nil, minLiquidity: Double? = nil, maxVolatility: Double? = nil) {
        self.categories = categories
        self.minCap = minCap
        self.maxRank = maxRank
        self.minLiquidity = minLiquidity
        self.maxVolatility = maxVolatility
    }

    public static let defaults = OppFilters()
}

/// Market and filters kept on the iPhone between openings.
public struct OppSaved: Codable, Sendable, Equatable {
    public var market: Kind
    public var filters: OppFilters
    public init(market: Kind = .stock, filters: OppFilters = .defaults) {
        self.market = market
        self.filters = filters
    }
}

public enum Opportunities {
    public static let caps: [(label: String, value: Double?)] = [("Toutes", nil), ("≥ 10 Md$", 1e10), ("≥ 50 Md$", 5e10), ("≥ 200 Md$", 2e11)]
    public static let ranks: [(label: String, value: Int?)] = [("Tous", nil), ("Top 20", 20), ("Top 50", 50), ("Top 100", 100)]
    public static let liquidity: [(label: String, value: Double?)] = [("Toutes", nil), ("≥ 1 M$ / jour", 1e6), ("≥ 10 M$ / jour", 1e7), ("≥ 100 M$ / jour", 1e8)]
    public static let volatility: [(label: String, value: Double?)] = [("Toutes", nil), ("≤ 2 % / jour", 2), ("≤ 4 % / jour", 4), ("≤ 8 % / jour", 8)]

    /// Whether an asset passes the numeric filters (an unknown value fails a filter that is set).
    public static func passes(_ item: OppItem, _ f: OppFilters) -> Bool {
        if let min = f.minCap, !(item.marketCap.map { $0 >= min } ?? false) { return false }
        if let max = f.maxRank, !(item.rank.map { $0 <= max } ?? false) { return false }
        if let min = f.minLiquidity, !(item.liquidity.map { $0 >= min } ?? false) { return false }
        if let max = f.maxVolatility, !(item.volatility.map { $0 <= max } ?? false) { return false }
        return true
    }

    /// Assets that pass the filters, with only the hits of the chosen categories; the most hits (then strongest) first.
    public static func filter(_ items: [OppItem], _ f: OppFilters) -> [OppItem] {
        let wanted = Set(f.categories)
        func best(_ i: OppItem) -> Double { max(0, i.hits.map(\.strength).max() ?? 0) }
        return items.filter { passes($0, f) }
            .map { i -> OppItem in
                var x = i
                x.hits = i.hits.filter { wanted.contains($0.category) }
                return x
            }
            .filter { !$0.hits.isEmpty }
            .enumerated()
            .sorted { a, b in
                if a.element.hits.count != b.element.hits.count { return a.element.hits.count > b.element.hits.count }
                if best(a.element) != best(b.element) { return best(a.element) > best(b.element) }
                return a.offset < b.offset
            }
            .map(\.element)
    }

    /// Number of assets per category once the numeric filters are applied (for the chips).
    public static func countByCategory(_ items: [OppItem], _ f: OppFilters) -> [OppCategory: Int] {
        var out = Dictionary(uniqueKeysWithValues: OppCategory.all.map { ($0, 0) })
        for i in items where passes(i, f) {
            for c in Set(i.hits.map(\.category)) where out[c] != nil { out[c]! += 1 }
        }
        return out
    }

    /// Saved market and filters; an unreadable or older value gives the defaults, unknown categories are dropped.
    public static func parseSaved(_ data: Data?) -> OppSaved {
        guard let data, let s = try? JSONDecoder().decode(OppSaved.self, from: data) else { return OppSaved() }
        var x = s
        x.filters.categories = s.filters.categories.filter { OppCategory.all.contains($0) }
        return x
    }

    /// "12,3 M$", "850 k$", "420 $".
    public static func compactUsd(_ v: Double) -> String {
        let a = abs(v)
        func f(_ x: Double, _ d: Int) -> String { JSFormat.fr(x, max: d) }
        if a >= 1e9 { return "\(f(v / 1e9, 1)) Md$" }
        if a >= 1e6 { return "\(f(v / 1e6, 1)) M$" }
        if a >= 1e3 { return "\(f(v / 1e3, 0)) k$" }
        return "\(f(v, 0)) $"
    }

    /// "+1,2 %" / "−0,4 %" (a zero is "+0 %", as on the web).
    public static func signed(_ v: Double, _ d: Int = 1) -> String { "\(v >= 0 ? "+" : "−")\(JSFormat.fr(abs(v), max: d)) %" }

    /// "RSI 14 : 28 · volatilité 2,1 %/j · échangé 12,3 M$/j · capitalisation 1,2 Md$".
    public static func metrics(_ i: OppItem) -> [String] {
        var out: [String] = []
        if let r = i.rsi14 { out.append("RSI 14 : \(Int((r + 0.5).rounded(.down)))") }
        if let v = i.volatility { out.append("volatilité \(JSFormat.fr(v, max: 1)) %/j") }
        if let l = i.liquidity { out.append("échangé \(compactUsd(l))/j") }
        if let c = i.marketCap { out.append("capitalisation \(compactUsd(c))") }
        return out
    }

    public static func pendingText(_ market: Kind) -> String {
        "Analyse des \(market == .crypto ? "120 cryptos" : "150 actions") en cours (environ 30 secondes la première fois)…"
    }
}

// MARK: - Anomalies of one asset

public struct Anomaly: Codable, Sendable, Identifiable {
    public enum Severity: String, Codable, Sendable {
        case normal, warning, high
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .warning }
    }
    /// "volume" | "priceVolume" | "zScore" | "openInterest" | "funding" | "longShort"
    public var code: String
    public var severity: Severity
    public var triggered: Bool
    public var title: String
    public var value: Double
    public var threshold: Double
    public var unit: String
    public var measured: String
    public var meaning: String
    public var source: String
    public var id: String { code }
}

public struct LiquidationSummary: Codable, Sendable {
    public struct Largest: Codable, Sendable {
        public var usd: Double
        public var long: Bool
        public var price: Double
        public var time: Double
    }
    public var longUsd: Double
    public var shortUsd: Double
    public var longCount: Int
    public var shortCount: Int
    public var largest: Largest?
    public var from: Double
    public var to: Double
    public var hours: Double
    public var complete: Bool
    public var scope: String

    /// Share of the liquidated value that was long positions (%), nil when nothing was liquidated.
    public var longShare: Double? {
        let total = longUsd + shortUsd
        return total > 0 ? longUsd / total * 100 : nil
    }
}

public struct Derivatives: Codable, Sendable {
    public struct OpenInterest: Codable, Sendable {
        public var usd: Double
        public var time: Double
        public var change24h: Double?
        public var change7d: Double?
    }
    /// Rates in % per settlement period.
    public struct Funding: Codable, Sendable {
        public var rate: Double
        public var p5: Double
        public var p95: Double
        public var samples: Int
        public var periodHours: Double?
        public var time: Double
    }
    public struct LongShort: Codable, Sendable {
        public var ratio: Double
        public var p5: Double
        public var p95: Double
        public var samples: Int
    }
    public var source: String
    public var liquidations: LiquidationSummary?
    public var openInterest: OpenInterest?
    public var funding: Funding?
    public var longShort: LongShort?
    public var errors: [String]
    public var notCovered: [OppNotCovered]
}

public struct AnomalyReport: Codable, Sendable {
    public var symbol: String
    public var kind: Kind
    public var asOf: Double
    public var price: Double?
    /// Close time of the last closed daily session the measures read.
    public var session: Double?
    /// Triggered measures, most severe first.
    public var anomalies: [Anomaly]
    /// Measures below their threshold.
    public var normal: [Anomaly]
    public var derivatives: Derivatives?
    public var errors: [String]
    public var source: String
}

extension AltimClient {
    /// Opportunities of the moment on one market (202 while the first scan runs: `.pending`, ask again).
    public func opportunities(_ kind: Kind) async throws -> OpportunitiesResult {
        let (data, status) = try await getWithStatus("/api/opportunities", ["kind": kind.rawValue])
        if status == 202 { return .pending }
        return .ready(try decodeResponse(OpportunityReport.self, data, status))
    }

    /// Unusual readings of one asset (derivatives for cryptos).
    public func anomalies(_ asset: Asset) async throws -> AnomalyReport {
        try await getAnomalies(["symbol": asset.symbol, "kind": asset.kind.rawValue])
    }
}
