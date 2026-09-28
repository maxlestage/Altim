/**
 * Decision tools that only compute (no advice added): comparison of assets over the same days, position size for a
 * chosen risk, and rebalancing towards a target allocation. Same computations on the iPhone and Android.
 */
import type { Close } from "./history";

const DAY = 86_400_000;
const dayOf = (t: number) => Math.floor(t / DAY) * DAY;

// ---------- Comparison ----------

export interface CompareStat {
  id: string;
  /** Change over the period, in %. */
  change: number;
  /** Annualised volatility of the daily returns, in % (365 days a year for crypto, 252 for stocks). */
  volatility: number;
  /** Deepest fall from a previous high, in % (≤ 0). */
  maxDrawdown: number;
  /** Change since the first day, one point per calendar day of the common period. */
  pct: number[];
}

export interface Comparison {
  stats: CompareStat[];
  /** Correlation of the daily returns, on the days both assets closed; null with fewer than 20 common days. */
  correlation: (number | null)[][];
  days: number[];
  missing: string[];
}

function returns(closes: Close[]): Map<number, number> {
  const out = new Map<number, number>();
  for (let i = 1; i < closes.length; i++) out.set(dayOf(closes[i]![0]), closes[i]![1] / closes[i - 1]![1] - 1);
  return out;
}

function correlation(a: Map<number, number>, b: Map<number, number>): number | null {
  const xs: number[] = [];
  const ys: number[] = [];
  for (const [d, x] of a) {
    const y = b.get(d);
    if (y !== undefined) {
      xs.push(x);
      ys.push(y);
    }
  }
  if (xs.length < 20) return null;
  const mx = xs.reduce((s, v) => s + v, 0) / xs.length;
  const my = ys.reduce((s, v) => s + v, 0) / ys.length;
  let sxy = 0;
  let sxx = 0;
  let syy = 0;
  for (let i = 0; i < xs.length; i++) {
    sxy += (xs[i]! - mx) * (ys[i]! - my);
    sxx += (xs[i]! - mx) ** 2;
    syy += (ys[i]! - my) ** 2;
  }
  return sxx > 0 && syy > 0 ? sxy / Math.sqrt(sxx * syy) : null;
}

export function compareAssets(series: Record<string, Close[]>, ids: string[], days: number, now = Date.now()): Comparison | null {
  const clean: Record<string, Close[]> = {};
  const missing: string[] = [];
  for (const id of ids) {
    const c = (series[id] ?? []).filter(([t, v]) => Number.isFinite(t) && v > 0 && t <= now).sort((a, b) => a[0] - b[0]);
    if (c.length < 2) missing.push(id);
    else clean[id] = c;
  }
  const kept = ids.filter((id) => clean[id]);
  if (!kept.length) return null;
  const end = dayOf(Math.max(...kept.map((id) => clean[id]!.at(-1)![0])));
  // Common period: from the latest first day among the assets (a younger asset shortens it) to the last close.
  const start = Math.max(end - days * DAY, ...kept.map((id) => dayOf(clean[id]![0]![0])));
  if (end - start < DAY) return null;
  const grid: number[] = [];
  for (let d = start; d <= end; d += DAY) grid.push(d);

  const inPeriod = (id: string) => clean[id]!.filter(([t]) => dayOf(t) >= start && dayOf(t) <= end);
  const stats: CompareStat[] = kept.map((id) => {
    const c = inPeriod(id);
    const all = clean[id]!;
    let i = 0;
    let last: number | null = null;
    const onGrid = grid.map((d) => {
      while (i < all.length && dayOf(all[i]![0]) <= d) last = all[i++]![1];
      return last!;
    });
    const base = onGrid[0]!;
    let peak = c[0]![1];
    let dd = 0;
    for (const [, v] of c) {
      peak = Math.max(peak, v);
      dd = Math.min(dd, (v / peak - 1) * 100);
    }
    const r = [...returns(c).values()];
    const mean = r.reduce((s, v) => s + v, 0) / (r.length || 1);
    const sd = Math.sqrt(r.reduce((s, v) => s + (v - mean) ** 2, 0) / Math.max(1, r.length - 1));
    const perYear = id.startsWith("crypto:") ? 365 : 252;
    return { id, change: (onGrid.at(-1)! / base - 1) * 100, volatility: sd * Math.sqrt(perYear) * 100, maxDrawdown: dd, pct: onGrid.map((v) => (v / base - 1) * 100) };
  });
  const rets = kept.map((id) => returns(inPeriod(id)));
  const corr = kept.map((_, a) => kept.map((__, b) => (a === b ? 1 : correlation(rets[a]!, rets[b]!))));
  return { stats, correlation: corr, days: grid, missing };
}

// ---------- Position size ----------

export interface PositionInput {
  /** Money available for the idea (capital or budget), in $. */
  capital: number;
  /** Share of the capital accepted as a loss if the stop is hit, in % (1 % is a common rule). */
  riskPct: number;
  entry: number;
  stop: number;
  target?: number | null;
}

export interface PositionSize {
  quantity: number;
  amount: number;
  /** Amount as a share of the capital, in %. */
  capitalShare: number;
  /** Loss if the stop is hit, in $. */
  risk: number;
  /** Distance to the stop, in % of the entry. */
  stopDistance: number;
  reward: number | null;
  /** Reward / risk ratio (R). */
  ratio: number | null;
  /** The size was cut to the capital (the stop is too close for the risk chosen). */
  capped: boolean;
}

/** Quantity such that hitting the stop costs riskPct % of the capital, never more than the capital itself. */
export function positionSize(p: PositionInput): PositionSize | null {
  if (!(p.capital > 0) || !(p.riskPct > 0) || !(p.entry > 0) || !(p.stop > 0) || p.stop >= p.entry) return null;
  const perUnit = p.entry - p.stop;
  let quantity = (p.capital * p.riskPct) / 100 / perUnit;
  let capped = false;
  if (quantity * p.entry > p.capital) {
    quantity = p.capital / p.entry;
    capped = true;
  }
  const risk = quantity * perUnit;
  const reward = p.target && p.target > p.entry ? quantity * (p.target - p.entry) : null;
  return {
    quantity,
    amount: quantity * p.entry,
    capitalShare: ((quantity * p.entry) / p.capital) * 100,
    risk,
    stopDistance: (perUnit / p.entry) * 100,
    reward,
    ratio: reward != null ? reward / risk : null,
    capped,
  };
}

// ---------- Sale after fees and tax ----------

export interface SaleLine {
  id: string;
  value: number;
  /** Purchase cost (quantity × average price), null when unknown. */
  cost: number | null;
}

export interface SaleResult {
  gross: number;
  fees: number;
  /** Gain after fees (negative: loss); null when a cost is unknown. */
  gain: number | null;
  tax: number;
  net: number;
}

/**
 * What a sale leaves once the fees and the tax on the gain are paid (France: flat tax "PFU" of 30 % = 12,8 % income
 * tax + 17,2 % social contributions). A loss pays no tax. Line by line: for cryptos, France computes the gain on the
 * whole portfolio at each sale (art. 150 VH bis), so this is an approximation, said as such on screen.
 */
export function saleAfterTax(line: SaleLine, taxPct = 30, feePct = 0.1): SaleResult {
  const fees = Math.max(0, line.value) * Math.max(0, feePct) / 100;
  const gain = line.cost == null ? null : line.value - fees - line.cost;
  const tax = gain != null && gain > 0 ? (gain * Math.max(0, taxPct)) / 100 : 0;
  return { gross: line.value, fees, gain, tax, net: line.value - fees - tax };
}

/** All lines sold: totals; gains and losses of the same year offset each other before the tax (France). */
export function saleTotal(lines: SaleLine[], taxPct = 30, feePct = 0.1): SaleResult & { lines: (SaleResult & { id: string })[]; unknownCost: number } {
  const each = lines.map((l) => ({ id: l.id, ...saleAfterTax(l, taxPct, feePct) }));
  const known = each.filter((l) => l.gain != null);
  const gain = known.length ? known.reduce((s, l) => s + l.gain!, 0) : null;
  const gross = each.reduce((s, l) => s + l.gross, 0);
  const fees = each.reduce((s, l) => s + l.fees, 0);
  const tax = gain != null && gain > 0 ? (gain * Math.max(0, taxPct)) / 100 : 0;
  return { gross, fees, gain, tax, net: gross - fees - tax, lines: each, unknownCost: each.length - known.length };
}

// ---------- Projection ----------

/** Yearly returns shown side by side: hypotheses to compare, not forecasts. */
export const PROJECTION_RATES = [0, 4, 8] as const;

/**
 * Value after `years` of a starting amount plus a contribution at the end of each month, compounded monthly at a
 * yearly rate. One point per year (year 0 = today).
 */
export function projection(start: number, monthly: number, years: number, ratePct: number): { year: number; value: number; paid: number }[] {
  const r = Math.pow(1 + ratePct / 100, 1 / 12) - 1;
  const out = [{ year: 0, value: Math.max(0, start), paid: Math.max(0, start) }];
  let v = Math.max(0, start);
  let paid = v;
  for (let m = 1; m <= Math.round(years) * 12; m++) {
    v = v * (1 + r) + Math.max(0, monthly);
    paid += Math.max(0, monthly);
    if (m % 12 === 0) out.push({ year: m / 12, value: v, paid });
  }
  return out;
}

// ---------- Rebalancing ----------

export type AssetClass = "crypto" | "stock" | "cash";

export interface Rebalance {
  total: number;
  current: Record<AssetClass, number>;
  /** Amount to buy (+) or sell (−) per class, in $. */
  moves: Record<AssetClass, number>;
  /** Split of each class's move over its lines, pro rata of their value. */
  lines: { id: string; amount: number }[];
}

/** Buys and sells that bring the portfolio to the target split (in %, summing to 100). */
export function rebalance(lines: { id: string; kind: "crypto" | "stock"; value: number }[], cash: number, target: Record<AssetClass, number>): Rebalance | null {
  const sum = target.crypto + target.stock + target.cash;
  if (Math.abs(sum - 100) > 0.01 || [target.crypto, target.stock, target.cash].some((v) => v < 0)) return null;
  const by: Record<AssetClass, number> = { crypto: 0, stock: 0, cash: Math.max(0, cash) };
  for (const l of lines) if (l.value > 0) by[l.kind] += l.value;
  const total = by.crypto + by.stock + by.cash;
  if (!(total > 0)) return null;
  const moves = {} as Record<AssetClass, number>;
  const current = {} as Record<AssetClass, number>;
  for (const k of ["crypto", "stock", "cash"] as AssetClass[]) {
    current[k] = (by[k] / total) * 100;
    moves[k] = (total * target[k]) / 100 - by[k];
  }
  const split = lines
    .filter((l) => l.value > 0 && by[l.kind] > 0)
    .map((l) => ({ id: l.id, amount: (moves[l.kind] * l.value) / by[l.kind] }));
  return { total, current, moves, lines: split };
}
