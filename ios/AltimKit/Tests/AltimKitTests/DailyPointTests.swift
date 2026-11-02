import XCTest
@testable import AltimKit

/// « Point du jour » on the device: markets for the filter, the watchlist's verdicts, the next Fed decision.
final class DailyPointTests: XCTestCase {
    func row(_ s: String, _ a: Action?) -> RadarRow {
        RadarRow(symbol: s, kind: .crypto, name: s, price: 1, change: nil, priceSources: nil,
                 signal: a.map { SignalSummary(action: $0, score: 0, confidence: 50) }, reliability: nil, agreeing: nil, sources: nil,
                 sparkline: nil, error: nil)
    }

    func testLines() throws {
        var m = try JSONDecoder().decode(MacroInfo.self, from: Data(#"{"score":20,"level":"calm","factors":[],"themes":[],"values":{}}"#.utf8))
        m.marketUp = ["crypto": false, "stock": true]
        m.nextFed = FedCalendarTests.day(2026, 12, 9)
        let lines = DailyPoint.lines(macro: m, rows: [row("A", .strongBuy), row("B", .buy), row("C", .hold), row("D", .sell), row("E", nil)],
                                     now: FedCalendarTests.day(2026, 11, 2))
        XCTAssertEqual(lines, [
            "Marché crypto (Bitcoin) hors tendance haussière : le filtre de marché suspend les achats et conseille de sortir.",
            "Marché actions (S&P 500) en tendance haussière : le filtre de marché laisse acheter.",
            "Vos 4 actifs suivis : 2 à l'achat (dont 1 sur repli), 1 en attente, 1 à l'écart ou à la sortie.",
            "Prochaine décision de taux de la Fed : 9 décembre (dans 37 jours).",
        ])
        XCTAssertEqual(DailyPoint.lines(macro: nil, rows: []), [], "rien de connu : pas de carte")
        let soon = EarningsEvent(symbol: "AAPL", name: "Apple", date: "2026-11-05", time: FedCalendarTests.day(2026, 11, 5) - 4.5 * 3_600_000,
                                 moment: "après la clôture", epsForecast: nil, lastYearEPS: nil)
        let far = EarningsEvent(symbol: "MSFT", name: "Microsoft", date: "2026-11-30", time: FedCalendarTests.day(2026, 11, 30), moment: nil, epsForecast: nil, lastYearEPS: nil)
        XCTAssertEqual(DailyPoint.lines(macro: nil, rows: [], earnings: [far, soon], now: FedCalendarTests.day(2026, 11, 2)),
                       ["Résultats trimestriels dans les 7 jours : AAPL (jeudi 5 novembre, après la clôture)."])
        let back = try JSONDecoder().decode(MacroInfo.self, from: JSONEncoder().encode(m))
        XCTAssertEqual(back.marketUp?["crypto"], false)
    }
}
