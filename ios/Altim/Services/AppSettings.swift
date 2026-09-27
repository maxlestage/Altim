import Foundation
import Observation
import AltimCore

/// Préférences persistées de l'utilisateur.
@Observable
final class AppSettings {
    private static let storageKey = "altim.settings.v1"

    private struct Snapshot: Codable {
        var watchlist: [Asset]
        var risk: RiskSettings
        var cryptoEnvironment: BrokerEnvironment
        var stockEnvironment: BrokerEnvironment
        var demoMode: Bool
        var notificationsEnabled: Bool
        var acceptedDisclaimer: Bool
        var timeframe: Timeframe
    }

    var watchlist: [Asset] = Asset.defaults { didSet { save() } }
    var risk = RiskSettings() { didSet { save() } }
    /// Environnement Binance (test = testnet, argent fictif).
    var cryptoEnvironment: BrokerEnvironment = .test { didSet { save() } }
    /// Environnement Alpaca (test = paper trading).
    var stockEnvironment: BrokerEnvironment = .test { didSet { save() } }
    /// Mode démo : courtier simulé local, aucune clé nécessaire.
    var demoMode = true { didSet { save() } }
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
        cryptoEnvironment = s.cryptoEnvironment
        stockEnvironment = s.stockEnvironment
        demoMode = s.demoMode
        notificationsEnabled = s.notificationsEnabled
        acceptedDisclaimer = s.acceptedDisclaimer
        timeframe = s.timeframe
        loading = false
    }

    func environment(for asset: Asset) -> BrokerEnvironment {
        demoMode ? .test : (asset.assetClass == .crypto ? cryptoEnvironment : stockEnvironment)
    }

    private func save() {
        guard !loading else { return }
        let snapshot = Snapshot(watchlist: watchlist, risk: risk, cryptoEnvironment: cryptoEnvironment,
                                stockEnvironment: stockEnvironment, demoMode: demoMode,
                                notificationsEnabled: notificationsEnabled, acceptedDisclaimer: acceptedDisclaimer,
                                timeframe: timeframe)
        if let data = try? JSONEncoder().encode(snapshot) {
            UserDefaults.standard.set(data, forKey: Self.storageKey)
        }
    }
}
