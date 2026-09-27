import Foundation

public enum OrderSide: String, Codable, Sendable {
    case buy = "BUY"
    case sell = "SELL"

    public var label: String { self == .buy ? "Achat" : "Vente" }
}

public enum OrderType: String, Codable, Sendable, CaseIterable {
    case market = "MARKET"
    case limit = "LIMIT"

    public var label: String { self == .market ? "Au marché" : "Limite" }
}

/// Demande d'ordre indépendante du courtier.
public struct OrderRequest: Codable, Hashable, Sendable {
    public let symbol: String
    public let side: OrderSide
    public let type: OrderType
    /// Quantité de l'actif de base.
    public let quantity: Decimal
    /// Prix limite (obligatoire pour un ordre limite).
    public let limitPrice: Decimal?
    /// Stop et objectif de protection (ordre OCO / bracket) après un achat.
    public let stopLoss: Decimal?
    public let takeProfit: Decimal?

    public init(symbol: String, side: OrderSide, type: OrderType, quantity: Decimal,
                limitPrice: Decimal? = nil, stopLoss: Decimal? = nil, takeProfit: Decimal? = nil) {
        self.symbol = symbol
        self.side = side
        self.type = type
        self.quantity = quantity
        self.limitPrice = limitPrice
        self.stopLoss = stopLoss
        self.takeProfit = takeProfit
    }
}

public struct OrderResult: Codable, Hashable, Sendable {
    public let orderId: String
    public let symbol: String
    public let side: OrderSide
    public let status: String
    public let executedQuantity: Decimal
    public let averagePrice: Decimal?
    public let protectionOrderId: String?
    public let isSimulated: Bool
    /// Informations importantes à afficher (ex. : protection non posée).
    public let notes: [String]

    public init(orderId: String, symbol: String, side: OrderSide, status: String, executedQuantity: Decimal,
                averagePrice: Decimal?, protectionOrderId: String?, isSimulated: Bool, notes: [String] = []) {
        self.orderId = orderId
        self.symbol = symbol
        self.side = side
        self.status = status
        self.executedQuantity = executedQuantity
        self.averagePrice = averagePrice
        self.protectionOrderId = protectionOrderId
        self.isSimulated = isSimulated
        self.notes = notes
    }
}

public struct Balance: Codable, Hashable, Sendable, Identifiable {
    public let asset: String
    public let free: Decimal
    public let locked: Decimal

    public var id: String { asset }
    public var total: Decimal { free + locked }

    public init(asset: String, free: Decimal, locked: Decimal) {
        self.asset = asset
        self.free = free
        self.locked = locked
    }
}
