import Foundation

// "Résumé intelligent" of the news (`summary` of /api/news, computed by the server in engine/news_summary.rs): types
// and the pure helpers of the card, same as web/src/webapp/news-summary.ts (same texts).

public struct StorySummary: Codable, Sendable, Identifiable, Hashable {
    public enum Impact: String, Codable, Sendable, Hashable {
        case low, medium, high, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
        /// "faible", "moyen", "important".
        public var label: String {
            switch self {
            case .low, .unknown: return "faible"
            case .medium: return "moyen"
            case .high: return "important"
            }
        }
    }

    public struct Link: Codable, Sendable, Hashable {
        public var source: String
        public var title: String
        public var link: String
        public var time: Double
        /// "negative" | "positive" | "neutral"
        public var tone: String

        /// Only http(s) links are opened.
        public var url: URL? { NewsSummary.safeURL(link) }
    }

    public struct Consensus: Codable, Sendable, Hashable {
        /// "convergent" | "divergent" | "single"
        public var agreement: String
        public var tone: String
        public var negative: Int
        public var positive: Int
        public var neutral: Int
    }

    /// Hourly closes of the server's cache: from the close preceding publication to the last one (ms, prices, %).
    public struct Move: Codable, Sendable, Hashable {
        public var asset: String
        public var fromTime: Double
        public var fromPrice: Double
        public var toTime: Double
        public var toPrice: Double
        public var changePct: Double
        public var source: String
    }

    /// The story against the asset's technical trend (the guard's, when cached).
    public struct Technical: Codable, Sendable, Hashable {
        public var asset: String
        /// "up" | "down" | "range"
        public var trend: String
        public var text: String
    }

    /// Id of the merged story in `items`.
    public var id: String
    public var title: String
    public var link: String
    public var source: String
    public var time: Double
    public var category: String
    public var themes: [String]
    public var alert: Bool
    /// Distinct sources that told the story.
    public var sources: Int
    /// Distinct headlines among them (one article syndicated word for word counts once): what the rule counts.
    public var independentSources: Int
    /// Each source's own headline (8 at most), the first one first.
    public var links: [Link]
    /// The user's assets named in the headlines: "stock:AAPL"…
    public var assets: [String]
    public var impact: Impact
    /// "measured": from the move of the named assets since publication; "rule": sources, theme, the user's assets.
    public var impactBasis: String
    public var ruleImpact: Impact
    public var impactPoints: Int
    /// French, one line per point of the rule, and the measured moves.
    public var impactReasons: [String]
    public var consensus: Consensus
    public var moves: [Move]
    public var technical: [Technical]

    public var url: URL? { NewsSummary.safeURL(link) }
    public var measured: Bool { impactBasis == "measured" }
}

public enum NewsSummary {
    static func safeURL(_ s: String) -> URL? {
        guard let u = URL(string: s), let scheme = u.scheme?.lowercased(), scheme == "https" || scheme == "http" else { return nil }
        return u
    }

    /// "impact mesuré" / "impact estimé par règle".
    public static func basisLabel(_ s: StorySummary) -> String { s.measured ? "impact mesuré" : "impact estimé par règle" }

    /// Counts the events of moyen or important impact; the faible ones are "autres sujets".
    public static func heading(_ list: [StorySummary]) -> (title: String, others: String?) {
        let n = list.filter { $0.impact != .low }.count
        let rest = list.count - n
        let title = n == 0 ? "Aucun événement important aujourd'hui" : n == 1 ? "1 événement important aujourd'hui" : "\(n) événements importants aujourd'hui"
        let others = rest == 0 ? nil : "\(n == 0 ? "" : "Et ")\(rest) sujet\(rest > 1 ? "s" : "") repris par plusieurs sources, à impact faible."
        return (title, others)
    }

    private static func plural(_ n: Int, _ one: String, _ many: String) -> String { "\(n) \(n > 1 ? many : one)" }

    /// "Convergent · 3 sources · ton des titres : 2 négatifs, 1 neutre" (tones counted over the distinct headlines).
    public static func consensusText(_ s: StorySummary) -> String {
        let c = s.consensus
        if c.agreement == "single" {
            return s.sources > 1 ? "Même titre repris par \(s.sources) sources : pas de consensus mesurable" : "Une seule source : pas de consensus mesurable"
        }
        var parts: [String] = []
        if c.negative != 0 { parts.append(plural(c.negative, "négatif", "négatifs")) }
        if c.positive != 0 { parts.append(plural(c.positive, "positif", "positifs")) }
        if c.neutral != 0 { parts.append(plural(c.neutral, "neutre", "neutres")) }
        let titles = c.negative + c.positive + c.neutral
        let head = "\(c.agreement == "convergent" ? "Convergent" : "Divergent") · \(plural(s.sources, "source", "sources"))"
        return "\(head)\(titles < s.sources ? " (\(titles) titres distincts)" : "") · ton des titres : \(parts.joined(separator: ", "))"
    }

    /// "stock:AAPL" → ("stock", "AAPL"); the asset to open when the kind is known.
    public static func assetLink(_ id: String) -> (kind: String, symbol: String, asset: Asset?) {
        guard let i = id.firstIndex(of: ":") else { return ("", id, nil) }
        let kind = String(id[..<i])
        let rest = id[id.index(after: i)...]
        // Same as the web's split(":"): the part after the first colon, up to a second one.
        let symbol = String(rest.split(separator: ":", omittingEmptySubsequences: false).first ?? "")
        return (kind, symbol, Kind(rawValue: kind).map { Asset(symbol: symbol, kind: $0, name: symbol) })
    }

    /// "AAPL +4,0 % depuis la publication".
    public static func moveText(_ m: StorySummary.Move) -> String {
        let digits = JSFormat.fr(abs(m.changePct), min: 1, max: 1).replacingOccurrences(of: "\u{202F}", with: "")
        return "\(assetLink(m.asset).symbol) \(m.changePct >= 0 ? "+" : "−")\(digits) % depuis la publication"
    }

    /// "Règle : 2 sources indépendantes (+1) ; Concerne NVDA (+1) → 2 points, impact moyen par règle ; retenu : faible, d'après la
    /// variation mesurée (NVDA -0,4 % depuis la publication)."
    public static func ruleText(_ s: StorySummary) -> String {
        let rule = s.impactReasons.filter { $0.contains("(+") }.joined(separator: " ; ")
        let head = "Règle : \(rule) → \(s.impactPoints) point\(s.impactPoints > 1 ? "s" : ""), impact \(s.ruleImpact.label) par règle"
        guard s.measured else { return head + "." }
        return head + " ; retenu : \(s.impact.label), d'après la variation mesurée (\(s.impactReasons.filter { !$0.contains("(+") }.joined(separator: " ; ")))."
    }

    public static let intro = "Sujets des dernières 24 h repris par plusieurs sources indépendantes, escalades graves, ou articles citant vos actifs sur un sujet sensible. Impact potentiel indicatif, pas un signal."
    public static let howRule = "Par règle : sources indépendantes, un même titre repris mot pour mot comptant pour une (2–3 : +1, 4 et plus : +2), thème (escalade grave +2 ; banques centrales, régulation ou piratage +1), cite un de vos actifs (+1). 0–1 point : faible, 2–3 : moyen, 4 et plus : important."
    public static let howMeasured = "Mesuré : si les bougies horaires d'un actif cité sont déjà en mémoire sur le serveur, la plus forte variation depuis la clôture précédant la publication (action : 1 % moyen, 3 % important ; crypto : 2 % et 5 %). Une variation après un article ne prouve pas qu'il en est la cause."
    public static let howConsensus = "Consensus : ton des titres de chaque source (repérage par mots-clés) ; divergent dès qu'un titre est négatif et un autre positif."
}
