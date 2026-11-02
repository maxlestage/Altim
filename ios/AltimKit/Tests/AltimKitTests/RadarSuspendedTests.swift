import XCTest
@testable import AltimKit

/// A Radar row held at ATTENDRE by the data (a single source, so low reliability) says so, apart from the rule's waits.
final class RadarSuspendedTests: XCTestCase {
    func testSingleSourceRowIsMarkedSuspended() async throws {
        let now = Date().timeIntervalSince1970 * 1000
        let bars = LocalBackendTests.trend(count: 400, step: 86_400_000, end: now - 3_600_000, slope: 0.003)
        let binance = "[" + bars.map { "[\(Int($0.time)),\"\($0.open)\",\"\($0.high)\",\"\($0.low)\",\"\($0.close)\",\"\($0.volume)\"]" }.joined(separator: ",") + "]"
        let backend = LocalBackend(fetcher: FakeFetcher(routes: [("https://api.binance.com/api/v3/klines", binance)]))
        let r = await backend.answer(path: "/api/radar", query: ["symbols": "BTC:crypto", "interval": "1d"])!
        let row = try XCTUnwrap(try JSONDecoder().decode([RadarRow].self, from: r.0).first)
        let s = try XCTUnwrap(row.signal, row.error ?? "pas de verdict")
        XCTAssertEqual(row.reliability?.level, "low")
        XCTAssertEqual(s.action, .hold)
        XCTAssertEqual(s.suspended, true, "attente imposée par les données, pas par la règle")
        XCTAssertEqual(DailyPoint.lines(macro: nil, rows: [row]), ["Votre actif suivi : 1 suspendu faute de données à jour ou fiables."])
    }
}
