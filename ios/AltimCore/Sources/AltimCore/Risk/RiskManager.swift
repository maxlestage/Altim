import Foundation

/// Règles de gestion du risque appliquées avant tout ordre.
public struct RiskSettings: Codable, Hashable, Sendable {
    /// Part du capital risquée si le stop est touché (1 % recommandé).
    public var riskPerTradePercent: Double = 1
    /// Taille maximale d'une position en % du capital.
    public var maxPositionPercent: Double = 20
    /// Perte journalière maximale au-delà de laquelle tout nouvel achat est bloqué.
    public var dailyLossLimitPercent: Double = 3
    /// Ratio gain/risque minimum pour accepter un trade.
    public var minRiskReward: Double = 1.5
    /// Frais estimés par exécution (0,1 % Binance spot).
    public var feeRate: Double = 0.001

    public init() {}
}

public struct PositionSize: Hashable, Sendable {
    public let quantity: Double
    public let notional: Double
    /// Perte estimée si le stop est touché, frais inclus.
    public let riskAmount: Double
    public let percentOfEquity: Double
    /// `true` si la taille a été plafonnée par `maxPositionPercent`.
    public let capped: Bool
}

public struct RiskManager: Sendable {
    public var settings: RiskSettings

    public init(settings: RiskSettings = RiskSettings()) {
        self.settings = settings
    }

    /// Taille de position pour risquer `riskPerTradePercent` du capital jusqu'au stop.
    public func positionSize(equity: Double, plan: TradePlan) -> PositionSize? {
        guard equity > 0, plan.entry > 0, plan.riskPerUnit > 0 else { return nil }
        let riskBudget = equity * settings.riskPerTradePercent / 100
        // Risque par unité = distance au stop + frais d'entrée et de sortie.
        let perUnit = plan.riskPerUnit + plan.entry * settings.feeRate * 2
        var quantity = riskBudget / perUnit
        let maxNotional = equity * settings.maxPositionPercent / 100
        var capped = false
        if quantity * plan.entry > maxNotional {
            quantity = maxNotional / plan.entry
            capped = true
        }
        let notional = quantity * plan.entry
        return PositionSize(quantity: quantity, notional: notional, riskAmount: quantity * perUnit,
                            percentOfEquity: notional / equity * 100, capped: capped)
    }

    /// Vérifications bloquantes avant un achat. Renvoie la liste des problèmes (vide = autorisé).
    public func preTradeIssues(side: OrderSide, notional: Double, equity: Double,
                               plan: TradePlan?, realizedPnLToday: Double) -> [String] {
        var issues: [String] = []
        guard side == .buy else { return issues }
        if equity <= 0 { issues.append("Capital disponible nul.") }
        if equity > 0, notional > equity * settings.maxPositionPercent / 100 * 1.0001 {
            issues.append(String(format: "Position supérieure à %.0f %% du capital.", settings.maxPositionPercent))
        }
        if equity > 0, realizedPnLToday < 0, -realizedPnLToday >= equity * settings.dailyLossLimitPercent / 100 {
            issues.append(String(format: "Limite de perte journalière (%.1f %%) atteinte : trading suspendu jusqu'à demain.", settings.dailyLossLimitPercent))
        }
        if let plan {
            if plan.stopLoss >= plan.entry { issues.append("Le stop doit être sous le prix d'entrée.") }
            if plan.riskReward < settings.minRiskReward {
                issues.append(String(format: "Ratio gain/risque %.2f inférieur au minimum %.2f.", plan.riskReward, settings.minRiskReward))
            }
        } else {
            issues.append("Aucun stop défini : achat refusé.")
        }
        return issues
    }
}
