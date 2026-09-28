/** Position sizing and pre-trade checks. */
export interface RiskSettings {
  riskPerTradePercent: number;
  maxPositionPercent: number;
  dailyLossLimitPercent: number;
  minRiskReward: number;
  feeRate: number;
  /** Cap of the crypto share of the portfolio (%, cash included). */
  maxCryptoPercent: number;
}

export const DEFAULT_RISK: RiskSettings = {
  riskPerTradePercent: 1,
  maxPositionPercent: 20,
  dailyLossLimitPercent: 3,
  minRiskReward: 1.5,
  feeRate: 0.001,
  maxCryptoPercent: 60,
};

export interface Plan { entry: number; stopLoss: number; takeProfit: number }
export interface PositionSize { quantity: number; notional: number; riskAmount: number; percentOfEquity: number; capped: boolean }

export const riskReward = (p: Plan) => {
  const risk = Math.abs(p.entry - p.stopLoss);
  return risk > 0 ? Math.abs(p.takeProfit - p.entry) / risk : 0;
};

export function positionSize(s: RiskSettings, equity: number, plan: Plan): PositionSize | null {
  const riskPerUnit = Math.abs(plan.entry - plan.stopLoss);
  if (equity <= 0 || plan.entry <= 0 || riskPerUnit <= 0) return null;
  const budget = (equity * s.riskPerTradePercent) / 100;
  const perUnit = riskPerUnit + plan.entry * s.feeRate * 2;
  let quantity = budget / perUnit;
  const maxNotional = (equity * s.maxPositionPercent) / 100;
  let capped = false;
  if (quantity * plan.entry > maxNotional) {
    quantity = maxNotional / plan.entry;
    capped = true;
  }
  const notional = quantity * plan.entry;
  return { quantity, notional, riskAmount: quantity * perUnit, percentOfEquity: (notional / equity) * 100, capped };
}

export function preTradeIssues(s: RiskSettings, side: "buy" | "sell", notional: number, equity: number, plan: Plan | null, pnlToday: number): string[] {
  const issues: string[] = [];
  if (side !== "buy") return issues;
  if (equity <= 0) issues.push("Capital disponible nul.");
  if (equity > 0 && notional > ((equity * s.maxPositionPercent) / 100) * 1.0001) {
    issues.push(`Position supérieure à ${s.maxPositionPercent.toFixed(0)} % du capital.`);
  }
  if (equity > 0 && pnlToday < 0 && -pnlToday >= (equity * s.dailyLossLimitPercent) / 100) {
    issues.push(`Limite de perte journalière (${s.dailyLossLimitPercent.toFixed(1)} %) atteinte : trading suspendu jusqu'à demain.`);
  }
  if (plan) {
    if (plan.stopLoss >= plan.entry) issues.push("Le stop doit être sous le prix d'entrée.");
    const rr = riskReward(plan);
    if (rr < s.minRiskReward) issues.push(`Ratio gain/risque ${rr.toFixed(2)} inférieur au minimum ${s.minRiskReward.toFixed(2)}.`);
  } else {
    issues.push("Aucun stop défini : achat refusé.");
  }
  return issues;
}
