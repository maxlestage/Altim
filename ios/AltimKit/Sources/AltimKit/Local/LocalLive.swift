import Foundation

/// Live prices without a server: the consensus quotes read again every few seconds by the iPhone (5 s for cryptos,
/// 15 s for stocks, 60 s for a closed stock market), as ticks of the same stream as the server's WebSocket.
enum LocalLive {
    static func messages(_ assets: [Asset], local: LocalBackend, opened: @escaping @Sendable () -> Void = {}) -> AsyncThrowingStream<LiveMessage, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                continuation.yield(.hello(version: 1))
                opened()
                var last: [String: Double] = [:]
                var failures = 0
                while !Task.isCancelled {
                    let now = Date()
                    let due = assets.filter { a in
                        let every: Double = a.kind == .crypto ? 5 : marketOpen(now) ? 15 : 60
                        return (last[a.id].map { now.timeIntervalSince1970 - $0 >= every } ?? true)
                    }
                    if !due.isEmpty {
                        let quotes = await local.quotes(due)
                        failures = quotes.isEmpty ? failures + 1 : 0
                        // Several rounds without a single price: the network is gone, the caller reconnects later.
                        if failures >= 3 { return continuation.finish(throwing: MarketError("aucun cours")) }
                        for a in due { last[a.id] = now.timeIntervalSince1970 }
                        for q in quotes {
                            continuation.yield(.tick(LiveTick(symbol: q.symbol, kind: q.kind, price: q.price, change: q.change, agreeing: q.agreeing,
                                                              total: q.total, sources: q.sources.filter(\.ok).map(\.name),
                                                              time: now.timeIntervalSince1970 * 1000,
                                                              market: q.kind == .stock ? (marketOpen(now) ? "open" : "closed") : nil)))
                        }
                    }
                    try? await Task.sleep(nanoseconds: 1_000_000_000)
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    /// New York regular session: Monday to Friday, 9:30 to 16:00 (13:00 on early-close days), not on exchange holidays.
    static func marketOpen(_ date: Date) -> Bool {
        let cal = NYSECalendar.calendar
        let c = cal.dateComponents([.hour, .minute], from: date)
        guard let close = NYSECalendar.closeMinutes(date), let h = c.hour, let m = c.minute else { return false }
        let minutes = h * 60 + m
        return minutes >= 570 && minutes < close
    }
}

/// The New York Stock Exchange calendar from its fixed rules (nothing to update each year): full-day holidays and the
/// 13:00 early closes (3 July, the day after Thanksgiving, 24 December).
public enum NYSECalendar {
    static let calendar: Calendar = {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "America/New_York") ?? TimeZone(secondsFromGMT: -5 * 3600)!
        return c
    }()

    static func key(_ y: Int, _ m: Int, _ d: Int) -> Int { y * 10_000 + m * 100 + d }

    static func weekday(_ y: Int, _ m: Int, _ d: Int) -> Int {
        calendar.date(from: DateComponents(year: y, month: m, day: d, hour: 12)).map { calendar.component(.weekday, from: $0) } ?? 0
    }

    /// The n-th given weekday (1 = Sunday) of a month; n = -1 for the last one.
    static func nth(_ y: Int, _ m: Int, weekday w: Int, _ n: Int) -> Int {
        if n > 0 { return 1 + (w - weekday(y, m, 1) + 7) % 7 + 7 * (n - 1) }
        let last = calendar.range(of: .day, in: .month, for: calendar.date(from: DateComponents(year: y, month: m, day: 1, hour: 12))!)!.count
        return last - (weekday(y, m, last) - w + 7) % 7
    }

    /// Easter Sunday (anonymous Gregorian algorithm): (month, day).
    static func easter(_ y: Int) -> (Int, Int) {
        let a = y % 19, b = y / 100, c = y % 100, d = b / 4, e = b % 4, f = (b + 8) / 25, g = (b - f + 1) / 3
        let h = (19 * a + b - d - g + 15) % 30, i = c / 4, k = c % 4, l = (32 + 2 * e + 2 * i - h - k) % 7, m = (a + 11 * h + 22 * l) / 451
        let month = (h + l - 7 * m + 114) / 31, day = (h + l - 7 * m + 114) % 31 + 1
        return (month, day)
    }

    /// The exchange's full-day holidays of a year, as yyyymmdd.
    public static func holidays(_ y: Int) -> Set<Int> {
        var out: Set<Int> = []
        // A fixed-date holiday on Saturday is observed the Friday before, on Sunday the Monday after (1 January on a
        // Saturday is not observed: the exchange does not close on the last day of the year).
        func fixed(_ m: Int, _ d: Int, observeSaturday: Bool = true) {
            switch weekday(y, m, d) {
            case 7: if observeSaturday, let t = calendar.date(from: DateComponents(year: y, month: m, day: d - 1, hour: 12)) { add(t) }
            case 1: if let t = calendar.date(from: DateComponents(year: y, month: m, day: d + 1, hour: 12)) { add(t) }
            default: out.insert(key(y, m, d))
            }
        }
        func add(_ t: Date) { let c = calendar.dateComponents([.year, .month, .day], from: t); out.insert(key(c.year!, c.month!, c.day!)) }
        fixed(1, 1, observeSaturday: false)
        out.insert(key(y, 1, nth(y, 1, weekday: 2, 3)))   // Martin Luther King Jr. Day
        out.insert(key(y, 2, nth(y, 2, weekday: 2, 3)))   // Washington's Birthday
        let (em, ed) = easter(y)
        if let goodFriday = calendar.date(from: DateComponents(year: y, month: em, day: ed - 2, hour: 12)) { add(goodFriday) }
        out.insert(key(y, 5, nth(y, 5, weekday: 2, -1)))  // Memorial Day
        if y >= 2022 { fixed(6, 19) }                     // Juneteenth
        fixed(7, 4)
        out.insert(key(y, 9, nth(y, 9, weekday: 2, 1)))   // Labor Day
        out.insert(key(y, 11, nth(y, 11, weekday: 5, 4))) // Thanksgiving
        fixed(12, 25)
        return out
    }

    /// Closing time of the session that day, in minutes after midnight New York; nil when the exchange is closed.
    public static func closeMinutes(_ date: Date) -> Int? {
        let c = calendar.dateComponents([.year, .month, .day, .weekday], from: date)
        guard let y = c.year, let m = c.month, let d = c.day, let wd = c.weekday, (2...6).contains(wd) else { return nil }
        let today = key(y, m, d)
        guard !holidays(y).contains(today) else { return nil }
        let early = [key(y, 7, 3), key(y, 12, 24), key(y, 11, nth(y, 11, weekday: 5, 4) + 1)]
        return early.contains(today) ? 780 : 960
    }

    /// True on a weekday the exchange is open (even for a short session).
    public static func isTradingDay(_ date: Date) -> Bool { closeMinutes(date) != nil }
}
