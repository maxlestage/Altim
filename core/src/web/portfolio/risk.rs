//! Position sizing and pre-trade checks (`web/src/engine/risk.ts`). The settings are `web::store::RiskSettings`
//! (Réglages → Prudence des conseils).
use serde::{Deserialize, Serialize};

use crate::js::to_fixed;
pub use crate::web::store::{DEFAULT_RISK, RiskSettings};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub entry: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskPositionSize {
    pub quantity: f64,
    pub notional: f64,
    pub risk_amount: f64,
    pub percent_of_equity: f64,
    pub capped: bool,
}

pub fn risk_reward(p: &Plan) -> f64 {
    let risk = (p.entry - p.stop_loss).abs();
    if risk > 0.0 { (p.take_profit - p.entry).abs() / risk } else { 0.0 }
}

/// Quantity so that the stop (fees included, both ways) costs the risk per idea, capped at the maximum position.
pub fn position_size(s: &RiskSettings, equity: f64, plan: &Plan) -> Option<RiskPositionSize> {
    let risk_per_unit = (plan.entry - plan.stop_loss).abs();
    if equity <= 0.0 || plan.entry <= 0.0 || risk_per_unit <= 0.0 {
        return None;
    }
    let budget = equity * s.risk_per_trade_percent / 100.0;
    let per_unit = risk_per_unit + plan.entry * s.fee_rate * 2.0;
    let mut quantity = budget / per_unit;
    let max_notional = equity * s.max_position_percent / 100.0;
    let mut capped = false;
    if quantity * plan.entry > max_notional {
        quantity = max_notional / plan.entry;
        capped = true;
    }
    let notional = quantity * plan.entry;
    Some(RiskPositionSize { quantity, notional, risk_amount: quantity * per_unit, percent_of_equity: notional / equity * 100.0, capped })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

/// Reasons to refuse a buy (none for a sale): position size, daily loss limit, stop, reward / risk.
pub fn pre_trade_issues(s: &RiskSettings, side: Side, notional: f64, equity: f64, plan: Option<&Plan>, pnl_today: f64) -> Vec<String> {
    let mut issues = Vec::new();
    if side != Side::Buy {
        return issues;
    }
    if equity <= 0.0 {
        issues.push("Capital disponible nul.".to_string());
    }
    if equity > 0.0 && notional > equity * s.max_position_percent / 100.0 * 1.0001 {
        issues.push(format!("Position supérieure à {} % du capital.", to_fixed(s.max_position_percent, 0)));
    }
    if equity > 0.0 && pnl_today < 0.0 && -pnl_today >= equity * s.daily_loss_limit_percent / 100.0 {
        issues
            .push(format!("Limite de perte journalière ({} %) atteinte : trading suspendu jusqu'à demain.", to_fixed(s.daily_loss_limit_percent, 1)));
    }
    match plan {
        Some(plan) => {
            if plan.stop_loss >= plan.entry {
                issues.push("Le stop doit être sous le prix d'entrée.".to_string());
            }
            let rr = risk_reward(plan);
            if rr < s.min_risk_reward {
                issues.push(format!("Ratio gain/risque {} inférieur au minimum {}.", to_fixed(rr, 2), to_fixed(s.min_risk_reward, 2)));
            }
        }
        None => issues.push("Aucun stop défini : achat refusé.".to_string()),
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizing_and_checks() {
        let plan = Plan { entry: 100.0, stop_loss: 95.0, take_profit: 110.0 };
        assert_eq!(risk_reward(&plan), 2.0);
        // 1 % of 10 000 $ = 100 $ over 5 $ + 0,2 $ of fees per unit: 19,23 units, 1 923 $ (under the 20 % cap).
        let p = position_size(&DEFAULT_RISK, 10_000.0, &plan).unwrap();
        assert!((p.quantity - 100.0 / 5.2).abs() < 1e-9 && !p.capped);
        assert!((p.risk_amount - 100.0).abs() < 1e-9);
        // A very close stop is capped at 20 % of the capital.
        let p = position_size(&DEFAULT_RISK, 10_000.0, &Plan { entry: 100.0, stop_loss: 99.9, take_profit: 101.0 }).unwrap();
        assert!(p.capped && (p.notional - 2000.0).abs() < 1e-9);
        assert_eq!(position_size(&DEFAULT_RISK, 0.0, &plan), None);
        assert!(pre_trade_issues(&DEFAULT_RISK, Side::Sell, 1e9, 0.0, None, 0.0).is_empty());
        assert_eq!(
            pre_trade_issues(&DEFAULT_RISK, Side::Buy, 3000.0, 10_000.0, Some(&Plan { entry: 100.0, stop_loss: 99.0, take_profit: 100.5 }), -400.0),
            vec![
                "Position supérieure à 20 % du capital.",
                "Limite de perte journalière (3.0 %) atteinte : trading suspendu jusqu'à demain.",
                "Ratio gain/risque 0.50 inférieur au minimum 1.50.",
            ]
        );
        assert_eq!(pre_trade_issues(&DEFAULT_RISK, Side::Buy, 100.0, 10_000.0, None, 0.0), vec!["Aucun stop défini : achat refusé."]);
    }
}
