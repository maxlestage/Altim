/**
 * Risk of the portfolio the user holds, beyond `holdings.ts`: market stress scenarios through each line's beta,
 * checks against the user's risk settings (weight per line, crypto share, correlated clusters, daily loss) and the
 * positions that became dangerous (stop broken or close, loss beyond the risk accepted per idea).
 * Pure, deterministic functions (same inputs, same results).
 */
import { alignedReturns, correlation, type PortfolioAnalysis } from "./holdings";
import type { Kind } from "./reliability";
import type { RiskSettings } from "./risk";
import { atr, type Candle } from "./signal";

// ---------- Beta ----------

/** Window of the beta (same as the correlations of `analyzePortfolio`) and minimum of shared daily returns. */
export const BETA_DAYS = 90;
export const MIN_BETA_DAYS = 30;
/** "Et si… ?": one year of shared sessions, steadier than 90 days for a shock on one market. */
export const WHATIF_BETA_DAYS = 250;
/** Below this |correlation| the beta explains little of the line's moves: said next to the line. */
export const WEAK_CORRELATION = 0.3;
/** Benchmark of each asset class: Bitcoin for cryptos, the S&P 500 (via the SPY ETF) for stocks. */
export const BENCHMARK: Record<Kind, { symbol: string; kind: Kind; label: string }> = {
  crypto: { symbol: "BTC", kind: "crypto", label: "Bitcoin" },
  stock: { symbol: "SPY", kind: "stock", label: "S&P 500" },
};

export interface BetaEstimate {
  beta: number;
  /** Shared daily returns used; 0 with the fallback. */
  days: number;
  /** false: history too short or missing, beta 1 assumed. */
  estimated: boolean;
  /** The asset is the benchmark itself (beta 1 by definition). */
  reference?: boolean;
}

/** Beta of an asset to its benchmark = cov(asset, benchmark) ÷ var(benchmark) on the aligned daily returns. */
export function estimateBeta(asset: Candle[], benchmark: Candle[], days = BETA_DAYS): BetaEstimate {
  const fallback = { beta: 1, days: 0, estimated: false };
  if (asset.length < 2 || benchmark.length < 2) return fallback;
  const [a, b] = alignedReturns([asset, benchmark], days) as [number[], number[]];
  const n = Math.min(a.length, b.length);
  if (n < MIN_BETA_DAYS) return fallback;
  const x = a.slice(-n);
  const y = b.slice(-n);
  const mx = x.reduce((s, v) => s + v, 0) / n;
  const my = y.reduce((s, v) => s + v, 0) / n;
  let cov = 0;
  let vary = 0;
  for (let i = 0; i < n; i++) {
    cov += (x[i]! - mx) * (y[i]! - my);
    vary += (y[i]! - my) ** 2;
  }
  return vary > 0 ? { beta: cov / vary, days: n, estimated: true } : fallback;
}

// ---------- Stress scenarios ----------

/** Shock of each benchmark, % (negative = fall). */
export interface StressScenario { key: string; label: string; crypto: number; stock: number }

export const STRESS_SCENARIOS: StressScenario[] = [
  { key: "all5", label: "Marchés −5 %", crypto: -5, stock: -5 },
  { key: "all10", label: "Marchés −10 %", crypto: -10, stock: -10 },
  { key: "all20", label: "Marchés −20 %", crypto: -20, stock: -20 },
  { key: "all30", label: "Marchés −30 %", crypto: -30, stock: -30 },
  { key: "crypto20", label: "Crypto −20 %, actions stables", crypto: -20, stock: 0 },
  { key: "stock10crypto30", label: "Actions −10 %, crypto −30 %", crypto: -30, stock: -10 },
];

export interface StressResult {
  scenario: StressScenario;
  /** Amount lost (USD, positive = loss; negative when the scenario would gain). */
  loss: number;
  /** Of the whole portfolio (cash included). */
  lossPercent: number;
  /** Of the invested part only (without cash): the difference is what the cash cushions. */
  investedLossPercent: number;
  worst: { symbol: string; name: string; loss: number; movePercent: number } | null;
}

/**
 * Each line moves by beta × the shock of its benchmark (never below −100 %); cash does not move. Lines of the same
 * asset share its beta (key `kind:symbol`, beta 1 when missing).
 */
export function stressTest(a: PortfolioAnalysis, betas: Record<string, BetaEstimate | undefined>, scenarios = STRESS_SCENARIOS): StressResult[] {
  return scenarios.map((scenario) => {
    const byAsset = new Map<string, { symbol: string; name: string; loss: number; movePercent: number }>();
    for (const l of a.lines) {
      const k = `${l.kind}:${l.symbol}`;
      const beta = betas[k]?.beta ?? 1;
      const move = Math.max(-100, beta * (l.kind === "crypto" ? scenario.crypto : scenario.stock));
      const prev = byAsset.get(k);
      const loss = (-l.value * move) / 100;
      byAsset.set(k, { symbol: l.symbol, name: l.name, loss: (prev?.loss ?? 0) + loss, movePercent: move });
    }
    const loss = [...byAsset.values()].reduce((s, x) => s + x.loss, 0);
    const worst = [...byAsset.values()].reduce<StressResult["worst"]>((w, x) => (x.loss > 0 && (!w || x.loss > w.loss) ? x : w), null);
    return {
      scenario,
      loss,
      lossPercent: a.total > 0 ? (loss / a.total) * 100 : 0,
      investedLossPercent: a.marketValue > 0 ? (loss / a.marketValue) * 100 : 0,
      worst,
    };
  });
}

// ---------- Correlated clusters ----------

export const CLUSTER_CORRELATION = 0.7;

export interface Cluster { symbols: string[]; averageCorrelation: number; weight: number; days: number }

/**
 * Groups of assets linked by a correlation above 0.7 (connected by pairs), kept when their average pairwise
 * correlation stays above 0.7. `series`: one entry per asset (weight in % of the portfolio, daily candles).
 */
export function correlatedClusters(series: { symbol: string; weight: number; daily: Candle[] }[], days = BETA_DAYS): Cluster[] {
  const usable = series.filter((s) => s.daily.length >= 20);
  if (usable.length < 2) return [];
  const returns = alignedReturns(usable.map((s) => s.daily), days);
  const n = usable.length;
  const corr: (number | null)[][] = usable.map(() => new Array(n).fill(null));
  for (let i = 0; i < n; i++)
    for (let j = i + 1; j < n; j++) corr[i]![j] = corr[j]![i] = correlation(returns[i]!, returns[j]!);
  const seen = new Set<number>();
  const clusters: Cluster[] = [];
  for (let s = 0; s < n; s++) {
    if (seen.has(s)) continue;
    const group = [s];
    seen.add(s);
    for (let k = 0; k < group.length; k++)
      for (let j = 0; j < n; j++)
        if (!seen.has(j) && (corr[group[k]!]![j] ?? -1) > CLUSTER_CORRELATION) {
          seen.add(j);
          group.push(j);
        }
    if (group.length < 2) continue;
    const pairs: number[] = [];
    for (let x = 0; x < group.length; x++)
      for (let y = x + 1; y < group.length; y++) {
        const c = corr[group[x]!]![group[y]!];
        if (c != null) pairs.push(c);
      }
    const averageCorrelation = pairs.reduce((a, b) => a + b, 0) / pairs.length;
    if (averageCorrelation <= CLUSTER_CORRELATION) continue;
    clusters.push({
      symbols: group.map((g) => usable[g]!.symbol),
      averageCorrelation,
      weight: group.reduce((a, g) => a + usable[g]!.weight, 0),
      days: Math.min(...returns.map((r) => r.length)),
    });
  }
  return clusters.sort((a, b) => b.weight - a.weight);
}

// ---------- Today's change ----------

const utcDay = (ms: number) => new Date(ms).toISOString().slice(0, 10);

/**
 * Previous close: the close of the last daily candle before today's (UTC calendar day; for stocks, the last
 * session). null without candles.
 */
export function previousClose(daily: Candle[], now: number): number | null {
  if (!daily.length) return null;
  const sorted = [...daily].sort((a, b) => a.time - b.time);
  const last = sorted[sorted.length - 1]!;
  if (utcDay(last.time) === utcDay(now)) return sorted.length > 1 ? sorted[sorted.length - 2]!.close : null;
  return last.close;
}

export interface DailyChange {
  /** USD, negative = loss today. */
  change: number;
  /** Of the portfolio value at the previous close (cash included). */
  percent: number;
  /** Lines with a live price and a previous close / all lines. */
  covered: number;
  lines: number;
}

/** Portfolio change since the previous close, from the current prices of the analysis. */
export function dailyChange(a: PortfolioAnalysis, daily: Record<string, Candle[] | undefined>, now: number): DailyChange | null {
  let change = 0;
  let covered = 0;
  for (const l of a.lines) {
    const prev = previousClose(daily[`${l.kind}:${l.symbol}`] ?? [], now);
    if (l.price == null || prev == null || prev <= 0) continue;
    change += l.quantity * (l.price - prev);
    covered++;
  }
  if (!covered) return null;
  const before = a.total - change;
  return { change, percent: before > 0 ? (change / before) * 100 : 0, covered, lines: a.lines.length };
}

// ---------- Limits of the user's settings ----------

export type LimitLevel = "danger" | "warning" | "ok" | "na";
export interface LimitCheck { code: string; level: LimitLevel; label: string; detail: string }

export const DAILY_LOSS_REACHED = "Limite de perte du jour atteinte : n'ouvrez plus de position aujourd'hui.";

const fr = (v: number, d = 1) => v.toLocaleString("fr-FR", { maximumFractionDigits: d });
const usd0 = (v: number) => `${fr(Math.abs(v), 0)} $`;

/** Loss (USD) if the stop of the line is hit: the user's stop when set, else the protective stop of the analysis. */
function lossAtStop(l: PortfolioAnalysis["lines"][number], userStop: number | undefined): number | null {
  const stop = userStop && userStop > 0 ? userStop : l.stop;
  if (l.price == null || stop == null) return null;
  return Math.max(0, l.quantity * (l.price - stop));
}

/**
 * Checks of the portfolio against the settings: loss at the stop per asset vs risk per idea, weight per asset vs
 * maximum, crypto share vs cap, correlated clusters above twice the maximum weight, today's loss vs the daily limit.
 */
export function checkLimits(
  a: PortfolioAnalysis,
  s: RiskSettings,
  opts: { clusters: Cluster[]; daily: DailyChange | null; stops?: Record<string, number | undefined> },
): LimitCheck[] {
  const out: LimitCheck[] = [];
  if (!a.lines.length || a.total <= 0) return out;
  const assets = new Map<string, { symbol: string; weight: number; loss: number | null }>();
  for (const l of a.lines) {
    const k = `${l.kind}:${l.symbol}`;
    const loss = lossAtStop(l, opts.stops?.[l.id]);
    const p = assets.get(k);
    assets.set(k, { symbol: l.symbol, weight: (p?.weight ?? 0) + l.weight, loss: p?.loss == null && loss == null ? null : (p?.loss ?? 0) + (loss ?? 0) });
  }

  const budget = (a.total * s.riskPerTradePercent) / 100;
  const risky = [...assets.values()].filter((x) => x.loss != null && x.loss > budget * 1.0001);
  out.push(risky.length
    ? { code: "risk_per_trade", level: "warning", label: `Risque par ligne (max ${fr(s.riskPerTradePercent, 2)} %)`,
        detail: `Si le stop était touché, ${risky.map((x) => `${x.symbol} coûterait ${usd0(x.loss!)} (${fr((x.loss! / a.total) * 100)} %)`).join(", ")} : plus que votre risque accepté par idée (${usd0(budget)}). Réduisez la ligne ou rapprochez le stop.` }
    : { code: "risk_per_trade", level: "ok", label: `Risque par ligne (max ${fr(s.riskPerTradePercent, 2)} %)`, detail: `Aucune ligne ne perdrait plus de ${usd0(budget)} à son stop.` });

  const heavy = [...assets.values()].filter((x) => x.weight > s.maxPositionPercent);
  out.push(heavy.length && assets.size > 1
    ? { code: "max_weight", level: "danger", label: `Poids max d'une ligne (${fr(s.maxPositionPercent, 0)} %)`,
        detail: `${heavy.map((x) => `${x.symbol} ${fr(x.weight)} %`).join(", ")} : au-dessus de votre maximum. Surexposition à un seul actif.` }
    : { code: "max_weight", level: "ok", label: `Poids max d'une ligne (${fr(s.maxPositionPercent, 0)} %)`, detail: "Aucune ligne au-dessus de votre maximum." });

  const crypto = a.allocation.crypto;
  out.push(crypto > s.maxCryptoPercent
    ? { code: "crypto_cap", level: "warning", label: `Part crypto (max ${fr(s.maxCryptoPercent, 0)} %)`,
        detail: `${fr(crypto)} % du patrimoine en crypto, au-dessus de votre plafond : un repli des cryptos toucherait tout le portefeuille.` }
    : { code: "crypto_cap", level: "ok", label: `Part crypto (max ${fr(s.maxCryptoPercent, 0)} %)`, detail: `${fr(crypto)} % du patrimoine en crypto.` });

  const clusterMax = Math.min(100, s.maxPositionPercent * 2);
  const over = opts.clusters.filter((c) => c.weight > clusterMax);
  out.push(over.length
    ? { code: "cluster", level: "warning", label: `Actifs corrélés (groupe max ${fr(clusterMax, 0)} %)`,
        detail: over.map((c) => `${c.symbols.join(" + ")} : ${fr(c.weight)} % du patrimoine, corrélation moyenne ${c.averageCorrelation.toFixed(2)} sur ${c.days} jours`).join(" ; ") +
          ". Ils se comportent comme une seule grosse ligne." }
    : { code: "cluster", level: "ok", label: `Actifs corrélés (groupe max ${fr(clusterMax, 0)} %)`,
        detail: opts.clusters.length ? `Groupe(s) corrélé(s) sous le seuil : ${opts.clusters.map((c) => `${c.symbols.join(" + ")} ${fr(c.weight)} %`).join(" ; ")}.` : "Aucun groupe d'actifs corrélés à plus de 0,7." });

  const d = opts.daily;
  const label = `Perte du jour (max ${fr(s.dailyLossLimitPercent, 2)} %)`;
  if (!d) out.push({ code: "daily_loss", level: "na", label, detail: "Clôture de la veille indisponible : variation du jour inconnue." });
  else {
    const partial = d.covered < d.lines ? ` (${d.covered} ligne(s) sur ${d.lines} mesurées)` : "";
    const text = `${d.change >= 0 ? "+" : "−"}${usd0(d.change)} (${d.percent >= 0 ? "+" : "−"}${fr(Math.abs(d.percent), 2)} %) depuis la clôture de la veille${partial}.`;
    out.push(d.percent <= -s.dailyLossLimitPercent
      ? { code: "daily_loss", level: "danger", label, detail: `${DAILY_LOSS_REACHED} ${text}` }
      : { code: "daily_loss", level: "ok", label, detail: text });
  }
  return out;
}

// ---------- Positions that became dangerous ----------

export type DangerCode = "stop_broken" | "near_stop" | "loss_over_risk";
export interface Danger { id: string; symbol: string; kind: Kind; name: string; reasons: { code: DangerCode; text: string }[] }

/**
 * A line is dangerous when its price broke the user's stop, is within one daily ATR (14) of it, or when its
 * unrealised loss exceeds the risk accepted per idea (% of the portfolio).
 */
export function dangerousPositions(a: PortfolioAnalysis, s: RiskSettings, daily: Record<string, Candle[] | undefined>, stops: Record<string, number | undefined>): Danger[] {
  const out: Danger[] = [];
  const budget = (a.total * s.riskPerTradePercent) / 100;
  for (const l of a.lines) {
    const reasons: Danger["reasons"] = [];
    const stop = stops[l.id];
    const candles = daily[`${l.kind}:${l.symbol}`] ?? [];
    const range = candles.length ? atr(candles)[candles.length - 1] ?? null : null;
    if (l.price != null && stop && stop > 0) {
      if (l.price <= stop) reasons.push({ code: "stop_broken", text: `Stop cassé : cours ${fr(l.price, 4)} $ sous votre stop ${fr(stop, 4)} $.` });
      else if (range && l.price - stop <= range)
        reasons.push({ code: "near_stop", text: `À moins d'une volatilité journalière (ATR ${fr(range, 4)} $) de votre stop ${fr(stop, 4)} $.` });
    }
    const loss = l.invested - l.value;
    if (l.price != null && loss > budget && budget > 0)
      reasons.push({ code: "loss_over_risk", text: `Perte latente de ${usd0(loss)} (${fr((loss / a.total) * 100)} % du patrimoine), au-delà de votre risque accepté par idée (${fr(s.riskPerTradePercent, 2)} %).` });
    if (reasons.length) out.push({ id: l.id, symbol: l.symbol, kind: l.kind, name: l.name, reasons });
  }
  return out;
}

// ---------- "Et si… ?": a shock on one market factor ----------

export type FactorKey = "qqq" | "spy" | "btc";
/** Factors of the simulator: the Nasdaq-100 (via the QQQ ETF), the S&P 500 (via SPY) and Bitcoin. */
export const FACTORS: Record<FactorKey, { symbol: string; kind: Kind; label: string; name: string }> = {
  qqq: { symbol: "QQQ", kind: "stock", label: "Nasdaq-100 (QQQ)", name: "le Nasdaq-100" },
  spy: { symbol: "SPY", kind: "stock", label: "S&P 500 (SPY)", name: "le S&P 500" },
  btc: { symbol: "BTC", kind: "crypto", label: "Bitcoin", name: "le Bitcoin" },
};
export const FACTOR_SHOCKS = [-5, -10, -20, -30, -50];

export interface FactorBeta extends BetaEstimate {
  /** Correlation of the daily returns with the factor (null: too few days). */
  correlation: number | null;
}

const dayKey = (ms: number) => new Date(ms).toISOString().slice(0, 10);

/**
 * Beta of an asset to a factor on the days where BOTH have a daily close (no forward fill: a crypto's weekends do
 * not become days where a stock "did not move"), returns between consecutive shared days, last `days` of them.
 * Not estimated (estimated: false, beta 1 only as a placeholder) under MIN_BETA_DAYS shared returns.
 */
export function factorBeta(asset: Candle[], factor: Candle[], days = BETA_DAYS): FactorBeta {
  const none: FactorBeta = { beta: 1, days: 0, estimated: false, correlation: null };
  const fa = new Map(factor.filter((c) => c.close > 0).map((c) => [dayKey(c.time), c.close]));
  const shared = [...new Map(asset.filter((c) => c.close > 0).map((c) => [dayKey(c.time), c.close])).entries()]
    .filter(([d]) => fa.has(d))
    .sort((a, b) => a[0].localeCompare(b[0]))
    .slice(-(days + 1));
  const x: number[] = [];
  const y: number[] = [];
  for (let i = 1; i < shared.length; i++) {
    x.push(shared[i]![1] / shared[i - 1]![1] - 1);
    y.push(fa.get(shared[i]![0])! / fa.get(shared[i - 1]![0])! - 1);
  }
  const n = x.length;
  if (n < MIN_BETA_DAYS) return { ...none, days: n };
  const mx = x.reduce((s, v) => s + v, 0) / n;
  const my = y.reduce((s, v) => s + v, 0) / n;
  let cov = 0;
  let vx = 0;
  let vy = 0;
  for (let i = 0; i < n; i++) {
    cov += (x[i]! - mx) * (y[i]! - my);
    vx += (x[i]! - mx) ** 2;
    vy += (y[i]! - my) ** 2;
  }
  if (!(vy > 0)) return { ...none, days: n };
  return { beta: cov / vy, days: n, estimated: true, correlation: vx > 0 ? cov / Math.sqrt(vx * vy) : null };
}

export interface WhatIfLine {
  key: string;
  symbol: string;
  name: string;
  kind: Kind;
  /** Value of the line in the simulated amount (USD). */
  value: number;
  /** % of the simulated amount. */
  weight: number;
  beta: number | null;
  days: number;
  correlation: number | null;
  /** The line is the factor itself (beta 1 by definition). */
  reference: boolean;
  /** Move of the line (%), null when its beta could not be measured ("non couvert"). */
  movePercent: number | null;
  /** USD, positive = loss; null when not covered. */
  loss: number | null;
}

export interface WhatIfResult {
  factor: FactorKey;
  shock: number;
  /** Simulated amount: the portfolio's value, or the amount entered spread on the current weights. */
  base: number;
  scaled: boolean;
  cash: number;
  lines: WhatIfLine[];
  /** Total loss of the covered lines (USD, positive = loss) and its share of `base`. */
  loss: number;
  lossPercent: number;
  /** Value of the lines whose beta could not be measured (left out of the total, never guessed). */
  uncoveredValue: number;
  uncovered: string[];
  worst: WhatIfLine | null;
  /** Shortest and longest window of the betas used (days). */
  minDays: number | null;
  maxDays: number | null;
}

/**
 * Each line moves by its beta to the factor × the shock (never below −100 %); cash does not move. Lines of the same
 * asset are merged. With `amount` (> 0), the lines are rescaled to that amount on the current weights (cash included).
 */
export function whatIf(a: PortfolioAnalysis, factor: FactorKey, shock: number, betas: Record<string, FactorBeta | undefined>, amount?: number | null): WhatIfResult {
  const f = FACTORS[factor];
  const scaled = amount != null && Number.isFinite(amount) && amount > 0 && a.total > 0;
  const k = scaled ? amount! / a.total : 1;
  const base = scaled ? amount! : a.total;
  const merged = new Map<string, { symbol: string; name: string; kind: Kind; value: number }>();
  for (const l of a.lines) {
    const key = `${l.kind}:${l.symbol}`;
    const m = merged.get(key);
    merged.set(key, { symbol: l.symbol, name: l.name, kind: l.kind, value: (m?.value ?? 0) + l.value * k });
  }
  const lines = [...merged.entries()]
    .map(([key, m]): WhatIfLine => {
      const reference = m.symbol === f.symbol && m.kind === f.kind;
      const b = betas[key];
      const beta = reference ? 1 : b?.estimated ? b.beta : null;
      const movePercent = beta == null ? null : Math.max(-100, beta * shock);
      return {
        key, symbol: m.symbol, name: m.name, kind: m.kind, value: m.value, weight: base > 0 ? (m.value / base) * 100 : 0,
        beta, days: reference ? 0 : b?.days ?? 0, correlation: reference ? 1 : b?.correlation ?? null, reference,
        movePercent, loss: movePercent == null ? null : (-m.value * movePercent) / 100,
      };
    })
    .sort((x, y) => (y.loss ?? -Infinity) - (x.loss ?? -Infinity) || y.value - x.value);
  const covered = lines.filter((l) => l.loss != null);
  const loss = covered.reduce((s, l) => s + l.loss!, 0);
  const worst = covered.reduce<WhatIfLine | null>((w, l) => (l.loss! > 0 && (!w || l.loss! > w.loss!) ? l : w), null);
  const measured = covered.filter((l) => !l.reference).map((l) => l.days);
  return {
    factor, shock, base, scaled, cash: a.cash * k, lines, loss, lossPercent: base > 0 ? (loss / base) * 100 : 0,
    uncoveredValue: lines.filter((l) => l.loss == null).reduce((s, l) => s + l.value, 0),
    uncovered: lines.filter((l) => l.loss == null).map((l) => l.symbol),
    worst,
    minDays: measured.length ? Math.min(...measured) : null,
    maxDays: measured.length ? Math.max(...measured) : null,
  };
}
