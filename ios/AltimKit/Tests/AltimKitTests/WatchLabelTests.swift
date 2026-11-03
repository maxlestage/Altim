import XCTest
@testable import AltimKit

/// The Watch speaks like the iPhone's alert titles, and never says ATTENDRE (which would hide a SORTIR verdict).
final class WatchLabelTests: XCTestCase {
    func testLabels() {
        func a(_ buy: Bool, _ strong: Bool) -> BuyAlert {
            BuyAlert(symbol: "X", kind: .stock, name: "X", price: 1, buy: buy, strong: strong, key: "", title: "", body: "")
        }
        XCTAssertEqual(a(true, true).shortLabel, "ACHAT CONSEILLÉ")
        XCTAssertEqual(a(true, false).shortLabel, "ACHAT POSSIBLE")
        XCTAssertEqual(a(false, false).shortLabel, "PAS D'ACHAT")
        XCTAssertEqual(a(false, false).spokenLabel, "Pas d'achat pour l'instant")
        for x in [a(true, true), a(true, false), a(false, false)] {
            XCTAssertFalse(x.shortLabel.contains("ATTENDRE"))
            XCTAssertFalse(x.shortLabel.contains("ZONE"), "la zone Fibonacci n'est pas un conseil d'achat mesuré")
        }
    }
}
