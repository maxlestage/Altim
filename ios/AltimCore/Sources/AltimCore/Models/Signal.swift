import Foundation

public enum SignalAction: String, Codable, Sendable {
    case strongBuy
    case buy
    case hold
    case sell
    case strongSell

    public var label: String {
        switch self {
        case .strongBuy: return "ACHAT FORT"
        case .buy: return "ACHAT"
        case .hold: return "ATTENDRE"
        case .sell: return "VENTE"
        case .strongSell: return "VENTE FORTE"
        }
    }

    public var isBuy: Bool { self == .buy || self == .strongBuy }
    public var isSell: Bool { self == .sell || self == .strongSell }
}

/// Contribution d'un indicateur au score global, pour expliquer le signal.
public struct SignalFactor: Codable, Hashable, Sendable, Identifiable {
    public let name: String
    /// Score normalisé de -1 (baissier) à +1 (haussier).
    public let score: Double
    public let weight: Double
    public let detail: String

    public var id: String { name }

    public init(name: String, score: Double, weight: Double, detail: String) {
        self.name = name
        self.score = max(-1, min(1, score))
        self.weight = weight
        self.detail = detail
    }
}

public struct TradePlan: Codable, Hashable, Sendable {
    public let entry: Double
    public let stopLoss: Double
    public let takeProfit: Double

    public init(entry: Double, stopLoss: Double, takeProfit: Double) {
        self.entry = entry
        self.stopLoss = stopLoss
        self.takeProfit = takeProfit
    }

    public var riskPerUnit: Double { abs(entry - stopLoss) }
    public var rewardPerUnit: Double { abs(takeProfit - entry) }
    public var riskReward: Double { riskPerUnit > 0 ? rewardPerUnit / riskPerUnit : 0 }
}

public struct Signal: Codable, Hashable, Sendable {
    public let action: SignalAction
    /// Score global de -100 à +100.
    public let score: Double
    /// Confiance de 0 à 100 : force du score × accord entre indicateurs.
    public let confidence: Double
    public let price: Double
    public let time: Date
    public let factors: [SignalFactor]
    /// Plan de trade suggéré (long pour un achat, sortie pour une vente).
    public let plan: TradePlan?
    public let warnings: [String]

    public init(action: SignalAction, score: Double, confidence: Double, price: Double, time: Date,
                factors: [SignalFactor], plan: TradePlan?, warnings: [String]) {
        self.action = action
        self.score = score
        self.confidence = confidence
        self.price = price
        self.time = time
        self.factors = factors
        self.plan = plan
        self.warnings = warnings
    }
}
