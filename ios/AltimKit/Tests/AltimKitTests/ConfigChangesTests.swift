import Foundation
import XCTest
@testable import AltimKit

/// Same cases as web/test/config-changes.test.ts, on the same decision sample (backend/tests/samples/decision-btc.json).
final class ConfigChangesTests: XCTestCase {
    func btc() throws -> Decision {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "decision-btc", withExtension: "json", subdirectory: "Fixtures"))
        return try JSONDecoder().decode(Decision.self, from: Data(contentsOf: url))
    }

    func with(_ d: Decision, verdict: Decision.Verdict? = nil, label: String? = nil, level: Decision.Level? = nil, levelLabel: String? = nil) -> Decision {
        var x = d
        if let verdict { x.verdict = verdict }
        if let label { x.label = label }
        if let level { x.level = level }
        if let levelLabel { x.levelLabel = levelLabel }
        return x
    }

    func testFirstSightingThenTransition() throws {
        let d = try btc()
        let first = ConfigChanges.apply(ConfigState(), d, personal: false, now: 1000)
        XCTAssertNil(first.transition)
        let next = with(d, verdict: .buyZone, label: "ZONE D'ACHAT", level: .moderate, levelLabel: "Signal modéré")
        let second = ConfigChanges.apply(first.state, next, personal: false, now: 2000)
        let t = try XCTUnwrap(second.transition)
        XCTAssertEqual(t.from.verdict, d.verdict)
        XCTAssertEqual(t.to.verdict, .buyZone)
        XCTAssertEqual(t.since, 1000)
        XCTAssertEqual(t.title, "🚨 BTC — changement de configuration : \(d.label) → ZONE D'ACHAT")
        XCTAssertEqual(t.missing, ConfigChanges.snapshot(of: next, at: 0).missing)
        XCTAssertEqual(t.missing.count, d.setup.steps.filter { $0.state != .ok }.count)
        XCTAssertEqual(t.triggers.count, d.toBuy.count)
        XCTAssertEqual(t.tone, .good)
        // Same configuration again: nothing new.
        XCTAssertNil(ConfigChanges.apply(second.state, next, personal: false, now: 3000).transition)
        // Personal mode has its own baseline.
        XCTAssertNil(ConfigChanges.apply(second.state, d, personal: true, now: 3000).transition)
    }

    /// The texts kept are the web's (snapshotOf on the same sample).
    func testSnapshotTexts() throws {
        let s = ConfigChanges.snapshot(of: try btc(), at: 1000)
        XCTAssertEqual(s.missing, [
            "Correction en cours : pas encore (Prix à 2 % de son plus haut récent)",
            "Retour sur un support (zone d'achat) : pas encore (Zone à 3,6 % sous le prix)",
            "Volume en baisse pendant la correction : non vérifiable (Pas de correction en cours)",
            "Signal de retournement : pas encore (—)",
            "Confirmation (clôture au-dessus du plus haut de la veille) : pas encore (—)",
        ])
        XCTAssertEqual(s.triggers, [
            "Repli dans la zone 78 400 – 80 100 $",
            "Puis une clôture journalière au-dessus du plus haut de la veille",
            "Ou une cassure au-dessus de 86 400 $ avec un volume supérieur à 1,5 fois la moyenne",
        ])
        var d = try btc()
        d.toBuy = [Decision.Condition(text: "Clôture au-dessus du plus haut", level: 86_400.5), Decision.Condition(text: "Petit prix", level: 0.001234)]
        XCTAssertEqual(ConfigChanges.snapshot(of: d, at: 0).triggers, ["Clôture au-dessus du plus haut (86\u{202F}400,5 $)", "Petit prix (0,001234 $)"])
    }

    func testLevelChangeAlone() throws {
        let s = ConfigChanges.apply(ConfigState(), try btc(), personal: false, now: 1).state
        let t = try XCTUnwrap(ConfigChanges.apply(s, with(try btc(), level: .highRisk, levelLabel: "Risque élevé"), personal: false, now: 2).transition)
        XCTAssertTrue(t.title.contains("→ Risque élevé"))
    }

    func testAtMostFiftyNewestFirst() throws {
        var s = ConfigState()
        let d = try btc()
        for i in 0..<60 { s = ConfigChanges.apply(s, with(d, verdict: i % 2 == 1 ? .wait : .buy), personal: false, now: Double(i)).state }
        XCTAssertEqual(s.transitions.count, ConfigChanges.maxTransitions)
        XCTAssertEqual(s.transitions[0].at, 59)
    }

    func testDamagedStorageDropped() throws {
        XCTAssertEqual(ConfigChanges.parse(Data("{nope".utf8)), ConfigState())
        XCTAssertEqual(ConfigChanges.parse(Data(#"{"version":2,"last":{},"transitions":[]}"#.utf8)), ConfigState())
        XCTAssertEqual(ConfigChanges.parse(nil), ConfigState())
        let d = try btc()
        let good = ConfigChanges.apply(ConfigChanges.apply(ConfigState(), d, personal: false, now: 1).state, with(d, verdict: .sell), personal: false, now: 2).state
        var json = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(good)) as? [String: Any])
        var last = try XCTUnwrap(json["last"] as? [String: Any])
        last["bad"] = ["verdict": "hodl"]
        json["last"] = last
        json["transitions"] = (json["transitions"] as? [Any] ?? []) + [["symbol": 1]]
        let p = ConfigChanges.parse(try JSONSerialization.data(withJSONObject: json))
        XCTAssertEqual(Array(p.last.keys), Array(good.last.keys))
        XCTAssertEqual(p.transitions.count, 1)
        XCTAssertEqual(p, good)
        // An unknown verdict saved by a newer version is dropped, never trusted.
        var unknown = good
        unknown.transitions[0].to.verdict = .unknown
        XCTAssertTrue(ConfigChanges.parse(try JSONEncoder().encode(unknown)).transitions.isEmpty)
    }
}
