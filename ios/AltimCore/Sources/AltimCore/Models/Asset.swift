import Foundation

public enum AssetClass: String, Codable, CaseIterable, Sendable {
    case crypto
    case stock

    public var label: String {
        switch self {
        case .crypto: return "Crypto"
        case .stock: return "Action"
        }
    }
}

/// Un actif suivi par l'application.
public struct Asset: Codable, Hashable, Identifiable, Sendable {
    /// Symbole côté fournisseur : `BTCUSDT` (Binance) ou `AAPL` (actions).
    public let symbol: String
    public let name: String
    public let assetClass: AssetClass
    /// Devise de cotation (USDT, USD, EUR…).
    public let quote: String

    public var id: String { "\(assetClass.rawValue):\(symbol)" }

    /// Symbole de base (BTC pour BTCUSDT).
    public var base: String {
        guard assetClass == .crypto, symbol.hasSuffix(quote) else { return symbol }
        return String(symbol.dropLast(quote.count))
    }

    public init(symbol: String, name: String, assetClass: AssetClass, quote: String) {
        self.symbol = symbol.uppercased()
        self.name = name
        self.assetClass = assetClass
        self.quote = quote.uppercased()
    }

    public static let defaults: [Asset] = [
        Asset(symbol: "BTCUSDT", name: "Bitcoin", assetClass: .crypto, quote: "USDT"),
        Asset(symbol: "ETHUSDT", name: "Ethereum", assetClass: .crypto, quote: "USDT"),
        Asset(symbol: "SOLUSDT", name: "Solana", assetClass: .crypto, quote: "USDT"),
        Asset(symbol: "BNBUSDT", name: "BNB", assetClass: .crypto, quote: "USDT"),
        Asset(symbol: "AAPL", name: "Apple", assetClass: .stock, quote: "USD"),
        Asset(symbol: "NVDA", name: "NVIDIA", assetClass: .stock, quote: "USD"),
        Asset(symbol: "MSFT", name: "Microsoft", assetClass: .stock, quote: "USD"),
        Asset(symbol: "TSLA", name: "Tesla", assetClass: .stock, quote: "USD"),
    ]
}

public enum Timeframe: String, Codable, CaseIterable, Sendable, Identifiable {
    case m15 = "15m"
    case h1 = "1h"
    case h4 = "4h"
    case d1 = "1d"

    public var id: String { rawValue }

    public var seconds: TimeInterval {
        switch self {
        case .m15: return 900
        case .h1: return 3_600
        case .h4: return 14_400
        case .d1: return 86_400
        }
    }

    /// Unité de temps supérieure utilisée pour confirmer la tendance.
    public var higher: Timeframe? {
        switch self {
        case .m15: return .h1
        case .h1: return .h4
        case .h4: return .d1
        case .d1: return nil
        }
    }

    public var label: String {
        switch self {
        case .m15: return "15 min"
        case .h1: return "1 h"
        case .h4: return "4 h"
        case .d1: return "1 j"
        }
    }
}
