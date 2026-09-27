#if canImport(ActivityKit)
import ActivityKit
import Foundation

/// Live Activity of one asset (lock screen and Dynamic Island): shared by the app, which starts and updates it,
/// and the widget extension, which draws it.
struct PriceActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        var price: Double
        /// Change over 24 h, in %.
        var change: Double?
        /// "ACHAT", "ATTENDRE"… (4 h signal), nil if unknown.
        var signal: String?
        /// Buyable according to Altim's rule (signal or zone, nothing blocking).
        var buy: Bool
        /// One line: why (or why not) it can be bought.
        var note: String
        var updated: Date
    }

    var symbol: String
    var name: String
    var kind: String
}
#endif
