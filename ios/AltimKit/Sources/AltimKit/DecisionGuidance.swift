import Foundation

// Guidance of the decision (backend engine/guidance.rs): when not to trade, action zones, watched scenarios, counter-
// argument and the compact snapshot kept to explain a later change. Types, and the pure helpers of the card, same as
// web/src/webapp/decision.ts and DecisionCard.tsx (same texts). A malformed guidance field fails the decoding (an error
// is shown instead of a broken card); an older answer without them still decodes.

extension Decision {
    /// A reason not to trade now.
    public struct NoTradeReason: Codable, Sendable, Identifiable {
        /// "volatility" | "liquidity" | "spread" | "earnings" | "announcement" | "event" | "trendless" | "weakSignal" | "degraded" | "marketClosed"
        public var code: String
        public var label: String
        public var detail: String
        public var id: String { code }
    }

    /// "Quand ne PAS trader": `headline` empty when not active; `unchecked`: checks without data.
    public struct NoTrade: Codable, Sendable {
        public var active: Bool
        public var headline: String
        public var reasons: [NoTradeReason]
        public var unchecked: [String]

        /// Badge of the section: "2 raisons" or "rien à signaler".
        public var badge: String { active ? "\(reasons.count) raison\(reasons.count > 1 ? "s" : "")" : "rien à signaler" }
        public static let empty = "Aucune raison mesurée de s'abstenir maintenant, ce qui ne garantit rien pour la suite."
        public var uncheckedText: String? { unchecked.isEmpty ? nil : "Non vérifié faute de données : \(unchecked.joined(separator: " ; "))." }
        public static let note = "Volatilité, liquidité, écart achat/vente, résultats (avant et 1 à 2 séances après), annonces, marché sans direction, signal faible ou dégradé, séance de Wall Street (actions). N'interdit rien : signale un mauvais moment."
    }

    public enum ActionZoneKind: String, Codable, Sendable {
        case invalidation, exit, buy, wait, profit, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
    }

    /// A band of the ladder, USD (from = to for a single level).
    public struct ActionZone: Codable, Sendable {
        public var kind: ActionZoneKind
        public var label: String
        public var from: Double
        public var to: Double
        public var note: String
    }

    /// `zones` ascending by price; `here`: kind of the zone holding the price (nil between zones or outside).
    public struct ActionZones: Codable, Sendable {
        public var price: Double
        public var zones: [ActionZone]
        public var here: ActionZoneKind?
        public var hereText: String
    }

    public struct Unfolding: Codable, Sendable {
        public var kind: ScenarioKind
        public var title: String
        public var met: Int
        public var total: Int
        public var text: String
    }

    public enum CheckState: String, Codable, Sendable {
        case met, unmet, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
        /// "✓ remplie", "✕ non remplie", "? inconnue".
        public var icon: String { self == .met ? "✓" : self == .unmet ? "✕" : "?" }
        public var label: String { self == .met ? "remplie" : self == .unmet ? "non remplie" : "inconnue" }
    }

    /// A scenario condition checked on the current data (daily candles, closed).
    public struct ScenarioCheck: Codable, Sendable {
        public var text: String
        public var state: CheckState
        public var detail: String
    }

    /// kind "level": value is a price (USD); "volume": a daily volume in units of the asset; else value nil.
    public struct Invalidator: Codable, Sendable {
        public var kind: String
        public var text: String
        public var value: Double?
    }

    public struct CounterArgument: Codable, Sendable {
        public var favourable: Int
        public var unfavourable: Int
        public var text: String
        public var familiesText: String
        public var invalidators: [Invalidator]
    }

    public struct SnapshotLevel: Codable, Sendable, Equatable {
        public var price: Double
        public var touches: Int
    }

    public struct SnapshotNews: Codable, Sendable {
        public var title: String
        /// "positive" | "negative" | "neutral"
        public var tone: String
        public var time: Double
    }

    public struct SnapshotFamily: Codable, Sendable {
        public var key: String
        public var label: String
        public var score: Double?
    }

    /// Compact numbers kept to explain a later change of the signal.
    public struct DecisionSnapshot: Codable, Sendable {
        public var price: Double?
        public var composite: Double?
        public var families: [SnapshotFamily]
        public var momentum: Double?
        public var relativeVolume: Double?
        public var rsi: Double?
        public var adx: Double?
        public var nearestSupport: SnapshotLevel?
        public var nearestResistance: SnapshotLevel?
        public var newsScore: Double?
        public var topNews: SnapshotNews?
    }
}

extension Decision.ScenarioKind {
    /// "Haussier", "Neutre", "Baissier" (web SCENARIO_UI).
    public var label: String {
        switch self {
        case .bull: return "Haussier"
        case .neutral: return "Neutre"
        case .bear: return "Baissier"
        case .unknown: return "Inconnu"
        }
    }
    public var icon: String { self == .bull ? "↗" : self == .bear ? "↘" : "→" }
}

public enum DecisionGuidance {
    /// One row of the action ladder: a zone (with whether the price is in it), or the price's own marker when it lies
    /// between zones or outside them.
    public enum LadderRow: Sendable {
        case zone(Decision.ActionZone, here: Bool)
        case marker(String)
    }

    /// Vertical ladder, highest price at the top. Price outside every zone: its marker goes above the first zone lying
    /// entirely under it.
    public static func ladder(_ z: Decision.ActionZones) -> [LadderRow] {
        let rows = Array(z.zones.reversed())
        let markerAt: Int
        if z.here != nil {
            markerAt = -1
        } else {
            markerAt = rows.firstIndex { $0.to < z.price } ?? rows.count
        }
        var hereText = z.hereText
        if hereText.hasPrefix("Vous êtes ici : ") { hereText = "vous êtes ici : " + hereText.dropFirst("Vous êtes ici : ".count) }
        let marker = LadderRow.marker("◀ \(hereText)")
        var out: [LadderRow] = []
        for (i, r) in rows.enumerated() {
            if i == markerAt { out.append(marker) }
            out.append(.zone(r, here: z.here == r.kind))
        }
        if markerAt == rows.count { out.append(marker) }
        return out
    }

    /// "261,79 $ – 268,77 $" or a single level.
    public static func range(_ r: Decision.ActionZone) -> String {
        r.from == r.to ? DecisionText.usd(r.from) : "\(DecisionText.usd(r.from)) – \(DecisionText.usd(r.to))"
    }

    /// "◀ vous êtes ici · 341,04 $".
    public static func hereMark(_ price: Double) -> String { "◀ vous êtes ici · \(DecisionText.usd(price))" }

    /// "1/2 conditions · en cours".
    public static func scenarioCount(_ s: Decision.Scenario) -> String? {
        guard let c = s.conditions, !c.isEmpty else { return nil }
        return "\(s.met ?? 0)/\(c.count) condition\(c.count > 1 ? "s" : "")\(s.unfolding == true ? " · en cours" : "")"
    }

    /// Badge of the scenarios section: "en cours : neutre".
    public static func scenariosBadge(_ d: Decision) -> String? {
        d.unfolding.map { "en cours : \($0.kind.label.lowercased())" }
    }

    /// "28/09 à 14:02" (Paris time), as the web's `shortDateTime`.
    public static func shortDateTime(_ ms: Double) -> String {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "Europe/Paris")!
        let c = cal.dateComponents([.day, .month, .hour, .minute], from: Date(timeIntervalSince1970: ms / 1000))
        return String(format: "%02d/%02d à %02d:%02d", c.day ?? 1, c.month ?? 1, c.hour ?? 0, c.minute ?? 0)
    }

    /// Title of "Pourquoi le signal a changé depuis le 28/09 à 14:02".
    public static func changeTitle(_ t: ConfigTransition) -> String { "Pourquoi le signal a changé depuis le \(shortDateTime(t.since))" }

    /// "ZONE D'ACHAT → ATTENDRE" (the transition's title without its prefix).
    public static func changeSubtitle(_ t: ConfigTransition) -> String {
        let prefix = "🚨 \(t.symbol) — changement de configuration : "
        let title = t.title
        return title.hasPrefix(prefix) ? String(title.dropFirst(prefix.count)) : title
    }

    public static let changeUnknown = "Mesures de la décision précédente non enregistrées (vue avant cette version) : changement non détaillé."
}
