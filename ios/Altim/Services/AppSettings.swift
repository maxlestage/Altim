import Foundation
import Observation
import AltimCore

/// Persisted user preferences.
@Observable
final class AppSettings {
    private static let storageKey = "altim.settings.v1"

    /// Stored JSON (extra fields from older versions are ignored).
    private struct Snapshot: Codable {
        var watchlist: [Asset]
        var risk: RiskSettings
        var notificationsEnabled: Bool
        var acceptedDisclaimer: Bool
        var timeframe: Timeframe
    }

    var watchlist: [Asset] = Asset.defaults { didSet { save() } }
    /// Prudence of the advice: share of wealth risked per idea, maximum size of a line.
    var risk = RiskSettings() { didSet { save() } }
    var notificationsEnabled = false { didSet { save() } }
    var acceptedDisclaimer = false { didSet { save() } }
    var timeframe: Timeframe = .h4 { didSet { save() } }

    @ObservationIgnored private var loading = false

    init() {
        guard let data = UserDefaults.standard.data(forKey: Self.storageKey),
              let s = try? JSONDecoder().decode(Snapshot.self, from: data) else { return }
        loading = true
        watchlist = s.watchlist
        risk = s.risk
        notificationsEnabled = s.notificationsEnabled
        acceptedDisclaimer = s.acceptedDisclaimer
        timeframe = s.timeframe
        loading = false
    }

    private func save() {
        guard !loading else { return }
        let snapshot = Snapshot(watchlist: watchlist, risk: risk, notificationsEnabled: notificationsEnabled,
                                acceptedDisclaimer: acceptedDisclaimer, timeframe: timeframe)
        if let data = try? JSONEncoder().encode(snapshot) {
            UserDefaults.standard.set(data, forKey: Self.storageKey)
        }
    }
}
