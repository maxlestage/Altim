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

    func testUnknownKindStillDecodes() throws {
        let json = #"{"date":0,"day":"2026-10-01","kind":"conference","category":"autre","importance":"medium","title":"X","source":"Source","url":"https://x.example"}"#
        let e = try JSONDecoder().decode(CalendarEvent.self, from: Data(json.utf8))
        XCTAssertEqual(e.kind, .unknown)
        XCTAssertEqual(e.categoryLabel, "autre")
        XCTAssertEqual(Agenda.filter([e], .macro, mine: nil).count, 0)
    }
}
