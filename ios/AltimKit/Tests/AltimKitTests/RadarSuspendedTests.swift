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
        XCTAssertEqual(WidgetNote.text(row), "SUSPENDU", "le widget ne montre pas un simple ATTENDRE")
    }

    func testWidgetNotes() {
        func row(_ signal: SignalSummary?, error: String?) -> RadarRow {
            RadarRow(symbol: "X", kind: .stock, name: "X", price: nil, change: nil, priceSources: nil, signal: signal,
                     reliability: nil, agreeing: nil, sources: nil, sparkline: nil, error: error)
        }
        XCTAssertNil(WidgetNote.text(nil), "pas encore lu")
        XCTAssertNil(WidgetNote.text(row(SignalSummary(action: .hold, score: 0, confidence: 50), error: nil)), "attente de la règle")
        XCTAssertEqual(WidgetNote.text(row(nil, error: "HTTP 404")), "INDISP.", "plus de « … » sans fin quand les données manquent")
    }
}
