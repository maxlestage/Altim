import Foundation
import XCTest
@testable import AltimKit

final class LocalMacroTests: XCTestCase {
    func series(_ n: Int, _ f: (Int) -> Double) -> [MacroEngine.Point] {
        (0..<n).map { MacroEngine.Point(time: Double($0) * 86_400_000, close: f($0)) }
    }

    func testCalmThenStressedMarket() {
        let calm = MacroEngine.report(["vix": series(300) { 15 + sin(Double($0)) }, "spx": series(300) { 5000 + Double($0) }], news: [], now: 0)!
        XCTAssertEqual(calm.level, "calm")
        XCTAssertEqual(calm.values["vix"]?.value ?? 0, 15 + sin(299), accuracy: 1e-9)
        // VIX at 35 after a jump, S&P 500 down 9 % from its month's high.
        let vix = series(300) { $0 < 295 ? 15 + sin(Double($0)) : 35 }
        let spx = series(300) { $0 < 290 ? 5000 : 4550 }
        let stressed = MacroEngine.report(["vix": vix, "spx": spx], news: [], now: 0)!
        XCTAssertTrue(stressed.factors.map(\.code).contains("vixHigh"))
        XCTAssertTrue(stressed.factors.map(\.code).contains("vixJump"))
        XCTAssertTrue(stressed.factors.map(\.code).contains("spxDrawdown"))
        XCTAssertEqual(stressed.level, "high")
        XCTAssertNil(MacroEngine.report([:], news: [], now: 0))
    }

    func testEscalationHeadlinesButNotQuestions() {
        let now = 1_800_000_000_000.0
        let news = [(title: "Country X declares war on Country Y", time: now - 3_600_000),
                    (title: "Is a bank run coming?", time: now - 3_600_000),
                    (title: "Fed holds interest rates", time: now - 7_200_000)]
        let h = MacroEngine.headlines(news, now: now)
        XCTAssertEqual(h.factors.first?.points, 10)
        XCTAssertTrue(h.factors.first!.text.contains("déclaration de guerre"))
        XCTAssertFalse(h.factors.first!.text.contains("panique bancaire"))
        XCTAssertEqual(Set(h.themes.map(\.theme)), ["geopolitics", "monetary", "stress"])
    }

    func testMacroRouteWithoutYahooFails() async {
        let r = await LocalBackend(fetcher: FakeFetcher(routes: [])).answer(path: "/api/macro", query: [:])!
        XCTAssertEqual(r.1, 502)
    }

    func testLiveMacro() async throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["ALTIM_LIVE"] == "1", "réseau réel non demandé")
        let r = await LocalBackend().answer(path: "/api/macro", query: [:])!
        print(String(decoding: r.0, as: UTF8.self).prefix(900))
    }
}
