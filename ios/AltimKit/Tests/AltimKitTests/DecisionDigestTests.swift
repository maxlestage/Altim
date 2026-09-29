import Foundation
import XCTest
@testable import AltimKit

/// The Radar follows the full decision; the 4 h technical signal is only a direction (web Radar.tsx, DecisionCard.tsx, ui.tsx).
final class DecisionDigestTests: XCTestCase {
    func btc() throws -> Decision {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "decision-btc", withExtension: "json", subdirectory: "Fixtures"))
        return try JSONDecoder().decode(Decision.self, from: Data(contentsOf: url))
    }

    let hour = 3_600_000.0

    func digest(_ verdict: Decision.Verdict, _ rating: Decision.Rating?, confidence: Double = 50, at: Double = 0, personal: Bool = false) -> DecisionDigest {
        DecisionDigest(verdict: verdict, rating: rating, label: verdict.label, level: .waiting, confidence: confidence, personal: personal, at: at)
    }

    func testTechnicalText() {
        XCTAssertEqual(Action.strongBuy.technicalText, "nettement haussier")
        XCTAssertEqual(Action.buy.technicalText, "haussier")
        XCTAssertEqual(Action.hold.technicalText, "neutre")
        XCTAssertEqual(Action.sell.technicalText, "baissier")
        XCTAssertEqual(Action.strongSell.technicalText, "nettement baissier")
        XCTAssertEqual(Action.buy.directionTone, .good)
        XCTAssertEqual(Action.strongSell.directionTone, .bad)
        XCTAssertEqual(Action.hold.directionTone, .neutral)
    }

    func testRatingRankAndTone() {
        let order: [Decision.Rating?] = [.strongBuy, .buy, .hold, .reduce, .sell, .strongSell, nil]
        XCTAssertEqual(order.map(DecisionDigests.ratingRank), [0, 1, 2, 3, 4, 5, 9])
        XCTAssertEqual(DecisionDigests.ratingRank(.unknown), 9)
        XCTAssertEqual(DecisionDigests.ratingTone(.buy, verdict: .wait), .good)
        XCTAssertEqual(DecisionDigests.ratingTone(.reduce, verdict: .buy), .bad)
        XCTAssertEqual(DecisionDigests.ratingTone(.hold, verdict: .buyZone), .neutral)
        XCTAssertEqual(DecisionDigests.ratingTone(nil, verdict: .buyZone), .good)
        XCTAssertEqual(DecisionDigests.ratingTone(nil, verdict: .trim), .bad)
        XCTAssertEqual(DecisionDigests.ratingTone(nil, verdict: .wait), .neutral)
        XCTAssertEqual(DecisionDigests.ratingTone(.unknown, verdict: .sell), .bad)
    }

    func testBadgeLabel() {
        XCTAssertEqual(digest(.wait, .strongBuy).badgeLabel, "ACHAT FORT")
        XCTAssertEqual(digest(.buyZone, nil).badgeLabel, "ZONE D'ACHAT")
        XCTAssertEqual(digest(.wait, .unknown).badgeLabel, "ATTENDRE")
    }

    func testRecordAndFreshness() throws {
        let d = try btc()
        let now = d.asOf + 1000
        let all = DecisionDigests.record([:], d, personal: false, now: now)
        let asset = Asset(symbol: "BTC", kind: .crypto, name: "Bitcoin")
        let got = try XCTUnwrap(DecisionDigests.fresh(all, asset, now: now))
        XCTAssertEqual(got.verdict, .wait)
        XCTAssertEqual(got.badgeLabel, "ATTENDRE")
        XCTAssertEqual(got.confidence, 67)
        XCTAssertEqual(got.at, d.asOf)
        XCTAssertEqual(got.tone, .neutral)
        XCTAssertFalse(got.isBuy)
        // More than 12 h later: "Décision…" rather than a stale verdict.
        XCTAssertNil(DecisionDigests.fresh(all, asset, now: d.asOf + 12 * hour + 1))
        XCTAssertNotNil(DecisionDigests.fresh(all, asset, now: d.asOf + 12 * hour))
    }

    func testPersonalKeptWhileRecent() throws {
        var personal = try btc()
        personal.verdict = .buyZone
        let t0 = personal.asOf
        let all = DecisionDigests.record([:], personal, personal: true, now: t0)
        var info = try btc()
        info.asOf = t0 + hour
        // The Radar's informational decision does not replace the personal one seen on the page.
        let kept = DecisionDigests.record(all, info, personal: false, now: t0 + hour)
        XCTAssertEqual(kept["crypto:BTC"]?.verdict, .buyZone)
        XCTAssertEqual(kept["crypto:BTC"]?.personal, true)
        // Once the personal one is stale, it is replaced.
        info.asOf = t0 + 13 * hour
        let replaced = DecisionDigests.record(all, info, personal: false, now: t0 + 13 * hour)
        XCTAssertEqual(replaced["crypto:BTC"]?.verdict, .wait)
        // An older answer (offline cache) never replaces a newer one.
        var old = try btc()
        old.asOf = t0 - hour
        XCTAssertEqual(DecisionDigests.record(replaced, old, personal: true, now: t0 + 14 * hour), replaced)
    }

    func testSortsAndOpportunities() {
        let a = Asset(symbol: "A", kind: .crypto, name: "A")
        let b = Asset(symbol: "B", kind: .stock, name: "B")
        let c = Asset(symbol: "C", kind: .crypto, name: "C")
        let e = Asset(symbol: "E", kind: .crypto, name: "E")
        let now = 100 * hour
        let all: [String: DecisionDigest] = [
            a.id: digest(.wait, .hold, confidence: 80, at: now),
            b.id: digest(.buyZone, .buy, confidence: 40, at: now),
            c.id: digest(.buy, .buy, confidence: 70, at: now),
            e.id: digest(.buy, .strongBuy, confidence: 90, at: now - 13 * hour),
        ]
        let d = Asset(symbol: "D", kind: .crypto, name: "D")
        // Stale (E) and unknown (D) last, in the user's order.
        XCTAssertEqual(DecisionDigests.sortByDecision([d, a, b, e, c], all, now: now).map(\.symbol), ["C", "B", "A", "D", "E"])
        let changes = ["A": 1.0, "B": -3.0, "C": 2.0]
        XCTAssertEqual(DecisionDigests.sortByChange([d, a, b, c], change: { changes[$0.symbol] }).map(\.symbol), ["B", "C", "A", "D"])
        let opp = DecisionDigests.opportunities([a, b, c, d, e], all, now: now)
        XCTAssertEqual(opp.map(\.asset.symbol), ["C", "B"])
        XCTAssertEqual(opp.first?.decision.badgeLabel, "ACHAT")
        XCTAssertEqual(DecisionDigests.opportunities([a, b, c], all, now: now, limit: 1).map(\.asset.symbol), ["C"])
    }

    func testCodableRoundTrip() throws {
        let x = digest(.buyZone, .buy, confidence: 61, at: 5)
        XCTAssertEqual(try JSONDecoder().decode(DecisionDigest.self, from: JSONEncoder().encode(x)), x)
    }
}
