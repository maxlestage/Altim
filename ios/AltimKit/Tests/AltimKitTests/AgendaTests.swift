import Foundation
import XCTest
@testable import AltimKit

/// Same cases as web/test/agenda.test.ts, on the same real /api/calendar answer (web/test/calendar-sample.json).
final class AgendaTests: XCTestCase {
    func report() throws -> CalendarReport {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "calendar-sample", withExtension: "json", subdirectory: "Fixtures"))
        return try JSONDecoder().decode(CalendarReport.self, from: Data(contentsOf: url))
    }

    let paris = TimeZone(identifier: "Europe/Paris")!

    func date(_ y: Int, _ m: Int, _ d: Int, _ h: Int) -> Date {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = paris
        return cal.date(from: DateComponents(year: y, month: m, day: d, hour: h))!
    }

    func testDayLabels() {
        let now = date(2026, 9, 28, 10)
        XCTAssertEqual(Agenda.dayLabel("2026-09-28", now: now, timeZone: paris), "Aujourd'hui")
        XCTAssertEqual(Agenda.dayLabel("2026-09-29", now: now, timeZone: paris), "Demain")
        XCTAssertEqual(Agenda.dayLabel("2026-09-30", now: now, timeZone: paris), "mer. 30 sept.")
        // Month end: tomorrow is the 1st of the next month.
        XCTAssertEqual(Agenda.dayLabel("2026-10-01", now: date(2026, 9, 30, 23), timeZone: paris), "Demain")
        // Same labels as the browser (Intl fr-FR).
        XCTAssertEqual(Agenda.dayLabel("2026-10-05", now: now, timeZone: paris), "lun. 5 oct.")
        XCTAssertEqual(Agenda.dayLabel("2026-12-06", now: now, timeZone: paris), "dim. 6 déc.")
        XCTAssertEqual(Agenda.dayLabel("2026-08-01", now: now, timeZone: paris), "sam. 1 août")
    }

    func testGroupedByDayInOrder() throws {
        let r = try report()
        let groups = Agenda.groupByDay(r.events)
        XCTAssertEqual(groups.map(\.day), Array(Set(r.events.map(\.day))).sorted())
        XCTAssertEqual(groups.map(\.events.count), [1, 3, 9, 3, 1])
        XCTAssertEqual(groups.reduce(0) { $0 + $1.events.count }, r.events.count)
    }

    func testFilters() throws {
        let events = try report().events
        XCTAssertEqual(events.count, 17)
        XCTAssertTrue(Agenda.filter(events, .ipo, mine: nil).allSatisfy { $0.kind == .ipo })
        XCTAssertEqual(Agenda.filter(events, .centralBank, mine: nil).count, 3)
        // Mine: MU's earnings stay, other companies and IPOs go, the economy and central banks stay.
        let mine = Agenda.filter(events, .all, mine: ["MU"])
        XCTAssertEqual(mine.count, 9)
        XCTAssertEqual(mine.filter { $0.kind == .earnings }.map(\.symbol), ["MU"])
        XCTAssertFalse(mine.contains { $0.kind == .ipo || $0.kind == .dividend })
        XCTAssertEqual(mine.filter { $0.kind == .macro }.count, events.filter { $0.kind == .macro }.count)
        // No stock at all: only the economy and central banks.
        let none = Agenda.filter(events, .all, mine: [])
        XCTAssertEqual(none.count, 8)
        XCTAssertTrue(none.allSatisfy { $0.kind == .macro || $0.kind == .centralBank })
    }

    func testEverEventHasItsSource() throws {
        let r = try report()
        for e in r.events {
            XCTAssertGreaterThan(e.source.count, 3)
            XCTAssertTrue(e.url.hasPrefix("https://"))
            XCTAssertNotNil(e.link)
            XCTAssertTrue(["high", "medium"].contains(e.importance))
            XCTAssertNotEqual(e.kind, .unknown)
            XCTAssertNotNil(Agenda.categoryLabels[e.category], e.category)
        }
        XCTAssertFalse(r.notCovered.isEmpty)
    }

    func testSymbolsAndQuery() {
        let a = [Asset(symbol: "aapl", kind: .stock, name: "Apple"), Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin"), Asset(symbol: "AAPL", kind: .stock, name: "Apple")]
        XCTAssertEqual(Agenda.stockSymbols(a), ["AAPL"])
        XCTAssertEqual(Agenda.query(days: 14, symbols: nil), ["days": "14"])
        XCTAssertEqual(Agenda.query(days: 14, symbols: []), ["days": "14"])
        XCTAssertEqual(Agenda.query(days: 30, symbols: ["AAPL", "BRK-B"]), ["days": "30", "symbols": "AAPL,BRK-B"])
        let url = AltimClient(baseURL: URL(string: "https://altim.example")!, credentials: nil).request("/api/calendar", query: Agenda.query(days: 30, symbols: ["AAPL", "BRK-B"])).url
        XCTAssertEqual(url?.absoluteString, "https://altim.example/api/calendar?days=30&symbols=AAPL,BRK-B")
    }

    func testRiskQuery() {
        // Risk view: top=1 only with symbols (without, the calendar already keeps the largest companies).
        XCTAssertEqual(Agenda.query(days: 7, symbols: ["AAPL"], top: true), ["days": "7", "symbols": "AAPL", "top": "1"])
        XCTAssertEqual(Agenda.query(days: 7, symbols: nil, top: true), ["days": "7"])
    }

    func testRiskCalendarSevenDaysWeekendIncluded() throws {
        let r = try report()
        // KDP held (its dividend counts), MU watched (its earnings count as high).
        let days = Agenda.riskDays(r.events, from: "2026-09-28", held: ["KDP"], watched: ["MU"], n: 7, failed: ["2026-10-03"])
        XCTAssertEqual(days.map(\.label), ["Lun. 28 sept.", "Mar. 29 sept.", "Mer. 30 sept.", "Jeu. 1 oct.", "Ven. 2 oct.", "Sam. 3 oct.", "Dim. 4 oct."])
        XCTAssertEqual(days.map(\.level), [.medium, .medium, .high, .high, .medium, .low, .low])
        XCTAssertEqual(days.map(\.weekend), [false, false, false, false, false, true, true])
        XCTAssertEqual(days[0].main, ["Dividende KDP"])
        // Other companies' earnings (large caps) and a central bank speech: 🟠; the others' dividends do not count.
        XCTAssertEqual(days[1].main, ["Résultats CCL", "Prise de parole de la présidence de la BCE"])
        XCTAssertFalse(days[1].events.contains { $0.event.symbol == "ERIC" })
        XCTAssertEqual(days[2].main, ["Résultats MU"])
        // US GDP and PCE (high); the UK's GDP (medium) is not among the main events.
        XCTAssertEqual(days[3].main, ["PIB", "Inflation PCE"])
        XCTAssertEqual(days[5].main, [])
        XCTAssertTrue(days[5].incomplete)
        XCTAssertTrue(days[5].unknown)
        XCTAssertFalse(days[6].incomplete)
        XCTAssertEqual(Agenda.riskText(days[5]), "Sources incomplètes : risque non évalué")
        XCTAssertEqual(Agenda.riskText(days[6]), "Aucun événement majeur")
        XCTAssertEqual(Agenda.riskText(days[3]), "PIB · Inflation PCE")
    }

    func testRiskRulePerEvent() throws {
        func ev(kind: String = "macro", category: String = "inflation", importance: String = "high", title: String = "Inflation (CPI)",
                symbol: String? = nil, country: String? = nil) throws -> CalendarEvent {
            var o: [String: Any] = ["date": 0, "day": "2026-10-06", "kind": kind, "category": category, "importance": importance, "title": title,
                                    "source": "s", "url": "https://x"]
            if let symbol { o["symbol"] = symbol }
            if let country { o["country"] = country }
            return try JSONDecoder().decode(CalendarEvent.self, from: JSONSerialization.data(withJSONObject: o))
        }
        XCTAssertEqual(Agenda.eventRisk(try ev(), held: [], watched: []), .high)
        XCTAssertEqual(Agenda.eventRisk(try ev(kind: "centralBank", category: "tauxDirecteurs"), held: [], watched: []), .high)
        XCTAssertEqual(Agenda.eventRisk(try ev(category: "activite", importance: "medium"), held: [], watched: []), .medium)
        XCTAssertEqual(Agenda.eventRisk(try ev(kind: "earnings", category: "resultats", symbol: "BRK.B"), held: ["BRK-B"], watched: []), .high)
        XCTAssertEqual(Agenda.eventRisk(try ev(kind: "earnings", category: "resultats", symbol: "NVDA"), held: [], watched: []), .medium)
        XCTAssertNil(Agenda.eventRisk(try ev(kind: "dividend", category: "dividende", symbol: "KO"), held: [], watched: ["KO"]))
        XCTAssertEqual(Agenda.eventRisk(try ev(kind: "split", category: "split", symbol: "KO"), held: ["KO"], watched: []), .medium)
        XCTAssertNil(Agenda.eventRisk(try ev(kind: "ipo", category: "ipo", symbol: "OURA"), held: ["OURA"], watched: []))
        XCTAssertEqual(Agenda.shortTitle(try ev()), "CPI")
        XCTAssertEqual(Agenda.shortTitle(try ev(country: "Allemagne")), "CPI (Allemagne)")
        XCTAssertEqual(Agenda.shortTitle(try ev(title: "Confiance des consommateurs (Conference Board)")), "Confiance des consommateurs (Conference Board)")
        var r = try report()
        r.sources = [CalendarReport.Source(name: "a", ok: false, failed: ["2026-10-01", "2026-10"], error: nil)]
        XCTAssertEqual(Agenda.failedDays(r), ["2026-10-01"])
    }

    func testUnknownKindStillDecodes() throws {
        let json = #"{"date":0,"day":"2026-10-01","kind":"conference","category":"autre","importance":"medium","title":"X","source":"Source","url":"https://x.example"}"#
        let e = try JSONDecoder().decode(CalendarEvent.self, from: Data(json.utf8))
        XCTAssertEqual(e.kind, .unknown)
        XCTAssertEqual(e.categoryLabel, "autre")
        XCTAssertEqual(Agenda.filter([e], .macro, mine: nil).count, 0)
    }
}
