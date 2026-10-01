import Foundation
import XCTest
@testable import AltimKit

/// « Bots sélectifs » (v4): the real /api/bot answer of 01/10/2026 (backend/tests/samples/bot-v4.json, reduced to its
/// `v4` and the basket rows as Fixtures/bot-v4.json), the v3 answer still read without v4, the texts and the `v4` of a
/// view (decision line).
final class BotV4Tests: XCTestCase {
    func raw(_ name: String) throws -> Data {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try Data(contentsOf: url)
    }

    func report(_ name: String) throws -> BotReport { try JSONDecoder().decode(BotReport.self, from: raw(name)) }

    func testRealReportSixteenBotsAndCorrectedThreshold() throws {
        let r = try report("bot-v4")
        let v4 = try XCTUnwrap(r.v4)
        XCTAssertEqual(v4.preregDate, "2026-10-01")
        XCTAssertEqual(v4.k?.total, 130)
        XCTAssertEqual(v4.k?.v4, 16)
        XCTAssertLessThan(abs(v4.tRequired - 3.5504), 0.001)
        XCTAssertEqual(v4.minSignals, 30)
        XCTAssertEqual(v4.bots.count, 16)
        XCTAssertEqual(v4.bots.prefix(4).map(\.side), [.rise, .fall, .top, .bottom])
        XCTAssertEqual(v4.bots.first?.id, "stock-20-rise")
        XCTAssertEqual(v4.bots.last?.id, "crypto-60-bottom")
        XCTAssertTrue(v4.headline.hasPrefix("Bots sélectifs : "))
        XCTAssertEqual(ModelBotV4.cards(v4).map(\.bots.count), [4, 4, 4, 4])
        XCTAssertEqual(ModelBotV4.cards(v4).first?.title, "Actions et ETF américains · 20 jours")
        for b in v4.bots {
            // Never « prouvé » before 30 forward signals.
            if b.forward.signals < 30 { XCTAssertNotEqual(b.status, .proven) }
            if b.status != .notProven { XCTAssertGreaterThanOrEqual(b.main.t ?? 0, v4.tRequired) }
            if b.main.signals > 0 {
                XCTAssertLessThanOrEqual(b.main.wilsonLow ?? 0, b.main.precision ?? 0)
                XCTAssertGreaterThanOrEqual(b.main.wilsonHigh ?? 0, b.main.precision ?? 0)
            }
            XCTAssertFalse(ModelBotV4.todayText(b).isEmpty)
        }
        XCTAssertTrue(r.assets.allSatisfy { $0.v4 != nil })
        // The v3 answer reads without v4.
        let v3 = try report("bot")
        XCTAssertNil(v3.v4)
        XCTAssertTrue(v3.assets.allSatisfy { $0.v4 == nil })
    }

    func testTexts() {
        let s = BotV4Stats(signals: 120, hits: 80, precision: 66.7, wilsonLow: 57.8, wilsonHigh: 74.5, reference: 54.2, perYear: 4.3)
        XCTAssertEqual(ModelBotV4.precisionShort(s), "67\u{a0}% (58\u{a0}%–75\u{a0}%)")
        XCTAssertEqual(ModelBotV4.precisionShort(BotV4Stats()), "aucun signal")
        XCTAssertEqual(ModelBotV4.rhythm(s), "120 · 4,3 signaux par an")
        XCTAssertEqual(ModelBotV4.shortLabel("stock-20-rise"), "Hausse 20 j")
        XCTAssertEqual(ModelBotV4.shortLabel("crypto-60-bottom"), "Bas du classement 60 j")
        XCTAssertEqual(ModelBotV4.todayText(BotV4Bot(id: "a", todayLevel: nil)), "pas d'avis (seuil fermé sur la dernière année)")
        XCTAssertEqual(ModelBotV4.todayText(BotV4Bot(id: "a", todayLevel: 5)), "pas d'avis")
        XCTAssertEqual(ModelBotV4.todayText(BotV4Bot(id: "a", side: .top, todayLevel: 5, today: ["AAPL", "NVDA"])),
                       "ACHETER (dans le haut de son groupe) : AAPL, NVDA")
        XCTAssertEqual(BotV4Status.pending.label, "en attente")
    }

    func testViewInADecision() throws {
        let json = """
        {"available": true, "action": "wait", "counts": false, "note": "Bots sélectifs : pas d'avis aujourd'hui (aucun score assez extrême) ; ils ne comptent pas.",
         "v3": {"available": true, "signals": [], "counts": false, "nudge": 0, "tRequired": 3.5157, "note": "Aucun avantage démontré"},
         "v4": {"available": true, "counts": false, "nudge": 0, "note": "n", "signals": [
           {"id": "stock-20-rise", "label": "Hausse à 20 jours (actions)", "horizon": 20, "side": "rise", "action": "buy", "probability": 71.2,
            "precision": 61.0, "wilsonLow": 55.0, "wilsonHigh": 66.6, "reference": 56.1, "perYear": 6.2, "status": "notProven", "counts": false, "text": "t"},
           {"id": "stock-20-fall", "label": "Baisse à 20 jours (actions)", "horizon": 20, "side": "fall", "status": "weird", "counts": false, "text": "t"}]}}
        """
        let v = try JSONDecoder().decode(BotView.self, from: Data(json.utf8))
        let w = try XCTUnwrap(v.v4)
        XCTAssertEqual(w.signals.count, 2)
        XCTAssertEqual(w.speaking.map(\.id), ["stock-20-rise"])
        XCTAssertEqual(w.signals[1].status, .notProven, "an unknown status reads as « non prouvé »")
        XCTAssertEqual(ModelBotV4.viewHead(w), "1 avis sur 2 bots")
        XCTAssertEqual(ModelBotV4.signalText(w.signals[0]),
                       "ACHETER (hausse attendue) · précision mesurée 61\u{a0}% (55\u{a0}%–67\u{a0}%) contre 56\u{a0}% au hasard · 6,2 signaux par an")
        // With v4, the server's counts and note decide.
        XCTAssertFalse(v.countsNow)
        XCTAssertEqual(v.noteNow, "Bots sélectifs : pas d'avis aujourd'hui (aucun score assez extrême) ; ils ne comptent pas.")
        // Encodes and decodes back (the view is cached).
        let back = try JSONDecoder().decode(BotView.self, from: JSONEncoder().encode(v))
        XCTAssertEqual(back.v4, v.v4)
    }
}
