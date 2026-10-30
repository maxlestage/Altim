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
