import Foundation
import XCTest
@testable import AltimKit

final class LocalEarningsTests: XCTestCase {
    func fixture() throws -> Any {
        let url = Bundle.module.url(forResource: "nasdaq-earnings", withExtension: "json", subdirectory: "Fixtures")!
        return try JSONSerialization.jsonObject(with: Data(contentsOf: url))
    }

    func testParsesNasdaqsRealCalendar() throws {
        let events = try EarningsCalendar.parse(fixture(), date: "2026-09-30")
        let mu = try XCTUnwrap(events.first { $0.symbol == "MU" })
        XCTAssertEqual(mu.name, "Micron Technology, Inc.")
        XCTAssertEqual(mu.moment, "après la clôture")
        XCTAssertEqual(mu.epsForecast, "$31.24")
        XCTAssertEqual(mu.lastYearEPS, "$2.86")
        XCTAssertEqual(mu.dayText, "mercredi 30 septembre, après la clôture")
        XCTAssertEqual(events.first { $0.symbol == "JBL" }?.moment, "avant l'ouverture")
        XCTAssertThrowsError(try EarningsCalendar.parse(["status": ["rCode": 400]], date: "2026-09-30"))
        XCTAssertEqual(try EarningsCalendar.parse(["data": NSNull()], date: "2026-09-30"), [])
    }

    func testWeekdaysOnly() {
        // Friday 30 October 2026, 12:00 UTC: Fri, then Mon–Fri, then Mon.
        let days = EarningsCalendar.weekdays(from: Date(timeIntervalSince1970: 1_793_361_600), days: 4)
        XCTAssertEqual(days, ["2026-10-30", "2026-11-02"])
    }

    /// The exchange's published calendars (nyse.com): holidays observed on Friday or Monday, Good Friday, early closes.
    func testNYSEHolidaysAndEarlyCloses() {
        XCTAssertEqual(NYSECalendar.holidays(2026), [20260101, 20260119, 20260216, 20260403, 20260525, 20260619, 20260703, 20260907, 20261126, 20261225])
        XCTAssertEqual(NYSECalendar.holidays(2027), [20270101, 20270118, 20270215, 20270326, 20270531, 20270618, 20270705, 20270906, 20271125, 20271224])
        XCTAssertFalse(NYSECalendar.holidays(2022).contains(20220101), "1er janvier un samedi : pas de fermeture")
        func ny(_ y: Int, _ m: Int, _ d: Int, _ h: Int, _ min: Int = 0) -> Date {
            NYSECalendar.calendar.date(from: DateComponents(year: y, month: m, day: d, hour: h, minute: min))!
        }
        XCTAssertTrue(LocalLive.marketOpen(ny(2026, 11, 25, 15)))
        XCTAssertFalse(LocalLive.marketOpen(ny(2026, 11, 26, 11)), "Thanksgiving")
        XCTAssertTrue(LocalLive.marketOpen(ny(2026, 11, 27, 12, 30)))
        XCTAssertFalse(LocalLive.marketOpen(ny(2026, 11, 27, 13, 30)), "lendemain de Thanksgiving : clôture à 13 h")
        XCTAssertFalse(LocalLive.marketOpen(ny(2026, 12, 24, 14)), "veille de Noël : clôture à 13 h")
        XCTAssertFalse(LocalLive.marketOpen(ny(2026, 12, 25, 11)))
        XCTAssertFalse(LocalLive.marketOpen(ny(2026, 10, 31, 11)), "samedi")
        XCTAssertFalse(LocalLive.marketOpen(ny(2026, 11, 2, 9, 29)))
        XCTAssertTrue(LocalLive.marketOpen(ny(2026, 11, 2, 9, 30)))
        XCTAssertFalse(LocalLive.marketOpen(ny(2026, 11, 2, 16)))
        // Earnings pages: no page asked for a closed day.
        XCTAssertEqual(EarningsCalendar.weekdays(from: ny(2026, 11, 25, 8), days: 5), ["2026-11-25", "2026-11-27"])
    }

    func testRouteKeepsTheFirstReportOfEachStock() async throws {
        let body = String(data: try JSONSerialization.data(withJSONObject: fixture()), encoding: .utf8)!
        let now = Date(timeIntervalSince1970: 1_793_361_600) // Friday 30 Oct 2026
        let backend = LocalBackend(fetcher: FakeFetcher(routes: [("https://api.nasdaq.com/api/calendar/earnings?date=2026-11-03", body)]))
        let events = await backend.earnings(["mu", "AAPL"], now: now)
        XCTAssertEqual(events.map(\.symbol), ["MU"])
        XCTAssertEqual(events.first?.date, "2026-11-03")
        XCTAssertEqual(events.first?.daysAway(now: now), 4)
    }

    func testBuyAlertWarnsBeforeResults() throws {
        Money.setDisplay(.usd, nil)
        let now = Date(timeIntervalSince1970: 1_793_361_600)
        let e = EarningsEvent(symbol: "AAPL", name: "Apple", date: "2026-11-02", time: nyOpen(2026, 11, 2)!, moment: "après la clôture", epsForecast: nil, lastYearEPS: nil)
        let v = TrendEngine.Verdict(state: .trend, action: .buy, score: 30, confidence: 100, momentum: 0.1, lookback: 62, ema200: 300, rsi2: 40, text: "Tendance.")
        let o = LocalAlertRule.evaluate(.init(asset: Asset(symbol: "AAPL", kind: .stock, name: "Apple"), price: 340, verdict: v,
                                              reliability: SignalEngine.reliability(quality: 100, independent: 3, conflict: false), zones: [],
                                              earnings: e, now: now))
        XCTAssertTrue(o.buy)
        XCTAssertTrue(o.cautions.contains { $0.hasPrefix("Résultats trimestriels dans 3 jours (lundi 2 novembre, après la clôture)") })
    }
}
