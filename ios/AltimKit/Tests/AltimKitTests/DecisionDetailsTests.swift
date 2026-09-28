import Foundation
import XCTest
@testable import AltimKit

/// Newer fields of GET /api/decision (fundamentals P/S, P/B, ROIC, valuation history, peers, stablecoins, developer
/// activity; track spread, expectancy, regimes): decoded when present, absent from older answers, and written like
/// the web (texts computed by web/src/webapp/decision.ts on the same figures).
final class DecisionDetailsTests: XCTestCase {
    func decision(_ name: String) throws -> Decision {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try JSONDecoder().decode(Decision.self, from: Data(contentsOf: url))
    }

    let n = "\u{202F}"

    func testOlderAnswersStillDecode() throws {
        let d = try decision("decision-aapl")
        guard case let .stock(f)? = d.fundamentals else { return XCTFail("stock fundamentals expected") }
        XCTAssertNil(f.ps)
        XCTAssertNil(f.guidance)
        XCTAssertNil(f.valuationHistory)
        XCTAssertFalse(try XCTUnwrap(d.track).hasDetails)
        let b = try decision("decision-btc")
        guard case let .crypto(c)? = b.fundamentals else { return XCTFail("crypto fundamentals expected") }
        XCTAssertFalse(c.knowsDevActivity)
        XCTAssertNil(c.stablecoins)
    }

    func testStockFundamentals() throws {
        let d = try decision("decision-aapl-full")
        guard case let .stock(f)? = d.fundamentals else { return XCTFail("stock fundamentals expected") }
        XCTAssertEqual(f.ps, 9.87)
        XCTAssertEqual(f.pb, 55.2)
        XCTAssertEqual(f.sector?.sic, "3571")
        XCTAssertEqual(f.peers?.peers.count, 2)
        XCTAssertNil(f.valuationHistory?.ps)
        XCTAssertEqual(DecisionText.num(f.ps, digits: 1), "9,9")
        XCTAssertEqual(DecisionText.roic(f), "48,3\(n)% (impôt 16,1\(n)%, taux effectif)")
        var statutory = f
        statutory.roicTaxStatutory = true
        XCTAssertEqual(DecisionText.roic(statutory), "48,3\(n)% (impôt 16,1\(n)% : taux légal américain, taux effectif non calculable)")
        let rows = DecisionText.historyRows("PER", f.valuationHistory?.per)
        XCTAssertEqual(rows.map(\.0), ["PER sur la période", "Centile du PER"])
        XCTAssertEqual(rows[0].1, "39,2 aujourd'hui · médiane 31,4 · de 23,1 à 42,7")
        XCTAssertEqual(rows[1].1, "plus haut que 91 % des 1\(n)180 jours (1 octobre 2021 – 28 septembre 2026)")
        XCTAssertTrue(DecisionText.historyRows("P/S", nil).isEmpty)
        XCTAssertEqual(DecisionText.peer(try XCTUnwrap(f.peers?.peers[1])),
                       "HP Inc. (HPQ) : PER 10,4, P/S 0,5, marge opérationnelle —, chiffre d'affaires −1,3\(n)% sur un an")
        XCTAssertEqual(DecisionText.nyDate(try XCTUnwrap(f.periodEnd)), "27 juin 2026")
        XCTAssertEqual(DecisionText.nyDate(try XCTUnwrap(f.filedAt)), "1 août 2026")
    }

    func testCryptoFundamentals() throws {
        let d = try decision("decision-btc-full")
        guard case let .crypto(c)? = d.fundamentals else { return XCTFail("crypto fundamentals expected") }
        XCTAssertTrue(c.knowsDevActivity)
        XCTAssertNil(c.devActivity, "unavailable: told as such, not hidden")
        XCTAssertNil(c.chainStablecoins)
        let rows = DecisionText.stableRows(c.stablecoins, label: "tous réseaux")
        XCTAssertEqual(rows.map(\.0), ["Stablecoins (tous réseaux)", "… sur 7 jours", "… sur 30 jours"])
        XCTAssertEqual(rows.map(\.1), ["310\(n)Md$ au 28 septembre 2026", "+2,8\(n)Md$ (+0,91\(n)%)", "−271\(n)M$ (−0,09\(n)%)"])
        XCTAssertEqual(DecisionText.usdCompact(12_345), "12,3\(n)k$")
        XCTAssertEqual(DecisionText.usdCompact(0.5), "0,5\(n)$")
        XCTAssertEqual(DecisionText.count(19_930_000), "19,93 millions")
        XCTAssertEqual(DecisionText.count(1234), "1\(n)234")
        let dev = try JSONDecoder().decode(Decision.DevActivity.self, from: Data(#"{"repo":"bitcoin/bitcoin","commits4w":120,"pullRequestsMerged":null,"contributors":900,"stars":80000,"additions4w":5000,"deletions4w":3000,"smartContractPlatform":false,"source":"GitHub"}"#.utf8))
        XCTAssertEqual(dev.commits4w, 120)
        XCTAssertNil(dev.pullRequestsMerged)
    }

    func testTrackDetails() throws {
        let t = try XCTUnwrap(try decision("decision-aapl-full").track)
        XCTAssertTrue(t.hasDetails)
        XCTAssertEqual(t.regimes?.count, 4)
        XCTAssertEqual(t.regimes?.first?.label, "Marché haussier")
        XCTAssertEqual(t.regimes?[2].winRate, nil)
        XCTAssertEqual(t.spreadMeasured, false)
        XCTAssertEqual(t.biasNotes?.first, "Paramètres fixes, ceux du signal en direct : aucune optimisation sur la période testée.")
        // 61,2 % × (1 − 30 %).
        XCTAssertEqual(t.afterTaxReturn, 42.84, accuracy: 1e-9)
        XCTAssertEqual(DecisionText.pct(t.afterTaxReturn, digits: 1, sign: true), "+42,8\(n)%")
        var loss = t
        loss.totalReturn = -5
        XCTAssertEqual(loss.afterTaxReturn, -5)
        // Newer answer without trades: expectancy null, regimes empty, still "newer".
        let b = try XCTUnwrap(try decision("decision-btc-full").track)
        XCTAssertTrue(b.hasDetails)
        XCTAssertNil(b.expectancy)
        XCTAssertEqual(b.spreadMeasured, true)
    }
}
