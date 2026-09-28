import Foundation
import XCTest
@testable import AltimKit

private let dayMs = 86_400_000.0
private func d(_ iso: String) -> Double { ISO8601DateFormatter().date(from: "\(iso)T00:00:00Z")!.timeIntervalSince1970 * 1000 }

final class HistoryTests: XCTestCase {
    /// Real answer of /api/history (ETH, AAPL + Bitcoin and SPY), 28 September 2026: the same file as the site's test.
    func testRealCloses() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "history", withExtension: "json", subdirectory: "Fixtures"))
        let r = try JSONDecoder().decode(HistoryResponse.self, from: Data(contentsOf: url))
        let h = try XCTUnwrap(PortfolioHistory.compute([("crypto:ETH", 2), ("stock:AAPL", 10)], series: r.byId, days: 90, now: Date(timeIntervalSince1970: r.asOf / 1000)))
        XCTAssertEqual(h.points.count, 91)
        XCTAssertFalse(h.shortened)
        XCTAssertEqual(h.points.last!.t, d("2026-09-27"))
        XCTAssertEqual(h.points.last!.value, 2 * 2688.32 + 10 * 341.07, accuracy: 0.1)
        for i in 1..<h.points.count { XCTAssertEqual(h.points[i].t - h.points[i - 1].t, dayMs) }
        XCTAssertEqual(h.benchmarks.map(\.label), ["Bitcoin", "S&P 500 (SPY)"])
        XCTAssertEqual(h.pct.first, 0)
        // Same figures as the site and Android on the same file.
        XCTAssertEqual(h.change, 45.5, accuracy: 0.05)
        XCTAssertEqual(h.benchmarks[0].change, 40.3, accuracy: 0.05)
        XCTAssertEqual(h.maxDrawdown, -6.8, accuracy: 0.05)
    }

    func testWeekendLateListingAndDrawdown() throws {
        let h = try XCTUnwrap(PortfolioHistory.compute([("stock:X", 1)], series: ["stock:X": [(d("2026-09-18"), 100), (d("2026-09-21"), 110)]], days: 3,
                                                        now: Date(timeIntervalSince1970: (d("2026-09-21") + 3_600_000) / 1000)))
        XCTAssertEqual(h.points.map(\.value), [100, 100, 100, 110])
        XCTAssertEqual(h.best!.change, 10, accuracy: 1e-9)
        XCTAssertNil(h.worst)

        let late: [String: [(Double, Double)]] = [
            "crypto:A": (0...10).map { (d("2026-09-01") + Double($0) * dayMs, 10) },
            "crypto:NEW": (0...3).map { (d("2026-09-08") + Double($0) * dayMs, 5) },
        ]
        let h2 = try XCTUnwrap(PortfolioHistory.compute([("crypto:A", 1), ("crypto:NEW", 2)], series: late, days: 10, now: Date(timeIntervalSince1970: (d("2026-09-11") + 1) / 1000)))
        XCTAssertTrue(h2.shortened)
        XCTAssertEqual(h2.points.first!.t, d("2026-09-08"))
        XCTAssertEqual(h2.change, 0)

        let closes = [100.0, 120, 90, 108, 60, 80].enumerated().map { (d("2026-09-01") + Double($0.offset) * dayMs, $0.element) }
        let h3 = try XCTUnwrap(PortfolioHistory.compute([("crypto:A", 1), ("crypto:GONE", 3)], series: ["crypto:A": closes], days: 5, now: Date(timeIntervalSince1970: (d("2026-09-06") + 1) / 1000)))
        XCTAssertEqual(h3.maxDrawdown, -50, accuracy: 1e-9)
        XCTAssertEqual(h3.missing, ["crypto:GONE"])
        XCTAssertNil(PortfolioHistory.compute([("crypto:GONE", 1)], series: [:], days: 30))
    }
}

final class NewsAlertTests: XCTestCase {
    let now = Date(timeIntervalSince1970: (d("2026-09-28") + 8 * 3_600_000) / 1000)

    func item(_ id: String, _ title: String, hoursAgo: Double = 1, alert: Bool = false, assets: [String] = [], alsoIn: Int = 0) throws -> NewsItem {
        let json: [String: Any] = [
            "id": id, "title": title, "link": "https://ex.com/\(id)", "time": now.timeIntervalSince1970 * 1000 - hoursAgo * 3_600_000, "source": "S",
            "lang": "en", "category": "monde", "themes": [String](), "tone": "neutral", "assets": assets, "alsoIn": (0..<alsoIn).map { "O\($0)" }, "alert": alert,
        ]
        return try JSONDecoder().decode(NewsItem.self, from: JSONSerialization.data(withJSONObject: json))
    }

    func testRule() throws {
        let owned: Set<String> = ["crypto:BTC"]
        let items = [
            try item("1", "Russia declares war on neighbour, markets slide", alert: true, alsoIn: 1),
            try item("6", "AI agents could trigger a bank run, says economist", alert: true), // one source: an opinion
            try item("2", "Bitcoin ETF inflows reach record as price jumps", assets: ["crypto:BTC"], alsoIn: 2),
            try item("3", "Bitcoin miners expand in Texas", assets: ["crypto:BTC"], alsoIn: 1),
            try item("4", "Ethereum upgrade goes live", assets: ["crypto:ETH"], alsoIn: 5),
            try item("5", "Bank collapse sparks contagion fears", hoursAgo: 7, alert: true),
        ]
        var tracker = NewsAlertTracker()
        XCTAssertEqual(tracker.newAlerts(items, owned: owned, now: now).map(\.id), ["1", "2"])
        let again = items + [try item("9", "Russia declares war on its neighbour, markets slide", alert: true, alsoIn: 3)]
        XCTAssertEqual(tracker.newAlerts(again, owned: owned, now: now.addingTimeInterval(900)).map(\.id), [])
        _ = tracker.newAlerts([], owned: owned, now: now.addingTimeInterval(49 * 3600))
        XCTAssertTrue(tracker.seen.isEmpty)
    }
}
