import Foundation
import Observation
import UserNotifications
import AltimCore

/// Dépendances partagées : données de marché, courtiers, journal.
@MainActor
@Observable
final class AppServices {
    /// Shared network transport: retries, circuit breaker per source.
    let transport: ResilientTransport
    let journal = TradeJournal()
    /// Simulated broker shared across the session: its balances persist.
    let paperBroker: PaperBroker

    /// Multi-source market data with cross-validation (rebuilt with the keys currently entered).
    var market: ConsensusMarketData { Self.makeMarket(transport: transport) }
    var sentiment: SentimentProvider { SentimentProvider(transport: transport) }

    init() {
        let transport = ResilientTransport()
        self.transport = transport
        self.paperBroker = PaperBroker(startingCash: 10_000, quoteAsset: "USDT") { symbol in
            let asset = symbol.hasSuffix("USDT")
                ? Asset(symbol: symbol, name: symbol, assetClass: .crypto, quote: "USDT")
                : Asset(symbol: symbol, name: symbol, assetClass: .stock, quote: "USD")
            return try await AppServices.makeMarket(transport: transport).quote(for: asset).price
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

    func broker(for asset: Asset, settings: AppSettings) -> Broker {
        if settings.demoMode { return paperBroker }
        switch asset.assetClass {
        case .crypto:
            return BinanceBroker(apiKey: KeychainStore.get(.binanceKey), secret: KeychainStore.get(.binanceSecret),
                                 environment: settings.cryptoEnvironment)
        case .stock:
            return AlpacaBroker(keyId: KeychainStore.get(.alpacaKey), secret: KeychainStore.get(.alpacaSecret),
                                environment: settings.stockEnvironment)
        }
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

/// Journal local des ordres exécutés, pour le suivi et la limite de perte journalière.
@MainActor
@Observable
final class TradeJournal {
    struct Entry: Codable, Identifiable, Hashable {
        var id = UUID()
        let date: Date
        let symbol: String
        let side: OrderSide
        let quantity: Double
        let price: Double
        let simulated: Bool
        let environment: BrokerEnvironment
    }

    private static let storageKey = "altim.journal.v1"
    private(set) var entries: [Entry] = []

    init() {
        if let data = UserDefaults.standard.data(forKey: Self.storageKey),
           let saved = try? JSONDecoder().decode([Entry].self, from: data) {
            entries = saved
        }
    }

    func record(_ result: OrderResult, fallbackPrice: Double, environment: BrokerEnvironment) {
        let qty = NSDecimalNumber(decimal: result.executedQuantity).doubleValue
        guard qty > 0 else { return }
        let price = result.averagePrice.map { NSDecimalNumber(decimal: $0).doubleValue } ?? fallbackPrice
        entries.insert(Entry(date: Date(), symbol: result.symbol, side: result.side, quantity: qty, price: price,
                             simulated: result.isSimulated, environment: environment), at: 0)
        if let data = try? JSONEncoder().encode(entries) {
            UserDefaults.standard.set(data, forKey: Self.storageKey)
        }
    }

    /// P&L réalisé aujourd'hui (méthode du coût moyen), pour un environnement donné.
    func realizedPnLToday(environment: BrokerEnvironment, simulated: Bool) -> Double {
        var position: [String: (qty: Double, cost: Double)] = [:]
        var pnl = 0.0
        let calendar = Calendar.current
        for e in entries.reversed() where e.environment == environment && e.simulated == simulated {
            var p = position[e.symbol] ?? (0, 0)
            if e.side == .buy {
                p.qty += e.quantity
                p.cost += e.quantity * e.price
            } else if p.qty > 0 {
                let avg = p.cost / p.qty
                let sold = min(e.quantity, p.qty)
                if calendar.isDateInToday(e.date) { pnl += sold * (e.price - avg) }
                p.qty -= sold
                p.cost -= sold * avg
            }
            position[e.symbol] = p
        }
        return pnl
    }
}
