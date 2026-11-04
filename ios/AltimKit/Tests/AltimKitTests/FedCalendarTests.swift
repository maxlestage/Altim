import XCTest
@testable import AltimKit

/// The Fed's rate decisions read from its published calendar (real page excerpt, 2026 and 2025 panels).
final class FedCalendarTests: XCTestCase {
    func html() throws -> String {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "fed-fomccalendars", withExtension: "html", subdirectory: "Fixtures"))
        return try String(contentsOf: url, encoding: .utf8)
    }
    static func day(_ y: Int, _ m: Int, _ d: Int) -> Double {
        var c = DateComponents(timeZone: TimeZone(identifier: "UTC"), year: y, month: m, day: d, hour: 18)
        c.calendar = Calendar(identifier: .gregorian)
        return c.date!.timeIntervalSince1970 * 1000
    }

    func testDecisionDaysOfThePublishedCalendar() throws {
        let days = FedCalendar.parse(try html())
        XCTAssertEqual(days.count, 16, "8 réunions en 2026, 8 en 2025 (le vote par écrit d'août 2025 n'est pas une réunion)")
        XCTAssertTrue(days.contains(Self.day(2026, 1, 28)), "dernier jour de la réunion des 27-28 janvier")
        XCTAssertTrue(days.contains(Self.day(2026, 12, 9)), "« 8-9* » : la marque des projections est ignorée")
        XCTAssertFalse(days.contains(Self.day(2025, 8, 22)))
        XCTAssertEqual(FedCalendar.next(days, now: Self.day(2026, 11, 2)), Self.day(2026, 12, 9))
        XCTAssertEqual(FedCalendar.next(days, now: Self.day(2026, 12, 9) - 3_600_000), Self.day(2026, 12, 9), "le jour même")
        XCTAssertNil(FedCalendar.next(days, now: Self.day(2027, 1, 1)), "au-delà du calendrier publié : rien")
        XCTAssertEqual(FedCalendar.parse(#"<h4><a id="x">2031 FOMC Meetings</a></h4><div class="fomc-meeting__month"><strong>April/May</strong></div><div class="fomc-meeting__date">30-1</div>"#),
                       [Self.day(2031, 5, 1)], "réunion à cheval sur deux mois")
    }

    /// The Fed's page changed its layout (nothing parses): the calendar read before stays, never an empty one.
    func testUnparsablePageKeepsTheLastCalendar() async {
        let backend = LocalBackend(fetcher: FakeFetcher(routes: [(FedCalendar.url, "<html>nouvelle mise en page</html>")]))
        let fresh = await backend.fedDecisions()
        XCTAssertEqual(fresh, [], "jamais lu : rien")
        await backend.setFedCache([Self.day(2026, 12, 9)], at: Date().timeIntervalSince1970 * 1000 - 13 * 3_600_000)
        let kept = await backend.fedDecisions()
        XCTAssertEqual(kept, [Self.day(2026, 12, 9)])
    }

    func testBuyAlertMentionsAnImminentDecision() {
        let a = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
        let v = TrendEngine.verdict(BotEngineTests.series(drift: 0.002), kind: .crypto)
        let now = Date(timeIntervalSince1970: Self.day(2026, 12, 7) / 1000)
        let o = LocalAlertRule.evaluate(.init(asset: a, price: 100, verdict: v, reliability: nil, zones: [], fedDecision: Self.day(2026, 12, 9), now: now))
        XCTAssertTrue(o.cautions.contains("Décision de taux de la Fed dans 2 jours (9 décembre, annoncée à 14 h heure de New York) : au labo, ce jour-là et le lendemain ont bougé en moyenne 15 à 20 % de plus que les autres jours."), o.cautions.joined(separator: "\n"))
        let far = LocalAlertRule.evaluate(.init(asset: a, price: 100, verdict: v, reliability: nil, zones: [], fedDecision: Self.day(2026, 12, 9),
                                                now: Date(timeIntervalSince1970: Self.day(2026, 11, 20) / 1000)))
        XCTAssertFalse(far.cautions.contains { $0.contains("Fed") }, "à plus de 3 jours : rien")
    }
}

extension FedCalendarTests {
    /// The macro card shows the next Fed decision; an older answer without it still reads.
    func testMacroCarriesTheNextDecision() throws {
        let old = #"{"score":20,"level":"calm","factors":[],"themes":[],"values":{}}"#
        var m = try JSONDecoder().decode(MacroInfo.self, from: Data(old.utf8))
        XCTAssertNil(m.nextFedText())
        m.nextFed = Self.day(2026, 12, 9)
        XCTAssertEqual(m.nextFedText(now: Self.day(2026, 11, 2)), "9 décembre (dans 37 jours)")
        XCTAssertEqual(m.nextFedText(now: Self.day(2026, 12, 8)), "9 décembre (demain)")
        XCTAssertEqual(m.nextFedText(now: Self.day(2026, 12, 9) - 3_600_000), "9 décembre (aujourd'hui)")
        XCTAssertNil(m.nextFedText(now: Self.day(2026, 12, 11)), "passée : rien")
        // 01:00 UTC on 9 December is still 8 December in New York (the day before the decision), whatever the iPhone's zone.
        XCTAssertEqual(m.nextFedText(now: Self.day(2026, 12, 9) - 17 * 3_600_000), "9 décembre (demain)")
        // 02:00 UTC on 10 December is still the evening of 9 December in New York: still « aujourd'hui ».
        XCTAssertEqual(m.nextFedText(now: Self.day(2026, 12, 9) + 8 * 3_600_000), "9 décembre (aujourd'hui)")
        XCTAssertEqual(FedCalendar.next([Self.day(2026, 12, 9)], now: Self.day(2026, 12, 9) + 8 * 3_600_000), Self.day(2026, 12, 9))
        XCTAssertNil(FedCalendar.next([Self.day(2026, 12, 9)], now: Self.day(2026, 12, 9) + 12 * 3_600_000), "lendemain à New York : passée")
        let back = try JSONDecoder().decode(MacroInfo.self, from: JSONEncoder().encode(m))
        XCTAssertEqual(back.nextFed, m.nextFed)
    }
}

/// The Fibonacci card no longer tells to wait for a pull-back (waiting missed most rises in the lab).
final class FibTextTests: XCTestCase {
    func testAboveTheZoneDoesNotSayWait() {
        Money.setDisplay(.usd, nil)
        let s = FibEngine.Swing(up: true, low: 100, high: 200, lowIndex: 0, highIndex: 10)
        let st = FibEngine.state(s, 190)
        XCTAssertEqual(st.status, "above")
        XCTAssertFalse(st.text.contains("attendre un repli vers"))
        XCTAssertTrue(st.text.hasSuffix("la zone est un repère, le verdict décide."), st.text)
    }
}
