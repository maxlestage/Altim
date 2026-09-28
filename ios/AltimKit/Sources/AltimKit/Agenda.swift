import Foundation

// Agenda (GET /api/calendar?days=&symbols=): economic releases, central banks, earnings, dividends, splits, IPOs,
// each with its source. Types and the pure helpers of the view (day labels, grouping, filters), same as
// web/src/webapp/calendar.ts.

public struct CalendarEvent: Codable, Sendable, Hashable {
    public enum Kind: String, Codable, Sendable, Hashable {
        case macro, earnings, dividend, split, ipo, centralBank, unknown
        public init(from decoder: Decoder) throws { self = Self(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .unknown }
    }

    /// Exact instant (ms) when the source gives a time, otherwise 00:00 UTC of `day`.
    public var date: Double
    /// "YYYY-MM-DD" (Paris time when the time is known).
    public var day: String
    /// "14:30" (Paris time) or, for earnings, "avant l'ouverture" / "après la clôture".
    public var time: String?
    public var kind: Kind
    /// "tauxDirecteurs" | "inflation" | "emploi" | "pib" | "activite" | "discours" | "resultats" | "dividende" | "split" | "ipo"
    public var category: String
    /// "high" | "medium"
    public var importance: String
    public var title: String
    public var originalName: String?
    public var country: String?
    public var symbol: String?
    public var actual: String?
    public var consensus: String?
    public var previous: String?
    public var detail: String?
    public var note: String?
    public var source: String
    public var url: String

    public var high: Bool { importance == "high" }
    public var categoryLabel: String { Agenda.categoryLabels[category] ?? category }

    /// Only http(s) links are opened.
    public var link: URL? {
        guard let u = URL(string: url), let scheme = u.scheme?.lowercased(), scheme == "https" || scheme == "http" else { return nil }
        return u
    }
}

public struct CalendarReport: Codable, Sendable {
    public struct Source: Codable, Sendable, Identifiable {
        public var name: String
        public var ok: Bool
        /// Days whose page failed.
        public var failed: [String]?
        public var error: String?
        public var id: String { name }
    }

    public var asOf: Double
    public var days: Int
    public var from: String
    public var to: String
    public var events: [CalendarEvent]
    public var sources: [Source]
    public var notCovered: [String]
}

public enum Agenda {
    public enum Filter: String, CaseIterable, Sendable {
        case all, macro, centralBank, earnings, dividend, split, ipo
        public var label: String {
            switch self {
            case .all: return "Tout"
            case .macro: return "Macro"
            case .centralBank: return "Banques centrales"
            case .earnings: return "Résultats"
            case .dividend: return "Dividendes"
            case .split: return "Splits"
            case .ipo: return "IPO"
            }
        }
    }

    /// Periods offered (days).
    public static let periods = [7, 14, 30]

    public static let categoryLabels: [String: String] = [
        "tauxDirecteurs": "Taux directeurs", "inflation": "Inflation", "emploi": "Emploi", "pib": "PIB", "activite": "Activité",
        "discours": "Discours", "resultats": "Résultats", "dividende": "Dividende", "split": "Split", "ipo": "IPO",
    ]

    private static func dayString(_ d: Date, _ calendar: Calendar) -> String {
        let c = calendar.dateComponents([.year, .month, .day], from: d)
        return String(format: "%04d-%02d-%02d", c.year ?? 1970, c.month ?? 1, c.day ?? 1)
    }

    /// "Aujourd'hui", "Demain", else "mer. 30 sept." (`now` and `timeZone`: the viewer's clock).
    public static func dayLabel(_ day: String, now: Date = Date(), timeZone: TimeZone = .current) -> String {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = timeZone
        if day == dayString(now, cal) { return "Aujourd'hui" }
        if let tomorrow = cal.date(byAdding: .day, value: 1, to: now), day == dayString(tomorrow, cal) { return "Demain" }
        let parts = day.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3, let date = cal.date(from: DateComponents(year: parts[0], month: parts[1], day: parts[2], hour: 12)) else { return day }
        // Written out rather than a "EEE d MMM" format: the browser's abbreviations on every system.
        let weekday = ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."][(cal.component(.weekday, from: date) - 1) % 7]
        let month = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."][(parts[1] - 1 + 12) % 12]
        return "\(weekday) \(parts[2]) \(month)"
    }

    private static let company: Set<CalendarEvent.Kind> = [.earnings, .dividend, .split]
    private static func norm(_ s: String) -> String { s.uppercased().replacingOccurrences(of: ".", with: "-").replacingOccurrences(of: "/", with: "-") }

    /// Events of the chosen filter. `mine` (the user's stock symbols) keeps the company events of these stocks only and
    /// leaves the IPOs out; the economy and central banks concern every asset and always stay.
    public static func filter(_ events: [CalendarEvent], _ filter: Filter, mine: [String]?) -> [CalendarEvent] {
        let own = mine.map { Set($0.map(norm)) }
        return events.filter { e in
            if filter != .all && e.kind.rawValue != filter.rawValue { return false }
            guard let own else { return true }
            if e.kind == .ipo { return false }
            return !company.contains(e.kind) || (e.symbol.map { own.contains(norm($0)) } ?? false)
        }
    }

    public struct DayGroup: Sendable, Identifiable {
        public var day: String
        public var events: [CalendarEvent]
        public var id: String { day }
    }

    /// Days in order, each with its events in the server's order.
    public static func groupByDay(_ events: [CalendarEvent]) -> [DayGroup] {
        var order: [String] = []
        var days: [String: [CalendarEvent]] = [:]
        for e in events {
            if days[e.day] == nil { order.append(e.day) }
            days[e.day, default: []].append(e)
        }
        return order.sorted().map { DayGroup(day: $0, events: days[$0] ?? []) }
    }

    /// The stock symbols of the user's radar and holdings (company events exist for stocks only), 50 at most.
    public static func stockSymbols(_ assets: [Asset]) -> [String] {
        var seen = Set<String>()
        return Array(assets.filter { $0.kind == .stock }.map { $0.symbol.uppercased() }.filter { seen.insert($0).inserted }.prefix(50))
    }

    /// Query of /api/calendar: no symbol, the whole calendar (filtered on the device when "Mes actifs" is on).
    public static func query(days: Int, symbols: [String]?) -> [String: String] {
        var q = ["days": String(days)]
        if let symbols, !symbols.isEmpty { q["symbols"] = symbols.joined(separator: ",") }
        return q
    }
}

extension AltimClient {
    /// Agenda of the next `days` days; `symbols` limits the company events to these stocks.
    public func calendar(days: Int, symbols: [String]?) async throws -> CalendarReport {
        try await getCalendar(Agenda.query(days: days, symbols: symbols))
    }
}
