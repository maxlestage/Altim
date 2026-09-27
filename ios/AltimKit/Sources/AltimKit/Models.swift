import Foundation

// Shapes of the Altim server's JSON (web/src/webapp/api.ts). Every figure is computed on the server
// (median consensus of 40 sources); the app only displays it. Optional fields stay optional so a
// server change never breaks decoding.

public enum Kind: String, Codable, Sendable, CaseIterable, Hashable {
    case crypto, stock
    public var label: String { self == .crypto ? "Crypto" : "Action" }
}

public enum Action: String, Codable, Sendable {
    case strongBuy, buy, hold, sell, strongSell
    public var label: String {
        switch self {
        case .strongBuy: return "ACHAT FORT"
        case .buy: return "ACHAT"
        case .hold: return "ATTENDRE"
        case .sell: return "VENTE"
        case .strongSell: return "VENTE FORTE"
        }
    }
    public var tone: Tone { self == .buy || self == .strongBuy ? .good : self == .hold ? .neutral : .bad }
}

/// Colour meaning, mapped to the theme by the app.
public enum Tone: Sendable { case good, warn, bad, neutral }

public struct Asset: Codable, Sendable, Hashable, Identifiable {
    public var symbol: String
    public var kind: Kind
    public var name: String
    public var id: String { "\(kind.rawValue):\(symbol)" }
    public init(symbol: String, kind: Kind, name: String) {
        self.symbol = symbol
        self.kind = kind
        self.name = name
    }
    public static let defaults: [Asset] = [
        .init(symbol: "BTC", kind: .crypto, name: "Bitcoin"),
        .init(symbol: "ETH", kind: .crypto, name: "Ethereum"),
        .init(symbol: "SOL", kind: .crypto, name: "Solana"),
        .init(symbol: "BNB", kind: .crypto, name: "BNB"),
        .init(symbol: "XRP", kind: .crypto, name: "XRP"),
        .init(symbol: "AAPL", kind: .stock, name: "Apple"),
        .init(symbol: "NVDA", kind: .stock, name: "NVIDIA"),
        .init(symbol: "MSFT", kind: .stock, name: "Microsoft"),
    ]
}

public struct Reliability: Codable, Sendable {
    public var score: Double
    public var level: String
    public var independent: Int?
    public var conflict: Bool?
    public var label: String {
        switch level {
        case "high": return "Fiabilité élevée"
        case "medium": return "Fiabilité moyenne"
        default: return "Fiabilité faible"
        }
    }
    public var tone: Tone { level == "high" ? .good : level == "medium" ? .warn : .bad }
}

public struct SignalSummary: Codable, Sendable {
    public var action: Action
    public var score: Double
    public var confidence: Double
}

public struct RadarRow: Codable, Sendable, Identifiable {
    public var symbol: String
    public var kind: Kind
    public var name: String?
    public var price: Double?
    public var change: Double?
    public var priceSources: String?
    public var signal: SignalSummary?
    public var reliability: Reliability?
    public var agreeing: Int?
    public var sources: Int?
    public var sparkline: [Double]?
    public var error: String?
    public var id: String { "\(kind.rawValue):\(symbol)" }
}

public struct Quote: Codable, Sendable {
    public struct Source: Codable, Sendable { public var name: String; public var ok: Bool; public var price: Double?; public var error: String? }
    public var symbol: String
    public var kind: Kind
    public var name: String
    public var price: Double
    public var change: Double?
    public var agreeing: Int
    public var total: Int
    public var sources: [Source]
}

public struct SearchItem: Codable, Sendable, Identifiable, Hashable {
    public var symbol: String
    public var name: String
    public var kind: Kind
    public var rank: Int?
    public var etf: Bool?
    public var id: String { "\(kind.rawValue):\(symbol)" }
    public var asset: Asset { Asset(symbol: symbol, kind: kind, name: name) }
}

public struct Candle: Codable, Sendable, Identifiable {
    public var time: Double
    public var open: Double
    public var high: Double
    public var low: Double
    public var close: Double
    public var volume: Double?
    public var id: Double { time }
    public var date: Date { Date(timeIntervalSince1970: time / 1000) }
}

public struct Snapshot: Codable, Sendable {
    public struct Source: Codable, Sendable { public var name: String; public var ok: Bool; public var deviation: Double?; public var error: String? }
    public var symbol: String
    public var kind: Kind
    public var interval: String
    public var candles: [Candle]
    public var source: String
    public var sources: [Source]
    public var agreeing: Int
    public var reliability: Reliability
}

// MARK: - Guard (regime, shock, reversal)

public struct Evidence: Codable, Sendable {
    public var samples: Int
    public var rate: Double
    public var base: Double
    public var lift: Double
}

public struct GuardFactor: Codable, Sendable, Identifiable {
    public var code: String
    public var points: Double
    public var basePoints: Double?
    public var text: String
    public var status: String?
    public var evidence: Evidence?
    public var id: String { code }
    public var statusText: String? {
        switch status {
        case "verified": return "vérifié sur cet actif"
        case "unproven": return "peu d'historique : compté à moitié"
        case "rejected": return "jamais prédictif ici : ignoré"
        case "unverifiable": return "sans historique : non vérifié"
        default: return nil
        }
    }
}

public struct GuardReport: Codable, Sendable {
    public struct Regime: Codable, Sendable { public var trend: String; public var strength: Double; public var text: String }
    public struct Shock: Codable, Sendable { public var score: Double; public var level: String; public var factors: [GuardFactor] }
    public struct Reversal: Codable, Sendable { public var score: Double; public var direction: String?; public var factors: [GuardFactor] }
    public struct Policy: Codable, Sendable { public var scalping: String; public var sizeMultiplier: Double; public var stopMultiplier: Double; public var notes: [String] }
    public struct Headline: Codable, Sendable, Identifiable { public var title: String; public var time: Double; public var source: String?; public var id: String { "\(time)\(title)" } }
    public struct Inputs: Codable, Sendable {
        public var fearGreed: Double?
        public var news24h: Int?
        public var headlines: [Headline]?
        public var vix: Double?
    }
    public var symbol: String
    public var kind: Kind
    public var price: Double?
    public var asOf: Double
    public var regime: Regime
    public var shock: Shock
    public var reversal: Reversal
    public var policy: Policy
    public var inputs: Inputs?
    public var macro: MacroInfo?

    public var trendLabel: String { ["up": "Haussière", "down": "Baissière"][regime.trend] ?? "Sans direction" }
    public var shockLabel: String { ["agitated": "Agité", "shock": "Choc"][shock.level] ?? "Calme" }
    public var shockTone: Tone { shock.level == "shock" ? .bad : shock.level == "agitated" ? .warn : .good }
    public var policyLabel: String { ["reduce": "Taille réduite", "pause": "Suspendu"][policy.scalping] ?? "Autorisé" }
}

// MARK: - Macro

public struct MacroInfo: Codable, Sendable {
    public struct Factor: Codable, Sendable, Identifiable { public var code: String; public var points: Double; public var text: String; public var id: String { code } }
    public struct Theme: Codable, Sendable, Identifiable { public var theme: String; public var label: String; public var count: Int; public var examples: [String]; public var id: String { theme } }
    public struct Value: Codable, Sendable { public var value: Double; public var change5d: Double }
    public var score: Double
    public var level: String
    public var marketScore: Double?
    public var factors: [Factor]
    public var themes: [Theme]
    public var values: [String: Value]
    public var asOf: Double?
    public var evidence: Evidence?

    public var levelLabel: String { ["tense": "Tendu", "high": "Très tendu"][level] ?? "Calme" }
    public var tone: Tone { level == "high" ? .bad : level == "tense" ? .warn : .good }
    /// Display order and French names of the market series.
    public static let series: [(key: String, label: String)] = [
        ("vix", "VIX (peur)"), ("spx", "S&P 500"), ("oil", "Pétrole"), ("gold", "Or"), ("dollar", "Dollar"), ("rates", "Taux 10 ans"),
    ]
}

// MARK: - Fibonacci buy zones

public struct Band: Codable, Sendable { public var from: Double; public var to: Double }

public struct FibZone: Codable, Sendable, Identifiable {
    public struct Level: Codable, Sendable { public var ratio: Double; public var price: Double }
    public var horizon: String
    public var label: String
    public var unit: String
    public var holding: String
    public var status: String
    public var levels: [Level]
    public var zone: Band?
    public var golden: Band?
    public var invalidation: Double?
    public var targets: [Double]
    public var distance: Double?
    public var evidence: Evidence?
    public var text: String
    public var macroNote: String?
    public var id: String { horizon }

    public var statusLabel: String {
        switch status {
        case "above": return "Attendre le repli"
        case "inZone": return "Dans la zone"
        case "golden": return "Zone d'or"
        case "deep": return "Repli profond"
        case "broken": return "Zone invalidée"
        case "downtrend": return "Tendance baissière"
        default: return "Pas de niveau net"
        }
    }
    public var tone: Tone {
        switch status {
        case "inZone", "golden": return .good
        case "above", "deep": return .warn
        case "broken", "downtrend": return .bad
        default: return .neutral
        }
    }
}

public struct ZonesReport: Codable, Sendable {
    public var symbol: String
    public var kind: Kind
    public var price: Double?
    public var asOf: Double
    public var zones: [FibZone]
    public var macro: MacroInfo?
}

// MARK: - Selection (which stocks / cryptos to buy)

public enum Horizon: String, Codable, Sendable, CaseIterable, Identifiable {
    case m30 = "30m", h1 = "1h", h5 = "5h", d7 = "7d", d14 = "14d", mo1 = "1m", mo3 = "3m", mo6 = "6m"
    public var id: String { rawValue }
    public var label: String {
        switch self {
        case .m30: return "30 min"
        case .h1: return "1 h"
        case .h5: return "5 h"
        case .d7: return "7 j"
        case .d14: return "14 j"
        case .mo1: return "1 mois"
        case .mo3: return "3 mois"
        case .mo6: return "6 mois"
        }
    }
    public var isIntraday: Bool { self == .m30 || self == .h1 || self == .h5 }
}

public enum Criterion: String, Codable, Sendable, CaseIterable {
    case signal, trend, momentum, zone, risk
}

public struct Plan: Codable, Sendable {
    public var entry: Double
    public var limit: Double?
    public var stop: Double
    public var target: Double
    public var atrPct: Double
}

public struct Check: Codable, Sendable, Identifiable {
    public var label: String
    public var ok: Bool
    public var detail: String
    public var id: String { label }
}

public struct Candidate: Codable, Sendable, Identifiable {
    public struct Track: Codable, Sendable { public var trades: Int; public var winRate: Double; public var avgReturn: Double }
    public var rank: Int
    public var symbol: String
    public var name: String
    public var sector: String
    public var marketCap: Double?
    public var price: Double
    public var scores: [String: Double]
    public var why: [String: String]
    public var action: String
    public var zoneStatus: String
    public var plan: Plan?
    public var track: Track?
    public var checks: [Check]
    /// Only for the "à surveiller" list: why it did not make the cut.
    public var reason: String?
    public var id: String { symbol }
    public var actionValue: Action? { Action(rawValue: action) }
}

public struct SetAside: Codable, Sendable, Identifiable {
    public var symbol: String
    public var name: String
    public var reason: String
    public var id: String { symbol }
}

public struct Validation: Codable, Sendable {
    public var periods: Int
    public var top: Double
    public var universe: Double
    public var beatRate: Double
    public var topN: Int
    public var from: Double?
    public var to: Double?
    public var benchmark: Double?
    public var cost: Double
    public var edge: String
    public var noEdge: Bool?
}

public struct SelectionReport: Codable, Sendable {
    public var market: Kind
    public var horizon: Horizon
    public var asOf: Double
    public var scanned: Int
    public var rankBy: Criterion
    public var rankRule: String
    public var holdText: String
    public var evidence: String
    public var marketClosed: Bool
    public var criteria: [String: String]
    public var roles: [String: String]
    public var buy: [Candidate]
    public var watch: [Candidate]
    public var setAside: [SetAside]
    public var validation: Validation?

    public var rankText: String {
        switch rankRule {
        case "signal": return "signal technique d'Altim"
        case "momentum": return "force relative, les plus en hausse d'abord"
        case "reversal": return "rebond, les plus en baisse d'abord"
        case "lowRisk": return "les plus calmes d'abord"
        default: return rankRule
        }
    }
    /// Criteria in display order, the ranking one first.
    public var orderedCriteria: [Criterion] {
        [rankBy] + [Criterion.momentum, .zone, .trend, .risk, .signal].filter { $0 != rankBy }
    }
}

/// The screener answers 202 `{pending: true}` while the first computation runs (≈ 30 s).
public enum SelectionResult: Sendable {
    case ready(SelectionReport)
    case pending
}

// MARK: - Live prices (Server-Sent Events)

public struct LiveTick: Codable, Sendable {
    public var symbol: String
    public var kind: Kind
    public var price: Double
    public var change: Double?
    public var agreeing: Int?
    public var total: Int?
    public var sources: [String]?
    public var time: Double
    /// Stocks only: "open" / "closed" (New York session).
    public var market: String?
    public var key: String { "\(kind.rawValue):\(symbol)" }
}

extension SelectionReport {
    /// Amount for each pick: the budget is split so that each line risks the same sum if its stop is hit
    /// (a volatile asset gets less money), and no line exceeds `maxPercent` of the budget (same rule as the web app).
    public func allocate(budget: Double, maxPercent: Double = 20) -> [String: Double] {
        let withPlan = buy.compactMap { c in c.plan.map { (c.symbol, $0) } }
        guard budget > 0, !withPlan.isEmpty else { return [:] }
        let inv = withPlan.map { 1 / max(0.5, (1 - $0.1.stop / ($0.1.limit ?? $0.1.entry)) * 100) }
        let sum = inv.reduce(0, +)
        let cap = budget * maxPercent / 100
        var out: [String: Double] = [:]
        for (i, p) in withPlan.enumerated() { out[p.0] = min(cap, budget * inv[i] / sum) }
        return out
    }
}
