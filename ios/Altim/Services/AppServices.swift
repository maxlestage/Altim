import Foundation
import Observation
import UserNotifications
import AltimCore

/// Shared dependencies: market data, holdings database, notifications.
/// Altim is an advisor: no broker, no orders.
@MainActor
@Observable
final class AppServices {
    /// Shared network transport: retries, circuit breaker per source.
    let transport: ResilientTransport
    /// Holdings entered by the user: SQLite database on the device.
    let holdingsDB: HoldingsDatabase?
    let holdingsDBError: String?

    /// Multi-source market data with cross-validation (rebuilt with the keys currently entered).
    var market: ConsensusMarketData { Self.makeMarket(transport: transport) }
    var sentiment: SentimentProvider { SentimentProvider(transport: transport) }

    init() {
        transport = ResilientTransport()
        do {
            holdingsDB = try HoldingsDatabase(path: HoldingsDatabase.defaultURL().path)
            holdingsDBError = nil
        } catch {
            holdingsDB = nil
            holdingsDBError = error.localizedDescription
        }
    }

    nonisolated static func makeMarket(transport: HTTPTransport) -> ConsensusMarketData {
        let alpacaKey = KeychainStore.get(.alpacaKey)
        let alpacaSecret = KeychainStore.get(.alpacaSecret)
        return .standard(transport: transport,
                         alpaca: alpacaKey.isEmpty ? nil : (key: alpacaKey, secret: alpacaSecret),
                         twelveDataKey: KeychainStore.get(.twelveDataKey),
                         polygonKey: KeychainStore.get(.polygonKey),
                         finnhubKey: KeychainStore.get(.finnhubKey))
    }

    // MARK: Notifications

    func requestNotificationPermission() async -> Bool {
        (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) ?? false
    }

    func notify(asset: Asset, signal: Signal) {
        let content = UNMutableNotificationContent()
        content.title = "\(signal.action.label) · \(asset.name)"
        content.body = String(format: "Score %.0f · confiance %.0f %% · prix %@", signal.score, signal.confidence, Format.price(signal.price))
        content.sound = .default
        let request = UNNotificationRequest(identifier: "\(asset.id)-\(signal.time.timeIntervalSince1970)", content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }
}
