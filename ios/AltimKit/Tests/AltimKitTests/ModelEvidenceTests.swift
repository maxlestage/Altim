import Foundation
import XCTest
@testable import AltimKit

/// « Preuve du modèle » (`modelEvidence`, `ratingReason`), same cases as web/test/decision.test.ts.
///
/// Fixture decision-evidence.json: the real AAPL decision of the server (backend/tests/samples/decision-guidance.json,
/// saved before this field existed) with the evidence block ADDED by hand — `"ratingReason": null` and the
/// `modelEvidence` the backend produces for AAPL from the saved real validation run
/// (backend/tests/samples/validation.json, test `model_evidence_from_the_saved_validation` of backend/tests/decision.rs,
/// serialized as is). The rest of the decision is unchanged (its confidence and cons predate the evidence).
final class ModelEvidenceTests: XCTestCase {
    func raw(_ name: String) throws -> Data {
        let url = try XCTUnwrap(Bundle.module.url(forResource: name, withExtension: "json", subdirectory: "Fixtures"))
        return try Data(contentsOf: url)
    }

    /// The evidence sample with its `modelEvidence` replaced.
    func sample(evidence: Any) throws -> Data {
        var o = try XCTUnwrap(JSONSerialization.jsonObject(with: raw("decision-evidence")) as? [String: Any])
        o["modelEvidence"] = evidence
        return try JSONSerialization.data(withJSONObject: o)
    }

    func testRealDecisionWithEvidenceDecodes() throws {
        let d = try JSONDecoder().decode(Decision.self, from: raw("decision-evidence"))
        XCTAssertEqual(d.symbol, "AAPL")
        XCTAssertNil(d.ratingReason)
        let e = try XCTUnwrap(d.modelEvidence)
        XCTAssertTrue(e.available)
        XCTAssertEqual(e.assetClass, "stock")
        XCTAssertEqual(e.classLabel, "Actions et ETF américains")
        XCTAssertEqual(e.classVerdict, .edge)
        XCTAssertEqual(e.classVerdictLabel, "Gain moyen positif (t ≥ 2), à confirmer")
        XCTAssertEqual([e.assets, e.beatHoldCount, e.trades, e.regimeTrades], [22, 2, 650, 486])
        XCTAssertEqual(e.beatHold, "2/22")
        XCTAssertEqual(e.tStat, 2.18)
        XCTAssertEqual(e.regime, "bull")
        XCTAssertEqual(e.regimeLabel, "marché haussier")
        XCTAssertEqual(e.regimeVerdict, .unproven)
        XCTAssertEqual(e.regimeTStat, 1.76)
        XCTAssertTrue(e.weak)
        XCTAssertEqual(
            e.text,
            "Sur les actions et ETF testés (22), gain moyen positif par trade (t = 2,2) mais la simple détention a fait mieux dans 20 cas sur 22 ; en marché haussier, régime actuel de l'actif : avantage non démontré (t = 1,8). Résultats passés, sans garantie pour la suite."
        )
        XCTAssertEqual(e.asOf, 1_790_662_942_496)
        XCTAssertEqual(e.link, "/app/validation")
        // What the card shows.
        XCTAssertEqual(e.tone, .weak)
        XCTAssertEqual(ModelEvidence.title, "Preuve du modèle")
        XCTAssertEqual(e.beatHoldChip, "bat la détention : 2/22")
        XCTAssertEqual(ModelEvidence.linkText, "Voir la validation du modèle →")
        XCTAssertEqual(e.asOfText, " · calculée le \(DecisionGuidance.shortDateTime(1_790_662_942_496))")
        XCTAssertEqual(e.asOfText, " · calculée le 29/09 à 08:22")
    }

    func testRatingReasonDecodes() throws {
        var o = try XCTUnwrap(JSONSerialization.jsonObject(with: raw("decision-evidence")) as? [String: Any])
        let why = "ACHAT plutôt que ACHAT FORT : la validation du modèle sur les actions et ETF américains (22 actifs) ne montre pas d'avantage du signal (la simple détention a fait mieux sur la plupart des actifs)."
        o["ratingReason"] = why
        let d = try JSONDecoder().decode(Decision.self, from: JSONSerialization.data(withJSONObject: o))
        XCTAssertEqual(d.ratingReason, why)
    }

    func testNotComputedYetOlderAnswersAndBadBlock() throws {
        let dec = JSONDecoder()
        // Not computed yet: said plainly, no chip, no date (the backend's default block).
        let na: [String: Any] = [
            "available": false, "assetClass": "btc", "classLabel": "Bitcoin", "classVerdict": NSNull(), "classVerdictLabel": NSNull(),
            "assets": 0, "beatHoldCount": 0, "beatHold": NSNull(), "trades": 0, "tStat": NSNull(), "regime": "unknown",
            "regimeLabel": "régime inconnu (historique trop court)", "regimeVerdict": NSNull(), "regimeTrades": 0, "regimeTStat": NSNull(),
            "weak": false, "text": "Validation pas encore calculée : ouvrez l'écran « Validation du modèle » pour la lancer (quelques minutes).",
            "asOf": NSNull(), "link": "/app/validation",
        ]
        let e = try XCTUnwrap(try dec.decode(Decision.self, from: sample(evidence: na)).modelEvidence)
        XCTAssertFalse(e.available)
        XCTAssertEqual(e.tone, .na)
        XCTAssertNil(e.beatHoldChip)
        XCTAssertNil(e.asOfText)
        XCTAssertTrue(e.text.hasPrefix("Validation pas encore calculée"))
        // Only `available` and `text` required: the other fields fall back.
        let minimal = try XCTUnwrap(try dec.decode(Decision.self, from: sample(evidence: ["available": true, "text": "…", "classVerdict": "edge"])).modelEvidence)
        XCTAssertEqual(minimal.tone, .edge)
        XCTAssertEqual(minimal.assets, 0)
        XCTAssertNil(minimal.beatHoldChip)
        // Unproven and negative classes.
        XCTAssertEqual(try dec.decode(Decision.self, from: sample(evidence: ["available": true, "text": "…", "classVerdict": "insufficient"])).modelEvidence?.tone, .unproven)
        XCTAssertEqual(try dec.decode(Decision.self, from: sample(evidence: ["available": true, "text": "…", "classVerdict": "negative"])).modelEvidence?.tone, .weak)
        // A malformed block fails (an error instead of a broken card), as the web's check.
        XCTAssertThrowsError(try dec.decode(Decision.self, from: sample(evidence: ["available": "yes", "text": "…"])))
        XCTAssertThrowsError(try dec.decode(Decision.self, from: sample(evidence: ["available": true])))
        // Older answers: nothing.
        for name in ["decision-guidance", "decision-btc", "decision-aapl-real"] {
            let old = try dec.decode(Decision.self, from: raw(name))
            XCTAssertNil(old.modelEvidence, name)
            XCTAssertNil(old.ratingReason, name)
        }
    }
}
