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

    /// Query of /api/calendar: no symbol, the whole calendar (filtered on the device when "Mes actifs" is on). `top`:
    /// the company events of `symbols` and of the largest companies together (the risk view), only with symbols.
    public static func query(days: Int, symbols: [String]?, top: Bool = false) -> [String: String] {
        var q = ["days": String(days)]
        if let symbols, !symbols.isEmpty {
            q["symbols"] = symbols.joined(separator: ",")
            if top { q["top"] = "1" }
        }
        return q
    }

    // MARK: Risk by day (web calendar.ts `riskDays`)

    /// 🔴 high, 🟠 medium, 🟢 low.
    public enum RiskLevel: String, Sendable, Hashable {
        case high, medium, low
        public var icon: String {
            switch self {
            case .high: return "🔴"
            case .medium: return "🟠"
            case .low: return "🟢"
            }
        }
        public var label: String {
            switch self {
            case .high: return "Risque élevé"
            case .medium: return "Risque modéré"
            case .low: return "Risque faible"
            }
        }
        var rank: Int { self == .high ? 2 : self == .medium ? 1 : 0 }
    }

    /// The categories whose high-importance releases make a 🔴 day.
    private static let major: Set<String> = ["tauxDirecteurs", "inflation", "emploi", "pib"]

    /// Risk of one event, nil when it does not count (IPOs, other companies' dividends and splits).
    /// - 🔴: a high-importance central bank decision / inflation (CPI) / jobs / GDP release, or the earnings of a stock
    ///   the user holds or watches;
    /// - 🟠: the other macro and central bank events, the earnings of other companies (the calendar keeps the largest
    ///   US ones only), a dividend or split of a held stock.
    public static func eventRisk(_ e: CalendarEvent, held: [String], watched: [String]) -> RiskLevel? {
        let sym = e.symbol.map(norm) ?? ""
        let isHeld = !sym.isEmpty && held.contains { norm($0) == sym }
        let isMine = isHeld || (!sym.isEmpty && watched.contains { norm($0) == sym })
        switch e.kind {
        case .macro, .centralBank: return e.importance == "high" && major.contains(e.category) ? .high : .medium
        case .earnings: return isMine ? .high : .medium
        case .dividend, .split: return isHeld ? .medium : nil
        default: return nil
        }
    }

    private static let acronym = try? NSRegularExpression(pattern: #"\(([^)]*[A-Z]{2,}[^)]*)\)\s*$"#)

    /// Short name of an event for the risk row: "CPI", "Décision de taux de la Fed", "Résultats AAPL"…
    public static func shortTitle(_ e: CalendarEvent) -> String {
        if let s = e.symbol {
            switch e.kind {
            case .earnings: return "Résultats \(s)"
            case .dividend: return "Dividende \(s)"
            case .split: return "Split \(s)"
            default: break
            }
        }
        // The acronym in brackets when there is one ("Inflation (CPI)" → "CPI"), else the whole title.
        var name = e.title
        if e.kind == .macro, let re = acronym,
           let m = re.firstMatch(in: e.title, range: NSRange(e.title.startIndex..., in: e.title)),
           let r = Range(m.range(at: 1), in: e.title) {
            name = String(e.title[r])
        }
        if e.kind == .macro, let c = e.country, !c.isEmpty, c != "États-Unis" { return "\(name) (\(c))" }
        return name
    }

    public struct RiskEvent: Sendable {
        public var event: CalendarEvent
        public var risk: RiskLevel
    }

    public struct RiskDay: Sendable, Identifiable {
        /// "YYYY-MM-DD".
        public var day: String
        /// "Lun. 28 sept."
        public var label: String
        public var weekend: Bool
        public var level: RiskLevel
        /// Short names of the events at the day's level, in the calendar's order, without repeats.
        public var main: [String]
        /// Every event that counts, with its risk.
        public var events: [RiskEvent]
        /// A source could not be read for this day: 🟢 is then not asserted.
        public var incomplete: Bool
        public var id: String { day }
        /// Sources incomplete and nothing found: the risk is not assessed (⚪).
        public var unknown: Bool { incomplete && level == .low }
    }

    private static var utc: Calendar {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "UTC")!
        return c
    }

    private static func parseDay(_ day: String) -> Date? {
        let p = day.split(separator: "-").compactMap { Int($0) }
        guard p.count == 3 else { return nil }
        return utc.date(from: DateComponents(year: p[0], month: p[1], day: p[2], hour: 12))
    }

    private static let weekdays = ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."]
    static let months = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."]

    /// "Lun. 28 sept." (a calendar day, read without time zone).
    public static func riskDayLabel(_ day: String) -> String {
        guard let d = parseDay(day) else { return day }
        let c = utc.dateComponents([.weekday, .day, .month], from: d)
        let w = weekdays[((c.weekday ?? 1) - 1) % 7]
        return "\(w.prefix(1).uppercased())\(w.dropFirst()) \(c.day ?? 1) \(months[((c.month ?? 1) - 1) % 12])"
    }

    /// The `n` days from `from` ("YYYY-MM-DD", the report's first day, Paris time), weekends included, each with its
    /// risk: the highest of its events' (`eventRisk`), 🟢 when none counts. `failed`: days a source could not read.
    public static func riskDays(_ events: [CalendarEvent], from: String, held: [String], watched: [String], n: Int = 7, failed: [String] = []) -> [RiskDay] {
        guard let start = parseDay(from) else { return [] }
        let cal = utc
        return (0..<max(0, n)).compactMap { i -> RiskDay? in
            guard let date = cal.date(byAdding: .day, value: i, to: start) else { return nil }
            let c = cal.dateComponents([.year, .month, .day, .weekday], from: date)
            let day = String(format: "%04d-%02d-%02d", c.year ?? 1970, c.month ?? 1, c.day ?? 1)
            let counted = events.filter { $0.day == day }.compactMap { e in eventRisk(e, held: held, watched: watched).map { RiskEvent(event: e, risk: $0) } }
            let level = counted.reduce(RiskLevel.low) { $1.risk.rank > $0.rank ? $1.risk : $0 }
            var seen = Set<String>()
            let main = counted.filter { $0.risk == level }.map { shortTitle($0.event) }.filter { seen.insert($0).inserted }
            let dow = c.weekday ?? 2
            return RiskDay(day: day, label: riskDayLabel(day), weekend: dow == 1 || dow == 7, level: level, main: main, events: counted,
                           incomplete: failed.contains(day))
        }
    }

    /// Days a source could not read ("YYYY-MM-DD" only: the IPO months do not change the risk).
    public static func failedDays(_ report: CalendarReport) -> [String] {
        var seen = Set<String>()
        return report.sources.flatMap { ($0.failed ?? []).filter { $0.count == 10 } }.filter { seen.insert($0).inserted }
    }

    /// Text of a risk row: "Sources incomplètes : risque non évalué", "Aucun événement majeur", or the main events
    /// (3 at most, "+2" for the others).
    public static func riskText(_ d: RiskDay) -> String {
        if d.unknown { return "Sources incomplètes : risque non évalué" }
        if d.level == .low { return "Aucun événement majeur" }
        let extra = d.main.count > 3 ? " +\(d.main.count - 3)" : ""
        return d.main.prefix(3).joined(separator: " · ") + extra
    }

    public static let riskRule = "🔴 décision de taux d'une banque centrale, inflation (CPI), emploi ou PIB d'importance haute, ou résultats d'une action de vos avoirs ou de votre radar. 🟠 autres publications économiques et banques centrales, résultats des grandes capitalisations américaines, dividende ou split d'une action détenue. 🟢 aucun de ces événements. Heures et jours de Paris ; seules les sources de l'Agenda sont prises en compte (voir « Non couvert »)."
}

extension AltimClient {
    /// Agenda of the next `days` days; `symbols` limits the company events to these stocks.
    public func calendar(days: Int, symbols: [String]?, top: Bool = false) async throws -> CalendarReport {
        try await getCalendar(Agenda.query(days: days, symbols: symbols, top: top))
    }
}
