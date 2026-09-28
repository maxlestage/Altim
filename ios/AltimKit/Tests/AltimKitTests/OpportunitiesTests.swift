import Foundation
import XCTest
@testable import AltimKit

/// Same cases as web/test/opportunities.test.ts, plus the decoding of the server's contracts (flattened metrics of an
/// item, derivatives block of an anomaly report) and the 202 "pending" answer.
final class OpportunitiesTests: XCTestCase {
    func item(_ symbol: String, marketCap: Double? = 1e11, rank: Int? = nil, hits: [OppHit] = [OppHit(category: .breakout, reason: "r", strength: 2)]) -> OppItem {
        OppItem(symbol: symbol, name: symbol, sector: "Technologie", marketCap: marketCap, rank: rank, price: 100, time: 0, change1d: 1, rsi14: 50,
                volumeRatio: 1, volatility: 2, liquidity: 5e8, distanceAtr: 0, hits: hits)
    }

    func testNumericFiltersUnknownValueFailsASetFilter() {
        let a = item("A")
        let d = OppFilters.defaults
        XCTAssertTrue(Opportunities.passes(a, d))
        XCTAssertFalse(Opportunities.passes(a, OppFilters(minCap: 2e11)))
        XCTAssertFalse(Opportunities.passes(item("B", marketCap: nil), OppFilters(minCap: 1e10)))
        XCTAssertTrue(Opportunities.passes(item("C", rank: 12), OppFilters(maxRank: 20)))
        XCTAssertFalse(Opportunities.passes(item("C", rank: 45), OppFilters(maxRank: 20)))
        XCTAssertFalse(Opportunities.passes(a, OppFilters(maxVolatility: 1.5)))
        XCTAssertFalse(Opportunities.passes(a, OppFilters(minLiquidity: 1e9)))
    }

    func testCategoriesOnlyChosenReasonsMostReasonsFirst() {
        let items = [
            item("ONE"),
            item("TWO", hits: [OppHit(category: .volume, reason: "v", strength: 5), OppHit(category: .breakout, reason: "b", strength: 3)]),
            item("OVS", hits: [OppHit(category: .oversold, reason: "o", strength: 9)]),
        ]
        XCTAssertEqual(Opportunities.filter(items, .defaults).map(\.symbol), ["TWO", "OVS", "ONE"])
        let only = Opportunities.filter(items, OppFilters(categories: [.breakout]))
        XCTAssertEqual(only.map(\.symbol), ["TWO", "ONE"])
        XCTAssertEqual(only[0].hits.map(\.category), [.breakout])
        XCTAssertTrue(Opportunities.filter(items, OppFilters(categories: [])).isEmpty)
        XCTAssertEqual(Opportunities.countByCategory(items, .defaults), [.setup: 0, .reversal: 0, .breakout: 2, .volume: 1, .oversold: 1, .fundamentals: 0])
        XCTAssertEqual(Opportunities.countByCategory(items, OppFilters(maxVolatility: 1))[.breakout], 0)
    }

    func testLiquidationsAndAmounts() throws {
        let json = #"{"longUsd":14389008,"shortUsd":5835591,"longCount":1486,"shortCount":764,"largest":null,"from":0,"to":0,"hours":24,"complete":true,"scope":""}"#
        var l = try JSONDecoder().decode(LiquidationSummary.self, from: Data(json.utf8))
        XCTAssertEqual(Int((l.longShare! + 0.5).rounded(.down)), 71)
        l.longUsd = 0
        l.shortUsd = 0
        XCTAssertNil(l.longShare)
        XCTAssertEqual(Opportunities.compactUsd(14_389_008), "14,4 M$")
        XCTAssertEqual(Opportunities.compactUsd(3_060_890_723), "3,1 Md$")
        XCTAssertEqual(Opportunities.compactUsd(843_414), "843 k$")
        XCTAssertEqual(Opportunities.compactUsd(420), "420 $")
    }

    func testSavedFiltersValidated() throws {
        let saved = OppSaved(market: .crypto, filters: OppFilters(categories: [.volume, .setup], maxRank: 50))
        XCTAssertEqual(Opportunities.parseSaved(try JSONEncoder().encode(saved)), saved)
        XCTAssertEqual(Opportunities.parseSaved(Data("{oops".utf8)), OppSaved())
        XCTAssertEqual(Opportunities.parseSaved(nil), OppSaved())
        let odd = #"{"market":"stock","filters":{"categories":["volume","whales"]}}"#
        XCTAssertEqual(Opportunities.parseSaved(Data(odd.utf8)).filters.categories, [.volume])
    }

    /// The server's item flattens its metrics (serde flatten), camelCase; unknown categories never break decoding.
    func testContractsDecode() throws {
        let report = #"""
        {"kind":"stock","asOf":1790628721151,"scanned":150,"universe":"150 actions","topN":150,
         "categories":[{"id":"breakout","label":"Cassures","rule":"Clôture au-dessus du plus haut de 20 séances","analyzed":150,"note":null,"error":null}],
         "items":[{"symbol":"NVDA","name":"NVIDIA","sector":"Technologie","marketCap":4.1e12,"rank":null,"price":181.2,"time":1790553600000,
           "change1d":2.1,"rsi14":61.4,"volumeRatio":1.8,"volatility":2.3,"liquidity":3.2e10,"distanceAtr":0.4,
           "hits":[{"category":"breakout","reason":"Clôture au-dessus du plus haut de 20 séances","strength":2.1},{"category":"whales","reason":"x","strength":1}]}],
         "notCovered":[{"label":"Baleines","reason":"aucune source gratuite vérifiable"}],"source":"Nasdaq"}
        """#
        let r = try JSONDecoder().decode(OpportunityReport.self, from: Data(report.utf8))
        XCTAssertEqual(r.items[0].hits.map(\.category), [.breakout, .unknown])
        XCTAssertEqual(Opportunities.filter(r.items, .defaults)[0].hits.count, 1)
        XCTAssertEqual(Opportunities.metrics(r.items[0]), ["RSI 14 : 61", "volatilité 2,3 %/j", "échangé 32 Md$/j", "capitalisation 4\u{202F}100 Md$"])
        let anomalies = #"""
        {"symbol":"BTC","kind":"crypto","asOf":1,"price":80000,"session":1790553600000,
         "anomalies":[{"code":"volume","severity":"high","triggered":true,"title":"Volume anormal","value":3.2,"threshold":2,"unit":"×","measured":"3,2 × la moyenne","meaning":"m","source":"s"}],
         "normal":[],"derivatives":{"source":"OKX","liquidations":null,"openInterest":{"usd":3.1e10,"time":1,"change24h":2.5,"change7d":null},
           "funding":{"rate":0.01,"p5":-0.002,"p95":0.012,"samples":100,"periodHours":8,"time":1},"longShort":null,"errors":[],"notCovered":[]},
         "errors":[],"source":"Bougies"}
        """#
        let a = try JSONDecoder().decode(AnomalyReport.self, from: Data(anomalies.utf8))
        XCTAssertEqual(a.anomalies.first?.severity, .high)
        XCTAssertEqual(a.derivatives?.funding?.periodHours, 8)
        XCTAssertEqual(Opportunities.signed(2.5), "+2,5 %")
        XCTAssertEqual(Opportunities.signed(-0.002, 4), "−0,002 %")
        XCTAssertEqual(Opportunities.pendingText(.crypto), "Analyse des 120 cryptos en cours (environ 30 secondes la première fois)…")
    }
}
