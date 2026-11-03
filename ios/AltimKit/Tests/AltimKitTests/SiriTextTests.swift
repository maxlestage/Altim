import XCTest
@testable import AltimKit

/// Siri's answers: a wait forced by the data is said as such, an asset without data is counted.
final class SiriTextTests: XCTestCase {
    func row(_ s: String, _ a: Action?, suspended: Bool? = nil, change: Double? = nil, error: String? = nil) -> RadarRow {
        RadarRow(symbol: s, kind: .crypto, name: s, price: 10, change: change, priceSources: nil,
                 signal: a.map { SignalSummary(action: $0, score: 0, confidence: 50, suspended: suspended) },
                 reliability: nil, agreeing: nil, sources: nil, sparkline: nil, error: error)
    }

    func testRadarSummary() {
        let text = SiriText.radar([row("A", .buy), row("B", .sell), row("C", .hold), row("D", .hold, suspended: true), row("E", nil, error: "HTTP 404")])
        XCTAssertTrue(text.hasPrefix("Signal d'achat sur A. Signal de sortie sur B. 1 actif à attendre. Verdict suspendu faute de données à jour ou fiables pour D. 1 actif sans données pour l'instant."), text)
        XCTAssertTrue(text.hasSuffix("Altim ne passe aucun ordre."))
        XCTAssertEqual(SiriText.radar([row("E", nil, error: "x")]), "Les marchés ne répondent pas pour l'instant. Réessayez dans un moment.")
    }

    func testAssetAnswer() {
        let a = Asset(symbol: "D", kind: .crypto, name: "Dcoin")
        XCTAssertTrue(SiriText.asset(a, row("D", .hold, suspended: true)).hasSuffix("Verdict Altim suspendu : données en retard ou peu fiables."))
        XCTAssertFalse(SiriText.asset(a, row("D", .hold)).contains("suspendu"))
        XCTAssertEqual(SiriText.asset(a, nil), "Le cours de Dcoin est indisponible pour l'instant.")
    }
}
