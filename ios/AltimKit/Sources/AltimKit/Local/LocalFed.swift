import Foundation

/// Rate decisions of the US Federal Reserve (FOMC), read on the device from the calendar the Fed publishes
/// (federalreserve.gov/monetarypolicy/fomccalendars.htm): the last day of each meeting, when the decision is announced
/// (14:00 New York time). Notation votes without a meeting are left out.
enum FedCalendar {
    static let url = "https://www.federalreserve.gov/monetarypolicy/fomccalendars.htm"
    static let months = ["january": 1, "february": 2, "march": 3, "april": 4, "may": 5, "june": 6, "july": 7, "august": 8,
                         "september": 9, "october": 10, "november": 11, "december": 12]

    /// Decision days (ms, 18:00 UTC ≈ 14:00 in New York), oldest first.
    static func parse(_ html: String) -> [Double] {
        var out: [Double] = []
        let years = matches(#"(\d{4}) FOMC Meetings</a>"#, html)
        for (k, y) in years.enumerated() {
            guard let year = Int(y.groups[0]) else { continue }
            let end = k + 1 < years.count ? years[k + 1].range.lowerBound : html.endIndex
            let panel = String(html[y.range.upperBound..<end])
            for m in matches(#"fomc-meeting__month[^>]*><strong>([^<]+)</strong>[\s\S]*?fomc-meeting__date[^>]*>([^<]+)</div>"#, panel) {
                let monthText = m.groups[0].lowercased(), dateText = m.groups[1].trimmingCharacters(in: .whitespaces)
                guard !dateText.contains("notation") else { continue }
                // "27-28", "17-18*", across two months "April/May" with "30-1": the decision is on the last day.
                let names = monthText.split(separator: "/").map { $0.trimmingCharacters(in: .whitespaces) }
                guard let lastName = names.last, let month = months[lastName] else { continue }
                let days = dateText.replacingOccurrences(of: "*", with: "").split(separator: "-").compactMap { Int($0.trimmingCharacters(in: .whitespaces)) }
                guard let day = days.last, (1...31).contains(day) else { continue }
                var c = DateComponents(timeZone: TimeZone(identifier: "UTC"), year: year, month: month, day: day, hour: 18)
                c.calendar = Calendar(identifier: .gregorian)
                if let d = c.date { out.append(d.timeIntervalSince1970 * 1000) }
            }
        }
        return out.sorted()
    }

    /// The next decision from `now` (the day itself included until its end in New York).
    static func next(_ decisions: [Double], now: Double) -> Double? {
        decisions.first { FedDay.days($0, now: now) >= 0 }
    }

    private static func matches(_ pattern: String, _ text: String) -> [(range: Range<String.Index>, groups: [String])] {
        guard let re = try? NSRegularExpression(pattern: pattern) else { return [] }
        return re.matches(in: text, range: NSRange(text.startIndex..., in: text)).compactMap { m in
            guard let r = Range(m.range, in: text) else { return nil }
            let groups = (1..<m.numberOfRanges).compactMap { Range(m.range(at: $0), in: text).map { String(text[$0]) } }
            return (r, groups)
        }
    }
}

extension LocalBackend {
    /// The Fed's decision days, read at most twice a day. A page that cannot be read, or no longer parses (its layout
    /// changed), keeps the last calendar read; an empty list only when none was ever read.
    func fedDecisions() async -> [Double] {
        if let c = fedCache, nowMs() - c.at < 12 * 3_600_000 { return c.days }
        guard let html = try? await sharedFetcher.text(FedCalendar.url) else { return fedCache?.days ?? [] }
        let days = FedCalendar.parse(html)
        guard !days.isEmpty else { return fedCache?.days ?? [] }
        fedCache = (nowMs(), days)
        return days
    }

    /// For the tests: a calendar as if read at `at` (ms).
    func setFedCache(_ days: [Double], at: Double) { fedCache = (at, days) }
}
