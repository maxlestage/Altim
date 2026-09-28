import Foundation
import XCTest
@testable import AltimKit

/// Same cases as web/test/journal.test.ts (same decision, same candles, same expected values and texts).
final class TradeJournalTests: XCTestCase {
    let day = TradeJournal.day
    /// Date.UTC(2026, 5, 1).
    let t0 = 1_780_272_000_000.0
    /// Entry at 10:00 UTC on day 0.
    var at: Double { t0 + 10 * 3_600_000 }

    /// A decision with only what the journal reads; `over` replaces fields, `removing` drops them (older servers).
    func decision(_ over: [String: Any] = [:], removing: [String] = []) throws -> Decision {
        var o: [String: Any] = [
            "symbol": "BTC", "kind": "crypto", "name": "Bitcoin", "asOf": at - 3_600_000, "price": 100, "mode": "informational",
            "verdict": "buy", "label": "Acheter", "level": "strong", "levelLabel": "Signal fort", "confidence": 72, "confidenceText": "", "headline": "Rebond sur support",
            "families": [Any](), "vetoes": [["code": "x", "label": "Volatilité extrême", "active": false, "verifiable": true, "detail": ""]], "blocked": false,
            "setup": ["name": "Rebond", "steps": [["label": "Support", "state": "ok", "detail": ""], ["label": "Volume", "state": "no", "detail": ""]], "met": 1, "total": 2],
            "plan": plan(),
            "whyWait": [Any](), "toBuy": [Any](), "toSell": [Any](), "scenarios": [Any](), "pros": ["a", "b", "c", "d"], "cons": ["x"],
            "whyNot": ["risks": [Any](), "uncertainty": "low", "invalidation": [Any]()],
            "liquidity": ["relativeVolume": 1.4, "source": ""], "sources": [Any](), "disclaimer": "d",
            "rating": "buy", "ratingLabel": "Achat", "score": ["value": 64, "label": "", "factors": [Any](), "missing": [Any](), "custom": false, "text": ""],
            "degraded": ["active": false, "headline": "", "reasons": [Any]()], "marketRegime": regime("riskOn", "Risk-on"),
            "horizon": ["kind": "swing", "label": "Swing", "atrDistance": 1, "detail": ""], "events": [Any](),
        ]
        for (k, v) in over { o[k] = v }
        for k in removing { o.removeValue(forKey: k) }
        return try JSONDecoder().decode(Decision.self, from: JSONSerialization.data(withJSONObject: o))
    }

    func plan(riskReward: Double = 2) -> [String: Any] {
        ["zoneFrom": 98, "zoneTo": 102, "entry": 100, "stop": 95, "target1": 110, "target2": 120, "riskPct": 5, "reward1Pct": 10, "reward2Pct": 20,
         "riskReward": riskReward, "minRiskReward": 2, "acceptable": true, "horizon": "swing"]
    }

    func regime(_ kind: String, _ label: String) -> [String: Any] { ["kind": kind, "label": label, "reasons": [Any]()] }

    func entry(id: String = "e1", source: TradeJournalSource = .paper, side: TradeSide = .buy, price: Double = 100, stop: Double? = 95,
               targets: [Double?] = [110], note: String = "", decision d: Decision?? = nil) throws -> TradeJournalEntry {
        let dec: Decision? = try d ?? decision()
        return TradeJournal.create(.init(id: id, now: at, source: source, side: side, symbol: "BTC", kind: .crypto, name: "Bitcoin", price: price,
                                         stop: stop, targets: targets, note: note, decision: dec))
    }

    /// Daily candles from day 0 (the entry day) with [low, high, close] per day.
    func days(_ rows: [(Double, Double, Double)]) -> [Candle] {
        rows.enumerated().map { i, r in
            Candle(time: t0 + Double(i) * day, open: i > 0 ? rows[i - 1].2 : 100, high: r.1, low: r.0, close: r.2, volume: 10)
        }
    }

    // MARK: Writing entries

    func testSnapshotKeepsReasonsVetoesSetupPlan() throws {
        let e = try entry(note: "  rebond  ")
        let d = try XCTUnwrap(e.decision)
        XCTAssertEqual(d.pros, ["a", "b", "c"])
        XCTAssertEqual(d.vetoes, [])
        XCTAssertEqual(d.setup.steps, [.init(label: "Support", state: "ok"), .init(label: "Volume", state: "no")])
        XCTAssertEqual(d.score, 64)
        XCTAssertEqual(e.market.regime, "riskOn")
        XCTAssertEqual(e.market.relativeVolume, 1.4)
        XCTAssertEqual(e.market.events, 0)
        XCTAssertNil(e.market.macroScore)
        XCTAssertNil(e.market.atrPct)
        XCTAssertEqual(e.signal, "Achat (Signal fort) · configuration « Rebond » 1/2 · horizon Swing")
        XCTAssertEqual(e.note, "rebond")
    }

    func testStopAboveAndTargetsBelowDroppedSaleKeepsNeither() throws {
        let a = try entry(stop: 105, targets: [90, 130, nil])
        XCTAssertNil(a.stop)
        XCTAssertEqual(a.targets, [130])
        let s = try entry(side: .sell)
        XCTAssertNil(s.stop)
        XCTAssertEqual(s.targets, [])
    }

    func testOlderDecisionsStillGiveAnEntry() throws {
        let d = try decision(removing: ["rating", "ratingLabel", "score", "degraded", "marketRegime", "horizon", "events"])
        let e = TradeJournal.create(.init(id: "x", now: at, source: .real, side: .buy, symbol: "BTC", kind: .crypto, name: "Bitcoin", price: 100, decision: d))
        XCTAssertNil(e.decision?.rating)
        XCTAssertNil(e.decision?.score)
        XCTAssertEqual(e.decision?.degraded, false)
        XCTAssertEqual(e.decision?.horizon, "swing")
        XCTAssertNil(e.market.regime)
        XCTAssertNil(e.market.events)
    }

    func testStateHelpersAddPatchRemoveValidate() throws {
        var s = TradeJournal.add(TradeJournalState(), try entry())
        s = TradeJournal.add(s, try entry())
        XCTAssertEqual(s.entries.count, 1)
        s = TradeJournal.patch(s, id: "e1", note: "x", market: JournalMarket(macroScore: 31, macroLevel: "tense"))
        XCTAssertEqual(s.entries[0].market.macroScore, 31)
        XCTAssertEqual(s.entries[0].market.macroLevel, "tense")
        XCTAssertEqual(s.entries[0].market.regime, "riskOn")
        XCTAssertEqual(s.entries[0].note, "x")
        let saved = TradeJournal.parseSaved(try JSONEncoder().encode(s))
        XCTAssertNil(saved.error)
        XCTAssertEqual(saved.state, s)
        XCTAssertNotNil(TradeJournal.parseSaved(Data(#"{"version":1,"entries":[{"id":1}]}"#.utf8)).error)
        XCTAssertTrue(TradeJournal.remove(s, id: "e1").entries.isEmpty)
    }

    func testAtrAndRelativeVolumeUseOnlyDaysClosedBeforeEntry() {
        var c: [Candle] = (0..<30).map { i in
            Candle(time: t0 - Double(30 - i) * day, open: 100, high: 102, low: 98, close: 100, volume: i == 29 ? 30 : 10)
        }
        // The entry day's own candle (huge range, huge volume) must not count.
        c.append(Candle(time: t0, open: 100, high: 200, low: 50, close: 150, volume: 1000))
        let m = TradeJournal.market(from: c, at: at)
        XCTAssertEqual(m.atrPct ?? 0, 4, accuracy: 1e-5)
        XCTAssertEqual(m.relativeVolume ?? 0, 3, accuracy: 1e-5)
    }

    func testHoldingEditBecomesPurchaseOrSale() {
        XCTAssertEqual(TradeJournal.holdingChange(before: (1, 100), after: (2, 110), livePrice: 130), .init(side: .buy, quantity: 1, price: 120, implied: true))
        XCTAssertEqual(TradeJournal.holdingChange(before: (1, 100), after: (3, 100), livePrice: 130), .init(side: .buy, quantity: 2, price: 130, implied: false))
        XCTAssertEqual(TradeJournal.holdingChange(before: (2, 100), after: (0.5, 100), livePrice: 90), .init(side: .sell, quantity: 1.5, price: 90, implied: false))
        XCTAssertNil(TradeJournal.holdingChange(before: (2, 100), after: (1, 100), livePrice: nil))
        XCTAssertNil(TradeJournal.holdingChange(before: (2, 100), after: (2, 90), livePrice: 90))
        // Without an average cost (optional on the iPhone), the live price.
        XCTAssertEqual(TradeJournal.holdingChange(before: (1, nil), after: (2, 110), livePrice: 130), .init(side: .buy, quantity: 1, price: 130, implied: false))
    }

    // MARK: Review

    func testTargetReachedOnDay6() throws {
        // Day 0 (entry day) touches the stop before the purchase: ignored.
        let c = days([(90, 101, 100), (99, 103, 102), (98, 104, 103), (99, 105, 104), (100, 106, 105), (101, 107, 106), (104, 111, 109),
                      (105, 108, 107), (104, 108, 106), (104, 107, 106), (103, 106, 105)])
        let r = TradeJournal.review(try entry(), candles: c, now: at + 40 * day)
        let h10 = try XCTUnwrap(r.horizons.first { $0.days == 10 })
        XCTAssertEqual(h10.status, .ready)
        XCTAssertEqual(h10.first, .target1)
        XCTAssertEqual(h10.firstDays, 6)
        XCTAssertEqual(h10.resultR, 2)
        XCTAssertEqual(h10.mfePct ?? 0, 11, accuracy: 1e-6)
        XCTAssertEqual(h10.maeR ?? 0, -0.4, accuracy: 1e-6)
        XCTAssertEqual(r.planR, 2)
        XCTAssertTrue(r.worked.contains("L'objectif 1 a été atteint en 6 jours (+2 R)."))
        XCTAssertEqual(r.horizons.first { $0.days == 30 }?.status, .ready)
        XCTAssertEqual(TradeJournal.horizonText(h10, buy: true, pendingDate: { _ in "" }),
                       "10 j : +5 % au dernier cours · plus haut +11 % (+2,2 R) · plus bas −2 % (−0,4 R) · objectif 1 atteint en 6 j · résultat selon le plan +2 R")
    }

    func testStopTouchedWhileDegraded() throws {
        let e = try entry(decision: .some(try decision(["degraded": ["active": true, "headline": "Données incomplètes", "reasons": [Any]()]])))
        let c = days([(99, 101, 100), (99, 102, 101), (94, 112, 96), (95, 97, 96)])
        let r = TradeJournal.review(e, candles: c, now: at + 5 * day)
        let h3 = r.horizons[0]
        XCTAssertEqual(h3.first, .stop)
        XCTAssertEqual(h3.firstDays, 2)
        XCTAssertEqual(h3.resultR, -1)
        XCTAssertEqual(r.failed.first, "Le stop a été touché en 2 jours (−1 R) alors que le signal était dégradé à l'entrée.")
        XCTAssertEqual(r.coherent, false)
        XCTAssertEqual(r.horizons[1].status, .pending)
    }

    func testGapBelowStopCountedAtTheOpen() throws {
        var c = days([(99, 101, 100), (88, 92, 90)])
        c[1].open = 90
        XCTAssertEqual(TradeJournal.review(try entry(), candles: c, now: at + 4 * day).horizons[0].resultR, -2)
    }

    func testCoherenceChecks() throws {
        let d = try decision([
            "verdict": "wait", "label": "Attendre",
            "vetoes": [["code": "v", "label": "Annonce macro dans les 48 h", "active": true, "verifiable": true, "detail": ""]],
            "plan": plan(riskReward: 1.2), "marketRegime": regime("riskOff", "Risk-off"),
        ])
        let e = try entry(price: 104.08, decision: .some(d))
        let checks = Dictionary(uniqueKeysWithValues: TradeJournal.coherenceChecks(e).map { ($0.code, $0) })
        XCTAssertEqual(checks["vetoes"]?.ok, false)
        XCTAssertTrue(checks["vetoes"]?.detail.contains("Annonce macro dans les 48 h") == true)
        XCTAssertEqual(checks["rr"]?.ok, false)
        XCTAssertEqual(checks["zone"]?.detail, "Entrée hors zone d'achat : +2 % au-dessus du haut de la zone.")
        XCTAssertEqual(checks["regime"]?.ok, false)
        XCTAssertEqual(checks["verdict"]?.ok, false)
        XCTAssertEqual(checks["degraded"]?.ok, true)
    }

    func testWithoutDecisionNothingVerifiable() throws {
        let bare = try entry(stop: nil, targets: [], decision: .some(nil))
        let r = TradeJournal.review(bare, candles: days([(99, 101, 100), (100, 104, 103), (101, 105, 104), (102, 106, 105)]), now: at + 4 * day)
        XCTAssertNil(r.coherent)
        XCTAssertNil(r.risk)
        XCTAssertNil(r.horizons[0].resultR)
        XCTAssertEqual(r.worked.first, "+5 % en 3 jours (sans stop, résultat en R non mesurable).")
        let fromPlan = TradeJournal.review(try entry(stop: nil, targets: []), candles: [], now: at)
        XCTAssertEqual(fromPlan.levels, TradeJournal.Levels(stop: 95, stopSource: .plan, targets: [110, 120], targetsSource: .plan))
    }

    func testHistoryStartingAfterEntryGivesNoReview() throws {
        let c = Array(days([(99, 101, 100), (99, 101, 100), (99, 101, 100), (99, 101, 100)]).dropFirst(2))
        XCTAssertEqual(TradeJournal.review(try entry(), candles: c, now: at + 5 * day).horizons[0].status, .noData)
    }

    func testClosedPaperTradeAddsRealizedR() throws {
        let r = TradeJournal.review(try entry(), candles: [], now: at + day, closed: .init(at: at + day, price: 105, reason: "vente manuelle"))
        XCTAssertEqual(r.realizedR, 1)
    }

    func testSaleCheckedAgainstDecision() throws {
        let e = try entry(side: .sell, decision: .some(try decision(["verdict": "trim", "label": "Alléger"])))
        let r = TradeJournal.review(e, candles: days([(99, 101, 100), (95, 99, 96), (94, 97, 95), (93, 96, 94)]), now: at + 4 * day)
        XCTAssertTrue(r.worked.first?.contains("3 jours après la vente, le cours est à −6 %") == true)
        XCTAssertNil(r.planRespected)
        XCTAssertEqual(r.coherent, true)
    }

    // MARK: Profile

    func testProfileGroupsByRatingPlanRegime() throws {
        let win = days([(99, 101, 100), (100, 111, 110), (104, 108, 107), (104, 108, 107)])
        let loss = days([(99, 101, 100), (94, 100, 95), (95, 97, 96), (95, 97, 96)])
        var reviews = try (0..<5).map { TradeJournal.review(try entry(id: "w\($0)"), candles: win, now: at + 5 * day) }
        let lossDecision = try decision(["marketRegime": regime("riskOff", "Risk-off"), "rating": "hold", "ratingLabel": "Conserver"])
        reviews.append(TradeJournal.review(try entry(id: "l", decision: .some(lossDecision)), candles: loss, now: at + 5 * day))
        let p = TradeJournal.profile(reviews, horizon: 3)
        XCTAssertEqual(p.reviewed, 6)
        XCTAssertEqual(p.all?.n, 6)
        XCTAssertEqual(p.all?.avgR, 1.5)
        XCTAssertEqual(p.all?.lowSample, false)
        XCTAssertEqual(p.byRating.map(\.key), ["buy", "hold"])
        XCTAssertEqual(p.byRating.map(\.n), [5, 1])
        XCTAssertEqual(p.byRating.map(\.lowSample), [false, true])
        XCTAssertEqual(p.byPlan.map(\.key), ["yes", "no"])
        XCTAssertEqual(p.byPlan.map(\.n), [5, 1])
        XCTAssertEqual(p.byRegime.map(\.key), ["riskOn", "riskOff"])
        XCTAssertEqual(p.byRegime.map(\.avgR), [2, -1])
        XCTAssertEqual(p.byRegime[1].worstMaeR, -1.2)
        XCTAssertEqual(TradeJournal.profile(reviews, horizon: 10).reviewed, 0)
    }

    func testSavedJournalEmptyValidUnreadable() throws {
        XCTAssertEqual(TradeJournal.parseSaved(nil).state, TradeJournalState())
        XCTAssertNil(TradeJournal.parseSaved(nil).error)
        let s = TradeJournal.add(TradeJournalState(), try entry())
        XCTAssertEqual(TradeJournal.parseSaved(try JSONEncoder().encode(s)).state.entries.count, 1)
        XCTAssertTrue(TradeJournal.parseSaved(Data("{oops".utf8)).error?.contains("illisible") == true)
        XCTAssertTrue(TradeJournal.parseSaved(Data(#"{"version":2,"entries":[]}"#.utf8)).error?.contains("illisible") == true)
    }

    func testSnapshotWithoutPlan() throws {
        XCTAssertNil(TradeJournal.snapshot(try decision(["plan": NSNull()])).plan)
    }
}
