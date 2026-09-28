import Foundation

// Summaries of a decision and its technical structure (backend/src/engine/synthesis.rs and structure.rs, web types in
// web/src/webapp/decision.ts): 6-level rating, composite score, degraded signal, market regime, horizon class,
// Ichimoku / Supertrend / Donchian / VWAP / volume profile / pivots / levels / breakout / market structure / relative
// strength. Unknown enum values decode as `.unknown`: a new server value never breaks decoding.

extension Decision {
    public enum Rating: String, Codable, Sendable {
        case strongBuy, buy, hold, reduce, sell, strongSell, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
        public var label: String {
            switch self {
            case .strongBuy: return "ACHAT FORT"
            case .buy: return "ACHAT"
            case .hold, .unknown: return "ATTENDRE"
            case .reduce: return "ALLÉGER"
            case .sell: return "VENDRE"
            case .strongSell: return "VENTE FORTE"
            }
        }
        public var emoji: String {
            switch self {
            case .strongBuy, .buy: return "🟢"
            case .hold, .unknown: return "⚪"
            case .reduce: return "🟠"
            case .sell, .strongSell: return "🔴"
            }
        }
        /// Colour of the matching level (web RATING_UI).
        public var level: Level {
            switch self {
            case .strongBuy, .buy: return .strong
            case .hold, .unknown: return .waiting
            case .reduce: return .highRisk
            case .sell, .strongSell: return .exit
            }
        }
    }

    public struct ScoreFactor: Codable, Sendable, Identifiable {
        /// "tech" | "mom" | "fund" | "sent" | "news" | "macro"
        public var key: String
        public var label: String
        /// Weight asked (0 – 100) and applied once renormalised on the measured factors (%).
        public var weight: Double
        public var applied: Double
        /// −100 … +100; nil: not measured.
        public var value: Double?
        public var contribution: Double?
        public var sources: [String]
        public var id: String { key }
    }

    public struct CompositeScore: Codable, Sendable {
        /// −100 … +100; nil when no factor could be measured.
        public var value: Double?
        public var label: String
        public var factors: [ScoreFactor]
        public var missing: [String]
        /// The user's own weights were used.
        public var custom: Bool
        public var text: String
    }

    public struct Degraded: Codable, Sendable {
        public var active: Bool
        public var headline: String
        public var reasons: [String]
    }

    public enum RegimeKind: String, Codable, Sendable {
        case riskOn, riskOff, neutral, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
        public var emoji: String { self == .riskOn ? "🟢" : self == .riskOff ? "🔴" : "⚪" }
        public var label: String {
            switch self {
            case .riskOn: return "Risk-on (appétit pour le risque)"
            case .riskOff: return "Risk-off (aversion au risque)"
            case .neutral, .unknown: return "Neutre"
            }
        }
    }

    public struct MarketRegime: Codable, Sendable {
        public var kind: RegimeKind
        public var label: String
        public var benchmark: String?
        public var reasons: [String]
    }

    public struct HorizonClass: Codable, Sendable {
        /// "scalping" | "dayTrading" | "swing" | "mediumTerm" | "longTerm"
        public var kind: String
        public var label: String
        public var atrDistance: Double
        public var detail: String
    }

    public enum Bias: String, Codable, Sendable {
        case bullish, neutral, bearish, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
        public var arrow: String { self == .bullish ? "↗" : self == .bearish ? "↘" : "→" }
        public var label: String { self == .bullish ? "haussier" : self == .bearish ? "baissier" : "neutre" }
        public var tone: Tone { self == .bullish ? .good : self == .bearish ? .bad : .neutral }
    }

    public struct Ichimoku: Codable, Sendable {
        public var tenkan: Double
        public var kijun: Double
        public var senkouA: Double
        public var senkouB: Double
        public var futureA: Double
        public var futureB: Double
        public var position: String
        public var tkCross: String?
        public var tkCrossBars: Int?
        public var bias: Bias
        public var reading: String
    }

    public struct Supertrend: Codable, Sendable {
        public var direction: String
        public var level: Double
        public var bars: Int
        public var bias: Bias
        public var reading: String
    }

    public struct Donchian: Codable, Sendable {
        public var upper: Double
        public var lower: Double
        public var mid: Double
        public var breakout: String?
        public var bias: Bias
        public var reading: String
    }

    public struct Vwap: Codable, Sendable {
        public var value: Double
        public var bars: Int
        public var deviationPct: Double
        public var bias: Bias
        public var reading: String
    }

    public struct VolumeProfile: Codable, Sendable {
        public var poc: Double
        public var valueAreaHigh: Double
        public var valueAreaLow: Double
        public var bars: Int
        public var bins: Int
        public var bias: Bias
        public var reading: String
        public var note: String
    }

    public struct FloorPivots: Codable, Sendable {
        public var pivot: Double
        public var r1: Double
        public var r2: Double
        public var s1: Double
        public var s2: Double
        public var from: Double
        public var reading: String
    }

    public struct SrLevel: Codable, Sendable {
        public var price: Double
        public var touches: Int
        /// "support" | "resistance"
        public var kind: String
        public var distancePct: Double
    }

    public struct Breakout: Codable, Sendable {
        /// "confirmed" | "unconfirmed" | "fake" | "none"
        public var kind: String
        public var side: String?
        public var level: Double?
        public var volumeRatio: Double?
        public var bias: Bias
        public var reading: String
    }

    public struct MarketStructure: Codable, Sendable {
        public var trend: String
        public var highs: [Double]
        public var lows: [Double]
        public var bias: Bias
        public var reading: String
    }

    public struct RsPeriod: Codable, Sendable {
        public var label: String
        public var days: Int
        public var assetPct: Double
        public var benchmarkPct: Double
        public var diff: Double
    }

    public struct RelativeStrength: Codable, Sendable {
        public var benchmark: String
        public var symbol: String
        public var periods: [RsPeriod]
        public var correlation: Double?
        public var bias: Bias
        public var reading: String
    }

    public struct Structure: Codable, Sendable {
        public var timeframe: String
        /// Overall direction −100 … +100; nil when nothing could be measured.
        public var score: Double?
        public var ichimoku: Ichimoku?
        public var supertrend: Supertrend?
        public var donchian: Donchian?
        public var vwap: Vwap?
        public var volumeProfile: VolumeProfile?
        public var pivots: FloorPivots?
        public var levels: [SrLevel]
        public var nearestSupport: SrLevel?
        public var nearestResistance: SrLevel?
        public var levelsReading: String
        public var breakout: Breakout?
        public var marketStructure: MarketStructure?
        public var relative: [RelativeStrength]
        public var relativeNote: String?
    }
}

// MARK: - Composite score weights (Réglages, sent as w=)

/// Weights of the decision's composite score, same factors and defaults as the server (synthesis.rs).
public struct ScoreWeights: Codable, Sendable, Equatable {
    public var tech: Int
    public var mom: Int
    public var fund: Int
    public var sent: Int
    public var news: Int
    public var macro: Int

    public init(tech: Int = 32, mom: Int = 18, fund: Int = 20, sent: Int = 10, news: Int = 10, macro: Int = 10) {
        self.tech = tech
        self.mom = mom
        self.fund = fund
        self.sent = sent
        self.news = news
        self.macro = macro
    }

    public static let defaults = ScoreWeights()

    public struct Factor {
        public let key: String
        public let path: WritableKeyPath<ScoreWeights, Int>
        public let label: String
        public let hint: String
        public let def: Int
    }

    public static let factors: [Factor] = [
        Factor(key: "tech", path: \.tech, label: "Technique", hint: "tendance, volume, volatilité, structure", def: 32),
        Factor(key: "mom", path: \.mom, label: "Momentum", hint: "MACD, RSI, variation sur 1 mois", def: 18),
        Factor(key: "fund", path: \.fund, label: "Fondamentaux", hint: "valorisation, comptes ou réseau", def: 20),
        Factor(key: "sent", path: \.sent, label: "Sentiment", hint: "Fear & Greed, financement, StockTwits", def: 10),
        Factor(key: "news", path: \.news, label: "Actualités", hint: "ton des titres sur 24 h", def: 10),
        Factor(key: "macro", path: \.macro, label: "Macro", hint: "stress des marchés, VIX, taux", def: 10),
    ]

    public var total: Int { Self.factors.reduce(0) { $0 + self[keyPath: $1.path] } }

    /// Whole numbers 0 – 100; all at 0 → the defaults (web `sanitizeScoreWeights`).
    public var sanitized: ScoreWeights {
        var w = self
        for f in Self.factors { w[keyPath: f.path] = min(100, max(0, w[keyPath: f.path])) }
        return w.total > 0 ? w : .defaults
    }

    /// "tech:40,mom:18,…", or nil for the default weights (the request stays the same).
    public var parameter: String? {
        let s = sanitized
        guard s != .defaults else { return nil }
        return Self.factors.map { "\($0.key):\(s[keyPath: $0.path])" }.joined(separator: ",")
    }

    /// A missing or bad stored value takes its default; out-of-range values are brought within 0 – 100.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        func read(_ k: CodingKeys, _ d: Int) -> Int {
            guard let v = (try? c.decodeIfPresent(Double.self, forKey: k)) ?? nil, v.isFinite else { return d }
            return Int(min(100, max(0, v.rounded())))
        }
        self.init(tech: read(.tech, 32), mom: read(.mom, 18), fund: read(.fund, 20), sent: read(.sent, 10), news: read(.news, 10), macro: read(.macro, 10))
        self = sanitized
    }
}

// MARK: - Texts (web DecisionCard.tsx)

extension DecisionText {
    /// Price in dollars like the web's `usd`: no decimals when whole, else 2 (4 or 8 below 1 $).
    public static func usd(_ v: Double?) -> String {
        guard let v, v.isFinite else { return "—" }
        let a = abs(v)
        let digits = a >= 1 ? (v.rounded() == v ? 0 : 2) : a >= 0.01 ? 4 : 8
        return "\(JSFormat.fr(v, min: digits, max: digits))\(nnbsp)$"
    }

    /// "+34", "−12", "0" (scores −100 … +100).
    public static func signedScore(_ v: Double) -> String {
        // Math.round of JavaScript (halves towards +∞), sign of the raw value, as the web writes it.
        let r = abs((v + 0.5).rounded(.down))
        return "\(v > 0 ? "+" : v < 0 ? "−" : "")\(Int(r))"
    }

    /// "+8,1 pts".
    public static func points(_ v: Double) -> String { "\(v > 0 ? "+" : v < 0 ? "−" : "")\(num(abs(v), digits: 1)) pts" }

    /// One item of the technical structure, measured or not (a missing one says why rather than disappearing).
    public struct StructureRow: Sendable {
        public var name: String
        public var bias: Decision.Bias?
        public var reading: String
        /// Figures under the reading.
        public var extra: String?
        /// Levels or periods, one per line.
        public var list: [String] = []
    }

    public static func structureRows(_ st: Decision.Structure) -> [StructureRow] {
        let na = "Non disponible : historique trop court"
        let noVolume = "Non disponible : volume absent ou historique trop court"
        var rows: [StructureRow] = []
        if let i = st.ichimoku {
            rows.append(StructureRow(name: "Ichimoku (9, 26, 52)", bias: i.bias, reading: i.reading,
                                     extra: "Tenkan \(usd(i.tenkan)) · Kijun \(usd(i.kijun)) · nuage \(usd(min(i.senkouA, i.senkouB))) – \(usd(max(i.senkouA, i.senkouB)))"))
        } else {
            rows.append(StructureRow(name: "Ichimoku (9, 26, 52)", reading: na))
        }
        rows.append(st.supertrend.map { StructureRow(name: "Supertrend (ATR 10 × 3)", bias: $0.bias, reading: $0.reading) }
                    ?? StructureRow(name: "Supertrend (ATR 10 × 3)", reading: na))
        rows.append(st.donchian.map { StructureRow(name: "Canal de Donchian (20)", bias: $0.bias, reading: $0.reading,
                                                   extra: "Haut \(usd($0.upper)) · milieu \(usd($0.mid)) · bas \(usd($0.lower))") }
                    ?? StructureRow(name: "Canal de Donchian (20)", reading: na))
        rows.append(st.vwap.map { StructureRow(name: "VWAP glissant (\($0.bars) bougies)", bias: $0.bias, reading: $0.reading) }
                    ?? StructureRow(name: "VWAP glissant", reading: noVolume))
        rows.append(st.volumeProfile.map { StructureRow(name: "Profil de volume (approximation)", bias: $0.bias, reading: $0.reading, extra: $0.note) }
                    ?? StructureRow(name: "Profil de volume (approximation)", reading: noVolume))
        rows.append(st.pivots.map { StructureRow(name: "Points pivots (dernière séance)", reading: $0.reading,
                                                 extra: "S2 \(usd($0.s2)) · S1 \(usd($0.s1)) · P \(usd($0.pivot)) · R1 \(usd($0.r1)) · R2 \(usd($0.r2))") }
                    ?? StructureRow(name: "Points pivots", reading: na))
        rows.append(StructureRow(name: "Supports et résistances", reading: st.levelsReading,
                                 list: st.levels.prefix(8).map { l in
                                     "\(l.kind == "support" ? "Support" : "Résistance") \(usd(l.price)) · \(l.touches) contacts · \(pct(l.distancePct, digits: 1, sign: true))"
                                 }))
        rows.append(st.breakout.map { StructureRow(name: "Cassure", bias: $0.bias, reading: $0.reading) } ?? StructureRow(name: "Cassure", reading: na))
        rows.append(st.marketStructure.map { StructureRow(name: "Structure (sommets et creux)", bias: $0.bias, reading: $0.reading) }
                    ?? StructureRow(name: "Structure (sommets et creux)", reading: "Non disponible : pas assez de sommets et de creux"))
        for r in st.relative {
            rows.append(StructureRow(name: "Force relative contre \(r.benchmark)", bias: r.bias, reading: r.reading,
                                     list: r.periods.map { "\($0.label) : \(pct($0.assetPct, digits: 1, sign: true)) contre \(pct($0.benchmarkPct, digits: 1, sign: true)) (\(points($0.diff)))" }))
        }
        if st.relative.isEmpty { rows.append(StructureRow(name: "Force relative", reading: st.relativeNote ?? "Non disponible")) }
        return rows
    }

    /// "Bougies journalières clôturées. Direction d'ensemble : +70/100."
    public static func structureHeader(_ st: Decision.Structure) -> String {
        "\(st.timeframe)." + (st.score.map { " Direction d'ensemble : \(signedScore($0))/100." } ?? "")
    }

    /// "Objectif 3 : projection : objectif 2 + …." (first letter lowered, like the web).
    public static func target3Source(_ s: String) -> String {
        guard let first = s.first else { return "" }
        return "Objectif 3 : \(first.lowercased())\(s.dropFirst())."
    }

    /// Badge of the "Agenda (7 jours)" section.
    public static func eventsBadge(_ events: [CalendarEvent]?) -> String {
        guard let events else { return "non vérifié" }
        if events.isEmpty { return "rien de majeur" }
        return "\(events.count) événement\(events.count > 1 ? "s" : "")"
    }

    /// Text of the "Agenda (7 jours)" section when there is no event to list; nil when there are some.
    public static func eventsNote(_ events: [CalendarEvent]?, kind: Kind) -> String? {
        guard let events else { return "Calendrier indisponible ou incomplet : les annonces à venir n'ont pas pu être vérifiées." }
        guard events.isEmpty else { return nil }
        return "Aucune annonce majeure (banques centrales, inflation, emploi, PIB)\(kind == .stock ? ", ni résultats, dividende ou split" : "") dans les 7 jours."
    }
}
