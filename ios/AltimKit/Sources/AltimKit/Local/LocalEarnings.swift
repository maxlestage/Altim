import Foundation

/// A company's next quarterly results, from Nasdaq's public earnings calendar (one page per trading day, read by the
/// iPhone; backend/src/calendar.rs `parse::earnings`).
public struct EarningsEvent: Codable, Sendable, Equatable, Identifiable {
    public var symbol: String
    public var name: String
    /// "2026-11-05" (New York date of the report).
    public var date: String
    /// ms, 9:30 New York of that day.
    public var time: Double
    /// "avant l'ouverture", "après la clôture", or nil when not given.
    public var moment: String?
    /// Analysts' expected earnings per share, and last year's ("$1.23").
    public var epsForecast: String?
    public var lastYearEPS: String?
    public var id: String { "\(symbol):\(date)" }

    /// "jeudi 5 novembre, après la clôture".
    public var dayText: String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "fr_FR")
        f.timeZone = TimeZone(identifier: "America/New_York")
        f.dateFormat = "EEEE d MMMM"
        return f.string(from: Date(timeIntervalSince1970: time / 1000)) + (moment.map { ", \($0)" } ?? "")
    }

    /// Whole days from `now` to the report (0 = today).
    public func daysAway(now: Date = Date()) -> Int {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "America/New_York") ?? .current
        return cal.dateComponents([.day], from: cal.startOfDay(for: now), to: cal.startOfDay(for: Date(timeIntervalSince1970: time / 1000))).day ?? 0
    }
}

enum EarningsCalendar {
    static func cell(_ v: Any?) -> String? {
        guard let s = (v as? String)?.trimmingCharacters(in: .whitespaces), !s.isEmpty, s != "N/A", s != "--" else { return nil }
        return s
    }

    /// One day's page: `{"data": {"rows": [{symbol, name, time, epsForecast, lastYearEPS, …}]}}`.
    static func parse(_ d: Any, date: String) throws -> [EarningsEvent] {
        if let code = (at(d, "status", "rCode") as? NSNumber)?.intValue, code != 200 { throw MarketError("Nasdaq : statut \(code)") }
        let parts = date.split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3, let t = nyOpen(parts[0], parts[1], parts[2]) else { throw MarketError("date invalide") }
        let rows = arr(at(d, "data", "rows")) ?? []
        return rows.compactMap { obj($0) }.compactMap { r in
            guard let symbol = cell(r["symbol"])?.uppercased() else { return nil }
            let moment: String? = switch r["time"] as? String {
            case "time-pre-market": "avant l'ouverture"
            case "time-after-hours": "après la clôture"
            default: nil
            }
            return EarningsEvent(symbol: symbol.replacingOccurrences(of: "/", with: "."), name: cell(r["name"]) ?? symbol, date: date, time: t,
                                 moment: moment, epsForecast: cell(r["epsForecast"]), lastYearEPS: cell(r["lastYearEPS"]))
        }
    }

    /// The New York weekdays from today, `days` calendar days ahead ("2026-10-30"…).
    static func weekdays(from now: Date, days: Int) -> [String] {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "America/New_York") ?? .current
        let f = DateFormatter()
        f.calendar = cal
        f.timeZone = cal.timeZone
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd"
        return (0..<days).compactMap { cal.date(byAdding: .day, value: $0, to: now) }
            .filter { !cal.isDateInWeekend($0) }.map { f.string(from: $0) }
    }
}

extension LocalBackend {
    /// The next results of these stocks within 3 weeks (one Nasdaq page per trading day, each kept 12 hours).
    func earnings(_ symbols: [String], now: Date = Date()) async -> [EarningsEvent] {
        let wanted = Set(symbols.map { $0.uppercased() })
        guard !wanted.isEmpty else { return [] }
        let days = EarningsCalendar.weekdays(from: now, days: 21)
        let fetcher = self.fetcher
        var pages: [String: [EarningsEvent]] = [:]
        var missing: [String] = []
        for d in days {
            if let c = earningsCache[d], nowMs() - c.at < 12 * 3_600_000 { pages[d] = c.events } else { missing.append(d) }
        }
        let fetched: [(String, [EarningsEvent]?)] = await withTaskGroup(of: (String, [EarningsEvent]?).self) { g in
            for d in missing {
                g.addTask {
                    guard let j = try? await fetcher.json("https://api.nasdaq.com/api/calendar/earnings?date=\(d)") else { return (d, nil) }
                    return (d, try? EarningsCalendar.parse(j, date: d))
                }
            }
            var out: [(String, [EarningsEvent]?)] = []
            for await r in g { out.append(r) }
            return out
        }
        for (d, events) in fetched {
            guard let events else { continue }
            pages[d] = events
            earningsCache[d] = (nowMs(), events)
        }
        // The first report of each stock.
        var first: [String: EarningsEvent] = [:]
        for d in days {
            for e in pages[d] ?? [] where wanted.contains(e.symbol) && first[e.symbol] == nil { first[e.symbol] = e }
        }
        return first.values.sorted { ($0.time, $0.symbol) < ($1.time, $1.symbol) }
    }
}

extension AltimClient {
    /// The next quarterly results of these stocks within 3 weeks (computed on the iPhone).
    public func earnings(_ symbols: [String]) async throws -> [EarningsEvent] {
        let (data, status) = try await getWithStatus("/api/earnings", ["symbols": symbols.prefix(30).joined(separator: ",")])
        return try decodeResponse([EarningsEvent].self, data, status)
    }
}
