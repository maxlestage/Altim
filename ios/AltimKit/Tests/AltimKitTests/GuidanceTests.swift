import Foundation
import XCTest
@testable import AltimKit

/// Same cases as web/test/guidance.test.ts, on the same real AAPL decision of the server with the guidance fields
/// (backend/tests/samples/decision-guidance.json) and the older BTC answer without them (decision-btc.json).
final class GuidanceTests: XCTestCase {
    func raw(_ name: String) throws -> Data {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try Data(contentsOf: url)
    }

    func aapl() throws -> Decision { try JSONDecoder().decode(Decision.self, from: raw("decision-guidance")) }

    /// The sample with one top-level field replaced.
    func aapl(replacing key: String, with value: Any) throws -> Data {
        var o = try XCTUnwrap(JSONSerialization.jsonObject(with: raw("decision-guidance")) as? [String: Any])
        o[key] = value
        return try JSONSerialization.data(withJSONObject: o)
    }

    func testSampleDecodesWithEveryNewField() throws {
        let d = try aapl()
        XCTAssertEqual(d.noTrade?.active, false)
        XCTAssertGreaterThan(d.noTrade?.unchecked.count ?? 0, 0)
        XCTAssertEqual(d.actionZones?.zones.map(\.kind), [.exit, .invalidation, .buy, .wait, .profit])
        XCTAssertEqual(d.actionZones?.here, .wait)
        XCTAssertEqual(d.scenarios.map { $0.conditions?.count }, [3, 2, 3])
        XCTAssertEqual(d.scenarios.filter { $0.unfolding == true }.map(\.kind), [try XCTUnwrap(d.unfolding).kind])
        let text = try XCTUnwrap(d.counterArgument?.text)
        XCTAssertNotNil(text.range(of: #"^🟢 Raisons favorables : \d+ / 🔴 Raisons défavorables : \d+$"#, options: .regularExpression))
        XCTAssertEqual(d.snapshot?.families.count, d.families.count)
    }

    func testBadGuidanceRefusedOlderAnswersPass() throws {
        let dec = JSONDecoder()
        XCTAssertThrowsError(try dec.decode(Decision.self, from: aapl(replacing: "noTrade", with: ["active": true])))
        XCTAssertThrowsError(try dec.decode(Decision.self, from: aapl(replacing: "actionZones", with: ["zones": [Any]()])))
        XCTAssertThrowsError(try dec.decode(Decision.self, from: aapl(replacing: "counterArgument", with: [String: Any]())))
        XCTAssertThrowsError(try dec.decode(Decision.self, from: aapl(replacing: "snapshot", with: [String: Any]())))
        let old = try dec.decode(Decision.self, from: raw("decision-btc"))
        XCTAssertNil(old.noTrade)
        XCTAssertNil(old.actionZones)
        XCTAssertNil(old.counterArgument)
        XCTAssertNil(old.snapshot)
        XCTAssertNil(old.unfolding)
        XCTAssertTrue(old.scenarios.allSatisfy { $0.conditions == nil && DecisionGuidance.scenarioCount($0) == nil })
    }

    func testNoTradeTexts() throws {
        let n = try XCTUnwrap(try aapl().noTrade)
        XCTAssertEqual(n.badge, "rien à signaler")
        XCTAssertTrue(n.uncheckedText?.hasPrefix("Non vérifié faute de données : ") == true)
        let active = Decision.NoTrade(active: true, headline: "🕰️ Pas le moment de trader : marché sans direction, marché fermé.", reasons: [
            Decision.NoTradeReason(code: "trendless", label: "Marché sans direction", detail: "ADX 14 (sous 20) et sommets et creux sans direction"),
            Decision.NoTradeReason(code: "marketClosed", label: "Marché fermé", detail: "Hors séance régulière de Wall Street"),
        ], unchecked: [])
        XCTAssertEqual(active.badge, "2 raisons")
        XCTAssertNil(active.uncheckedText)
    }

    func testLadderReadsFromHighestPriceDownWithPriceMarked() throws {
        let z = try XCTUnwrap(try aapl().actionZones)
        let rows = DecisionGuidance.ladder(z)
        let kinds: [Decision.ActionZoneKind] = rows.compactMap { if case let .zone(r, _) = $0 { return r.kind } else { return nil } }
        XCTAssertEqual(kinds, [.profit, .wait, .buy, .invalidation, .exit])
        let here: [Decision.ActionZoneKind] = rows.compactMap { if case let .zone(r, h) = $0, h { return r.kind } else { return nil } }
        XCTAssertEqual(here, [.wait])
        XCTAssertFalse(rows.contains { if case .marker = $0 { return true } else { return false } })
        XCTAssertEqual(DecisionGuidance.hereMark(z.price), "◀ vous êtes ici · 341,04\u{202F}$")
        // Price between two zones: a marker of its own, placed above the first zone under it.
        var between = z
        between.price = 275
        between.here = nil
        between.hereText = "Vous êtes ici : entre la zone de sortie et la zone d'achat (275 $)"
        let b = DecisionGuidance.ladder(between)
        let labels: [String] = b.map {
            switch $0 {
            case let .zone(r, _): return r.kind.rawValue
            case let .marker(t): return t
            }
        }
        XCTAssertEqual(labels, ["profit", "wait", "buy", "◀ vous êtes ici : entre la zone de sortie et la zone d'achat (275 $)", "invalidation", "exit"])
        // Price under every zone: the marker is last.
        between.price = 10
        if case .marker = try XCTUnwrap(DecisionGuidance.ladder(between).last) {} else { XCTFail("marker last") }
    }

    func testScenariosChecklistCountsUnfolding() throws {
        let d = try aapl()
        XCTAssertEqual(DecisionGuidance.scenariosBadge(d), "en cours : neutre")
        XCTAssertEqual(d.unfolding?.text, "Scénario neutre en cours : 1 condition sur 2 remplie. Lecture des données actuelles, pas une prévision.")
        let neutral = try XCTUnwrap(d.scenarios.first { $0.kind == .neutral })
        XCTAssertEqual(DecisionGuidance.scenarioCount(neutral), "1/2 conditions · en cours")
        let bull = try XCTUnwrap(d.scenarios.first { $0.kind == .bull })
        XCTAssertEqual(DecisionGuidance.scenarioCount(bull), "1/3 conditions")
        XCTAssertTrue(bull.conditions?.contains { $0.text.hasPrefix("Clôture au-dessus de 345,34") } == true)
        XCTAssertEqual(bull.conditions?.map(\.state), [.unmet, .unmet, .met])
        XCTAssertEqual(Decision.CheckState.met.icon + " " + Decision.CheckState.met.label, "✓ remplie")
        XCTAssertEqual(Decision.CheckState.unmet.label, "non remplie")
    }

    func testCounterArgumentAndWhyTheSignalChanged() throws {
        let d = try aapl()
        let c = try XCTUnwrap(d.counterArgument)
        XCTAssertTrue(c.invalidators.contains { $0.text.hasPrefix("Cassure de la résistance 344,95") })
        var before = d
        before.verdict = .buyZone
        before.label = "ZONE D'ACHAT"
        before.level = .moderate
        before.levelLabel = "Signal modéré"
        before.snapshot?.composite = 30
        before.snapshot?.relativeVolume = 1.4
        // Date.UTC(2026, 8, 28, 12, 2) and Date.UTC(2026, 8, 28, 16).
        let s1 = ConfigChanges.apply(ConfigState(), before, personal: false, now: 1_790_596_920_000).state
        let t = try XCTUnwrap(ConfigChanges.apply(s1, d, personal: false, now: 1_790_611_200_000).transition)
        XCTAssertEqual(DecisionGuidance.changeTitle(t), "Pourquoi le signal a changé depuis le 28/09 à 14:02")
        XCTAssertEqual(DecisionGuidance.changeSubtitle(t), "ZONE D'ACHAT → ATTENDRE")
        XCTAssertTrue(t.changes.contains("Score composite +30 → +41"))
        XCTAssertTrue(t.changes.contains("Volume en baisse (1,4× → 0,7× la moyenne)"))
        XCTAssertEqual(ConfigChanges.latestChange(ConfigChanges.apply(s1, d, personal: false, now: 1_790_611_200_000).state.transitions, d)?.since, 1_790_596_920_000)
    }
}
