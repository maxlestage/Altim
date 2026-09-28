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
        XCTAssertEqual(ConfigChanges.parse(Data(#"{"version":3,"last":{},"transitions":[]}"#.utf8)), ConfigState())
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

    // MARK: Why the signal changed (web "why the signal changed")

    func m(price: Double? = 342, composite: Double? = 24, families: [SignalMetrics.Family]? = nil, relVolume: Double? = 1.4, rsi: Double? = 62,
           support: SignalMetrics.Level? = SignalMetrics.Level(price: 335.61, touches: 2),
           resistance: SignalMetrics.Level? = SignalMetrics.Level(price: 344.95, touches: 2),
           newsScore: Double? = 0, topNews: SignalMetrics.News? = nil) -> SignalMetrics {
        SignalMetrics(price: price, composite: composite,
                      families: families ?? [.init(key: "trend", label: "Tendance", score: 50), .init(key: "momentum", label: "Momentum", score: 40),
                                             .init(key: "news", label: "Actualités", score: 0)],
                      relVolume: relVolume, rsi: rsi, support: support, resistance: resistance, newsScore: newsScore, topNews: topNews)
    }

    func testEveryMeasuredChangeToldMostTellingFirst() {
        let next = m(composite: 6,
                     families: [.init(key: "trend", label: "Tendance", score: 45), .init(key: "momentum", label: "Momentum", score: 22),
                                .init(key: "news", label: "Actualités", score: nil)],
                     relVolume: 0.7, rsi: 48, resistance: SignalMetrics.Level(price: 344.9, touches: 3),
                     topNews: SignalMetrics.News(title: "Apple cuts iPhone orders", tone: "negative"))
        XCTAssertEqual(ConfigChanges.explainChange(m(), next), [
            "Score composite +24 → +6",
            "Momentum −18 pts (+40 → +22)",
            "Actualités : plus mesuré(e) (données indisponibles)",
            "Volume en baisse (1,4× → 0,7× la moyenne)",
            "RSI 62 → 48",
            "Résistance 344,90\u{202F}$ confirmée (touchée 3 fois)",
            "Actualité négative : « Apple cuts iPhone orders »",
        ])
    }

    func testLevelsBrokenOrCrossedSmallMovesIgnored() {
        XCTAssertEqual(ConfigChanges.explainChange(m(), m(price: 330, composite: 22, rsi: 58)), ["Support 335,61\u{202F}$ cassé (prix 330\u{202F}$)"])
        XCTAssertEqual(ConfigChanges.explainChange(m(), m(price: 346, resistance: SignalMetrics.Level(price: 360, touches: 2))),
                       ["Résistance 344,95\u{202F}$ franchie (prix 346\u{202F}$)"])
        XCTAssertEqual(ConfigChanges.explainChange(m(), m()), [])
    }

    func testTransitionCarriesExplanationRatingChangeAloneIsATransition() throws {
        let d = try btc()
        func scored(_ value: Double, rating: Decision.Rating, label: String) -> Decision {
            var x = d
            x.rating = rating
            x.ratingLabel = label
            x.score = Decision.CompositeScore(value: value, label: "", factors: [], missing: [], custom: false, text: "")
            return x
        }
        let s = ConfigChanges.apply(ConfigState(), scored(24, rating: .buy, label: "ACHAT"), personal: false, now: 1).state
        XCTAssertEqual(s.last.values.first?.metrics?.families.count, d.families.count)
        let t = try XCTUnwrap(ConfigChanges.apply(s, scored(-30, rating: .hold, label: "ATTENDRE"), personal: false, now: 2).transition)
        XCTAssertEqual(t.title, "🚨 BTC — changement de configuration : note ACHAT → note ATTENDRE")
        // Older decision without `snapshot`: the measurements are read from the decision itself.
        XCTAssertEqual(t.changes, ["Score composite +24 → −30"])
    }

    /// Data saved by the previous version of the app (version 1) is migrated; damaged measurements alone are dropped.
    func testVersionOneMigratedDamagedMeasurementsDropped() throws {
        let v1 = #"""
        {"version":1,"last":{"crypto:BTC:i":{"verdict":"wait","level":"waiting","label":"ATTENDRE","levelLabel":"Attente","missing":[],"triggers":[],"at":1}},
         "transitions":[{"symbol":"BTC","kind":"crypto","name":"Bitcoin","personal":false,"at":2,"since":1,
           "from":{"verdict":"buy","level":"moderate","label":"ACHETER","levelLabel":"Signal modéré"},
           "to":{"verdict":"wait","level":"waiting","label":"ATTENDRE","levelLabel":"Attente"},"missing":[],"triggers":[]}]}
        """#
        let p = ConfigChanges.parse(Data(v1.utf8))
        XCTAssertEqual(p.version, 2)
        XCTAssertNil(p.last["crypto:BTC:i"]?.metrics)
        XCTAssertEqual(p.transitions.first?.changes, [])
        XCTAssertEqual(p.transitions.count, 1)
        let bad = v1.replacingOccurrences(of: #""version":1"#, with: #""version":2"#)
            .replacingOccurrences(of: #""triggers":[],"at":1}"#, with: #""triggers":[],"at":1,"metrics":{"price":"x"}}"#)
        let q = ConfigChanges.parse(Data(bad.utf8))
        XCTAssertEqual(Array(q.last.keys), ["crypto:BTC:i"])
        XCTAssertNil(q.last["crypto:BTC:i"]?.metrics)
        // A migrated baseline (no measurements) gives a transition without explanation, never a made-up one.
        let t = try XCTUnwrap(ConfigChanges.apply(p, with(try btc(), verdict: .buy, label: "ACHETER"), personal: false, now: 3).transition)
        XCTAssertEqual(t.changes, [])
        // Saved again as version 2 and read back the same.
        let saved = ConfigChanges.apply(p, try btc(), personal: false, now: 3).state
        XCTAssertEqual(saved.version, 2)
        XCTAssertEqual(try JSONDecoder().decode(ConfigState.self, from: JSONEncoder().encode(saved)), saved)
    }

    func testDecisionCardFindsTheChangeThatLedToIt() throws {
        let d = try btc()
        let s = ConfigChanges.apply(ConfigState(), d, personal: false, now: 1).state
        let next = with(d, verdict: .buyZone, level: .moderate)
        let state = ConfigChanges.apply(s, next, personal: false, now: 2).state
        XCTAssertEqual(ConfigChanges.latestChange(state.transitions, next)?.since, 1)
        XCTAssertNil(ConfigChanges.latestChange(state.transitions, d))
        var personal = next
        personal.mode = "personal"
        XCTAssertNil(ConfigChanges.latestChange(state.transitions, personal))
    }
}
