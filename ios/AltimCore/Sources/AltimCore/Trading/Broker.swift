import Foundation

public enum BrokerEnvironment: String, Codable, CaseIterable, Sendable {
    /// Argent fictif (testnet Binance / paper trading Alpaca).
    case test
    /// Argent réel.
    case live

    public var label: String { self == .live ? "RÉEL" : "TEST" }
}

/// Règles de négociation d'un symbole (pas de quantité, pas de prix, montant minimum).
public struct SymbolRules: Hashable, Sendable {
    public let symbol: String
    public let baseAsset: String
    public let quoteAsset: String
    public let stepSize: Decimal
    public let minQuantity: Decimal
    public let maxQuantity: Decimal
    public let tickSize: Decimal
    public let minNotional: Decimal
    public let isTradable: Bool

    public init(symbol: String, baseAsset: String, quoteAsset: String, stepSize: Decimal, minQuantity: Decimal,
                maxQuantity: Decimal, tickSize: Decimal, minNotional: Decimal, isTradable: Bool) {
        self.symbol = symbol
        self.baseAsset = baseAsset
        self.quoteAsset = quoteAsset
        self.stepSize = stepSize
        self.minQuantity = minQuantity
        self.maxQuantity = maxQuantity
        self.tickSize = tickSize
        self.minNotional = minNotional
        self.isTradable = isTradable
    }

    public func normalizeQuantity(_ q: Decimal) -> Decimal { q.floored(toStep: stepSize) }
    public func normalizePrice(_ p: Decimal) -> Decimal { p.rounded(toStep: tickSize) }

    /// Problèmes empêchant l'envoi de l'ordre (vide = conforme).
    public func issues(for order: OrderRequest, referencePrice: Decimal) -> [String] {
        var issues: [String] = []
        if !isTradable { issues.append("\(symbol) n'est pas négociable actuellement.") }
        if order.quantity <= 0 { issues.append("Quantité nulle après arrondi au pas de \(stepSize.plainString).") }
        if order.quantity < minQuantity { issues.append("Quantité minimale : \(minQuantity.plainString).") }
        if maxQuantity > 0, order.quantity > maxQuantity { issues.append("Quantité maximale : \(maxQuantity.plainString).") }
        if normalizeQuantity(order.quantity) != order.quantity {
            issues.append("La quantité doit être un multiple de \(stepSize.plainString).")
        }
        let price = order.limitPrice ?? referencePrice
        if order.quantity * price < minNotional {
            issues.append("Montant minimum : \(minNotional.plainString) \(quoteAsset).")
        }
        if order.type == .limit {
            if order.limitPrice == nil { issues.append("Prix limite manquant.") }
        }
        if order.side == .buy, let stop = order.stopLoss, stop >= price {
            issues.append("Le stop doit être inférieur au prix d'achat.")
        }
        if order.side == .buy, let tp = order.takeProfit, tp <= price {
            issues.append("L'objectif doit être supérieur au prix d'achat.")
        }
        return issues
    }
}

public enum BrokerError: Error, LocalizedError, Equatable {
    case rejected(code: Int, message: String)
    case validation([String])
    /// Network cut during submission and the order could not be found: its state is unknown.
    case unknownOrderState(clientOrderId: String)

    public var errorDescription: String? {
        switch self {
        case let .rejected(code, message): return BrokerError.explain(code: code, message: message)
        case let .validation(issues): return issues.joined(separator: "\n")
        case let .unknownOrderState(id):
            return "Connexion perdue pendant l'envoi : impossible de confirmer l'ordre \(id). Vérifiez vos ordres chez le courtier avant de réessayer."
        }
    }

    static func explain(code: Int, message: String) -> String {
        switch code {
        case -2010: return "Ordre refusé : solde insuffisant ou règle non respectée. (\(message))"
        case -2014, -2015: return "Clé API invalide, IP non autorisée ou permission « Spot trading » absente."
        case -1021: return "Horloge désynchronisée avec le serveur. Réessayez."
        case -1013: return "Ordre non conforme aux règles du marché (quantité/prix/montant). (\(message))"
        case -1121: return "Symbole inconnu sur ce marché."
        default: return "Ordre refusé (\(code)) : \(message)"
        }
    }
}

/// Courtier capable d'exécuter des ordres.
public protocol Broker: Sendable {
    var name: String { get }
    var environment: BrokerEnvironment { get }
    func balances() async throws -> [Balance]
    func rules(for symbol: String) async throws -> SymbolRules
    /// Current price on the broker (compared with the multi-source consensus before any order).
    func lastPrice(for symbol: String) async throws -> Decimal
    /// Valide l'ordre auprès du courtier sans l'exécuter.
    func test(_ order: OrderRequest) async throws
    func place(_ order: OrderRequest) async throws -> OrderResult
}
