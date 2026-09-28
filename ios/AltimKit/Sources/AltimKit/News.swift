import Foundation

/// News section (/api/news): articles from ~20 sources (FR + EN), stories told by several sources merged.
public struct NewsItem: Codable, Sendable, Identifiable, Hashable {
    public var id: String
    public var title: String
    public var link: String
    public var time: Double
    public var source: String
    public var summary: String?
    public var lang: String
    /// "monde", "marches", "crypto" or "actifs" (names one of the user's assets).
    public var category: String
    public var themes: [String]
    /// "negative", "positive" or "neutral" (keywords of the title, indicative).
    public var tone: String
    /// "crypto:BTC"… assets of the user named in the title.
    public var assets: [String]
    /// Other sources that told the same story.
    public var alsoIn: [String]
    /// Serious escalation (war declared, invasion, bank run…).
    public var alert: Bool

    /// Only http(s) links are opened (the server already filters, checked again here).
    public var url: URL? {
        guard let u = URL(string: link), let scheme = u.scheme?.lowercased(), scheme == "https" || scheme == "http" else { return nil }
        return u
    }

    public static let themeLabels: [String: String] = [
        "geopolitics": "Géopolitique / guerre", "monetary": "Banques centrales / taux", "trade": "Commerce / droits de douane",
        "stress": "Crise / krach", "regulation": "Régulation", "earnings": "Résultats d'entreprises",
    ]

    /// "il y a 5 min", "il y a 3 h", "il y a 2 j".
    public func age(now: Date = Date()) -> String { Self.ago(time, now: now) }

    public static func ago(_ time: Double, now: Date = Date()) -> String {
        let m = max(0, Int((now.timeIntervalSince1970 * 1000 - time) / 60_000))
        if m < 1 { return "à l'instant" }
        if m < 60 { return "il y a \(m) min" }
        let h = Int((Double(m) / 60).rounded())
        return h < 24 ? "il y a \(h) h" : "il y a \(Int((Double(h) / 24).rounded())) j"
    }
}

public struct NewsReport: Codable, Sendable {
    public struct Digest: Codable, Sendable {
        public struct Theme: Codable, Sendable, Identifiable { public var theme: String; public var label: String; public var count: Int; public var id: String { theme } }
        public struct Tone: Codable, Sendable { public var negative: Int; public var positive: Int; public var neutral: Int }
        public var total: Int
        public var themes: [Theme]
        public var tone: Tone
    }
    public struct Source: Codable, Sendable, Identifiable { public var name: String; public var ok: Bool; public var count: Int; public var error: String?; public var id: String { name } }

    public var asOf: Double
    public var items: [NewsItem]
    public var top: [String]
    public var digest: Digest
    public var sources: [Source]

    public var topItems: [NewsItem] { top.compactMap { id in items.first { $0.id == id } } }
}

extension AltimClient {
    /// News of the world, the markets, crypto and these assets (20 at most).
    public func news(_ assets: [Asset]) async throws -> NewsReport {
        try await getNews(Array(assets.prefix(20)))
    }
}
