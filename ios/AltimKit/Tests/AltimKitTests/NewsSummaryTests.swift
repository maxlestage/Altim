import Foundation
import XCTest
@testable import AltimKit

/// Same cases as web/test/news-summary.test.ts, on the same real /api/news `summary` (web/test/news-summary-sample.json:
/// 28/09/2026, radar AAPL, BTC, NVDA; guard and hourly candles cached for them).
final class NewsSummaryTests: XCTestCase {
    struct Sample: Decodable { var summary: [StorySummary] }

    func list() throws -> [StorySummary] {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "news-summary-sample", withExtension: "json", subdirectory: "Fixtures"))
        return try JSONDecoder().decode(Sample.self, from: Data(contentsOf: url)).summary
    }

    func testRealSampleAtMostFiveEventsEachComplete() throws {
        let list = try list()
        XCTAssertGreaterThan(list.count, 0)
        XCTAssertLessThanOrEqual(list.count, 5)
        for s in list {
            XCTAssertNotEqual(s.impact, .unknown)
            XCTAssertEqual(s.links.first?.link, s.link)
            XCTAssertLessThanOrEqual(s.independentSources, s.sources)
            // Measured only with a move, and every move's asset is one of the story's.
            XCTAssertEqual(s.measured, !s.moves.isEmpty)
            for m in s.moves { XCTAssertTrue(s.assets.contains(m.asset)) }
            for t in s.technical { XCTAssertTrue(s.assets.contains(t.asset)) }
            XCTAssertNotNil(s.url)
        }
    }

    func testLabels() throws {
        let list = try list()
        func at(_ impact: StorySummary.Impact) -> StorySummary {
            var s = list[0]
            s.impact = impact
            return s
        }
        XCTAssertEqual(NewsSummary.heading([]).title, "Aucun événement important aujourd'hui")
        XCTAssertNil(NewsSummary.heading([]).others)
        let lows = NewsSummary.heading([at(.low), at(.low)])
        XCTAssertEqual(lows.title, "Aucun événement important aujourd'hui")
        XCTAssertEqual(lows.others, "2 sujets repris par plusieurs sources, à impact faible.")
        let one = NewsSummary.heading([at(.high), at(.low)])
        XCTAssertEqual(one.title, "1 événement important aujourd'hui")
        XCTAssertEqual(one.others, "Et 1 sujet repris par plusieurs sources, à impact faible.")
        XCTAssertEqual(NewsSummary.heading([at(.high), at(.medium)]).title, "2 événements importants aujourd'hui")
        XCTAssertEqual(StorySummary.Impact.high.label, "important")
        let nvda = try XCTUnwrap(list.first { $0.assets.contains("stock:NVDA") })
        XCTAssertEqual(NewsSummary.basisLabel(nvda), "impact mesuré")
        XCTAssertEqual(NewsSummary.consensusText(nvda), "Convergent · 2 sources · ton des titres : 2 positifs")
        var move = try XCTUnwrap(nvda.moves.first)
        move.changePct = -0.4220148770699983
        XCTAssertEqual(NewsSummary.moveText(move), "NVDA −0,4 % depuis la publication")
        let link = NewsSummary.assetLink("stock:BRK-B")
        XCTAssertEqual(link.kind, "stock")
        XCTAssertEqual(link.symbol, "BRK-B")
        XCTAssertEqual(link.asset, Asset(symbol: "BRK-B", kind: .stock, name: "BRK-B"))
        XCTAssertEqual(NewsSummary.ruleText(nvda),
                       "Règle : 2 sources indépendantes (+1) ; Concerne NVDA (+1) → 2 points, impact moyen par règle ; retenu : faible, d'après la variation mesurée (NVDA -0,4 % depuis la publication).")
    }

    func testConsensusDivergentSingleRepeated() throws {
        let base = try list()[0]
        var divergent = base
        divergent.sources = 3
        divergent.consensus = StorySummary.Consensus(agreement: "divergent", tone: "negative", negative: 2, positive: 1, neutral: 0)
        XCTAssertEqual(NewsSummary.consensusText(divergent), "Divergent · 3 sources · ton des titres : 2 négatifs, 1 positif")
        var single = base
        single.sources = 1
        single.consensus.agreement = "single"
        XCTAssertEqual(NewsSummary.consensusText(single), "Une seule source : pas de consensus mesurable")
        single.sources = 4
        XCTAssertEqual(NewsSummary.consensusText(single), "Même titre repris par 4 sources : pas de consensus mesurable")
        var partly = divergent
        partly.sources = 4
        XCTAssertEqual(NewsSummary.consensusText(partly), "Divergent · 4 sources (3 titres distincts) · ton des titres : 2 négatifs, 1 positif")
    }

    /// An older server's /api/news (no `summary`) still decodes.
    func testOlderNewsWithoutSummary() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "news", withExtension: "json", subdirectory: "Fixtures"))
        let r = try JSONDecoder().decode(NewsReport.self, from: Data(contentsOf: url))
        XCTAssertNil(r.summary)
    }
}
