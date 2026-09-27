/**
 * Analysis of the portfolio the user actually holds.
 * Pure, deterministic functions: same results in the browser and in the iOS app (Swift port,
 * checked by a shared fixture file).
 */
import { atr, type Action, type Candle } from "./signal";
import type { Kind, ReliabilityLevel } from "./reliability";

export interface Holding {
  id: string;
  symbol: string;
  kind: Kind;
  name: string;
  quantity: number;
  /** Average cost price (PRU) in USD. */
  averagePrice: number;
}

export interface MarketInput {
  price: number;
  /** Daily candles (volatility, correlation, protective stop). */
  daily: Candle[];
  daySignal?: { action: Action; score: number } | null;
  shortSignal?: { action: Action; score: number } | null;
  reliability?: ReliabilityLevel | null;
}

export type Recommendation = "sell" | "protect" | "lighten" | "strengthen" | "hold" | "unknown";
export type InsightLevel = "danger" | "warning" | "info" | "good";
export interface Insight { level: InsightLevel; code: string; values: Record<string, number | string> }

export interface LineAnalysis {
  id: string;
  symbol: string;
  kind: Kind;
  name: string;
  quantity: number;
  averagePrice: number;
  price: number | null;
  value: number;
  invested: number;
  pnl: number;
  pnlPercent: number;
  weight: number; // % of the total (cash included)
  stop: number | null; // protective stop: price − 2 × daily ATR
  lossAtStop: number | null;
  recommendation: Recommendation;
  reasons: string[]; // reason codes
  /** Suggested sale amount for "lighten" (USD), otherwise 0. */
  trimValue: number;
}

export interface PortfolioAnalysis {
  total: number;
  cash: number;
  invested: number;
  marketValue: number;
  pnl: number;
  pnlPercent: number;
  allocation: { crypto: number; stock: number; cash: number };
  lines: LineAnalysis[];
  risk: {
    maxWeight: number;
    effectiveAssets: number;
    volatilityAnnual: number | null;
    var95Day: number | null; // loss (USD) exceeded 1 day out of 20, positive
    var95DayPercent: number | null;
    averageCorrelation: number | null;
    lossAtStops: number;
  };
  insights: Insight[];
}

export const MAX_WEIGHT = 35;
export const TARGET_WEIGHT = 30;

const isBuyA = (a?: Action | null) => a === "buy" || a === "strongBuy";
const isSellA = (a?: Action | null) => a === "sell" || a === "strongSell";

/** Daily closes indexed by UTC date (crypto and stocks aligned by calendar day). */
function closesByDay(c: Candle[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const x of [...c].sort((a, b) => a.time - b.time)) m.set(new Date(x.time).toISOString().slice(0, 10), x.close);
  return m;
}

/** Daily returns over the last `days` shared calendar days, carrying the price forward when a market is closed. */
export function alignedReturns(series: Candle[][], days = 90): number[][] {
  const maps = series.map(closesByDay);
  const all = [...new Set(maps.flatMap((m) => [...m.keys()]))].sort();
  const start = all.findIndex((d) => maps.every((m) => [...m.keys()].some((k) => k <= d)));
  if (start < 0) return series.map(() => []);
  const dates = all.slice(start).slice(-(days + 1));
  return maps.map((m) => {
    let last: number | undefined;
    const keys = [...m.keys()].sort();
    for (const k of keys) if (k <= dates[0]!) last = m.get(k);
    const closes = dates.map((d) => (m.has(d) ? (last = m.get(d)!) : last!));
    return closes.slice(1).map((c, i) => (closes[i]! > 0 ? c / closes[i]! - 1 : 0));
  });
}

const mean = (v: number[]) => (v.length ? v.reduce((a, b) => a + b, 0) / v.length : 0);
const std = (v: number[]) => {
  if (v.length < 2) return 0;
  const m = mean(v);
  return Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (v.length - 1));
};
export function correlation(a: number[], b: number[]): number | null {
  const n = Math.min(a.length, b.length);
  if (n < 10) return null;
  const x = a.slice(-n);
  const y = b.slice(-n);
  const mx = mean(x);
  const my = mean(y);
  let sxy = 0, sxx = 0, syy = 0;
  for (let i = 0; i < n; i++) {
    sxy += (x[i]! - mx) * (y[i]! - my);
    sxx += (x[i]! - mx) ** 2;
    syy += (y[i]! - my) ** 2;
  }
  return sxx > 0 && syy > 0 ? sxy / Math.sqrt(sxx * syy) : null;
}
/** 5th percentile (linear interpolation, same definition in Swift). */
export function percentile(v: number[], p: number): number {
  const s = [...v].sort((a, b) => a - b);
  if (!s.length) return 0;
  const idx = (s.length - 1) * p;
  const lo = Math.floor(idx);
  const hi = Math.ceil(idx);
  return s[lo]! + (s[hi]! - s[lo]!) * (idx - lo);
}

export function analyzePortfolio(holdings: Holding[], cash: number, market: Record<string, MarketInput | undefined>): PortfolioAnalysis {
  const key = (h: Holding) => `${h.kind}:${h.symbol}`;
  const base = holdings.map((h) => {
    const m = market[key(h)];
    const price = m && m.price > 0 ? m.price : null;
    const value = h.quantity * (price ?? h.averagePrice);
    const invested = h.quantity * h.averagePrice;
    return { h, m, price, value, invested };
  });
  const marketValue = base.reduce((a, b) => a + b.value, 0);
  const safeCash = Math.max(0, cash);
  const total = marketValue + safeCash;
  const investedTotal = base.reduce((a, b) => a + b.invested, 0);
  const pnl = marketValue - investedTotal;

  const lines: LineAnalysis[] = base.map(({ h, m, price, value, invested }) => {
    const weight = total > 0 ? (value / total) * 100 : 0;
    const pnlL = value - invested;
    const pnlPercent = invested > 0 ? (pnlL / invested) * 100 : 0;
    const a = m?.daily?.length ? atr(m.daily)[m.daily.length - 1] ?? null : null;
    const stop = price && a ? Math.max(price - 2 * a, price * 0.01) : null;
    const lossAtStop = stop && price ? h.quantity * (price - stop) : null;
    const reasons: string[] = [];
    let rec: Recommendation;
    const day = m?.daySignal?.action;
    const short = m?.shortSignal?.action;

    if (!price || !m || m.reliability === "low" || (!m.daySignal && !m.shortSignal)) {
      rec = "unknown";
      reasons.push(!price ? "no_price" : "unreliable");
    } else if (isSellA(day) && isSellA(short)) {
      rec = "sell";
      reasons.push("bearish_day_and_short");
    } else if (isSellA(day) || short === "strongSell") {
      rec = "protect";
      reasons.push(isSellA(day) ? "bearish_day" : "strong_bearish_short");
    } else if (weight > MAX_WEIGHT && holdings.length > 1) {
      rec = "lighten";
      reasons.push("overweight");
    } else if (pnlPercent > 50 && !isBuyA(day)) {
      rec = "lighten";
      reasons.push("take_profit");
    } else if (isBuyA(day) && isBuyA(short) && weight < 20) {
      rec = "strengthen";
      reasons.push("bullish_day_and_short");
    } else {
      rec = "hold";
      reasons.push(isBuyA(day) ? "bullish_day" : "neutral");
    }
    if (rec !== "lighten" && weight > MAX_WEIGHT && holdings.length > 1) reasons.push("overweight");
    if (pnlPercent < -20) reasons.push("deep_loss");

    let trimValue = 0;
    if (rec === "lighten") {
      trimValue = reasons[0] === "overweight" ? Math.max(0, value - (TARGET_WEIGHT / 100) * total) : value * 0.25;
    }
    return {
      id: h.id, symbol: h.symbol, kind: h.kind, name: h.name, quantity: h.quantity, averagePrice: h.averagePrice,
      price, value, invested, pnl: pnlL, pnlPercent, weight, stop, lossAtStop, recommendation: rec, reasons, trimValue,
    };
  });

  const crypto = lines.filter((l) => l.kind === "crypto").reduce((a, l) => a + l.value, 0);
  const stock = lines.filter((l) => l.kind === "stock").reduce((a, l) => a + l.value, 0);
  const pct = (v: number) => (total > 0 ? (v / total) * 100 : 0);
  const weights = lines.map((l) => l.weight / 100);
  const hhi = weights.reduce((a, w) => a + w * w, 0);
  const maxWeight = lines.reduce((a, l) => Math.max(a, l.weight), 0);

  // Volatility, VaR and correlation from aligned daily returns.
  const withData = lines.map((l, i) => ({ l, d: base[i]!.m?.daily ?? [] })).filter((x) => x.d.length >= 20);
  let volatilityAnnual: number | null = null;
  let var95Day: number | null = null;
  let averageCorrelation: number | null = null;
  if (withData.length && total > 0) {
    const returns = alignedReturns(withData.map((x) => x.d));
    const n = Math.min(...returns.map((r) => r.length));
    if (n >= 20) {
      const port = Array.from({ length: n }, (_, t) =>
        withData.reduce((a, x, i) => a + (x.l.value / total) * returns[i]![returns[i]!.length - n + t]!, 0),
      );
      volatilityAnnual = std(port) * Math.sqrt(365) * 100;
      var95Day = Math.max(0, -percentile(port, 0.05) * total);
      const pairs: number[] = [];
      for (let i = 0; i < returns.length; i++)
        for (let j = i + 1; j < returns.length; j++) {
          const c = correlation(returns[i]!, returns[j]!);
          if (c !== null) pairs.push(c);
        }
      averageCorrelation = pairs.length ? mean(pairs) : null;
    }
  }
  const lossAtStops = lines.reduce((a, l) => a + (l.lossAtStop ?? 0), 0);

  const insights: Insight[] = [];
  const add = (level: InsightLevel, code: string, values: Insight["values"] = {}) => insights.push({ level, code, values });
  if (!lines.length) add("info", "empty");
  else {
    add(pnl >= 0 ? "good" : "warning", "pnl", { pnl, pnlPercent: investedTotal > 0 ? (pnl / investedTotal) * 100 : 0 });
    const sells = lines.filter((l) => l.recommendation === "sell" || l.recommendation === "protect");
    if (sells.length) add("danger", "act_bearish", { count: sells.length, symbols: sells.map((l) => l.symbol).join(", ") });
    const top = lines.reduce((a, l) => (l.weight > a.weight ? l : a));
    if (lines.length > 1 && top.weight > 40) add("danger", "concentration", { symbol: top.symbol, weight: top.weight });
    else if (lines.length > 1 && top.weight > 25) add("warning", "concentration", { symbol: top.symbol, weight: top.weight });
    else if (lines.length === 1) add("warning", "single_asset", { symbol: top.symbol });
    if (pct(crypto) > 60) add("warning", "crypto_heavy", { weight: pct(crypto) });
    if (lines.length > 1 && hhi > 0 && 1 / hhi < 3) add("warning", "low_diversification", { effective: 1 / hhi });
    if (averageCorrelation !== null && averageCorrelation > 0.7 && lines.length > 1) add("warning", "correlated", { correlation: averageCorrelation });
    if (var95Day !== null) add("info", "var", { amount: var95Day, percent: total > 0 ? (var95Day / total) * 100 : 0 });
    if (lossAtStops > 0) add("info", "stops", { amount: lossAtStops, percent: total > 0 ? (lossAtStops / total) * 100 : 0 });
    if (pct(safeCash) < 5) add("info", "low_cash", { weight: pct(safeCash) });
    const deep = lines.filter((l) => l.pnlPercent < -20);
    if (deep.length) add("warning", "deep_loss", { symbols: deep.map((l) => l.symbol).join(", ") });
    const strong = lines.filter((l) => l.recommendation === "strengthen");
    if (strong.length) add("good", "opportunities", { symbols: strong.map((l) => l.symbol).join(", ") });
    const unknown = lines.filter((l) => l.recommendation === "unknown");
    if (unknown.length) add("warning", "unknown", { symbols: unknown.map((l) => l.symbol).join(", ") });
  }
  const order: InsightLevel[] = ["danger", "warning", "good", "info"];
  insights.sort((a, b) => order.indexOf(a.level) - order.indexOf(b.level));

  return {
    total, cash: safeCash, invested: investedTotal, marketValue, pnl,
    pnlPercent: investedTotal > 0 ? (pnl / investedTotal) * 100 : 0,
    allocation: { crypto: pct(crypto), stock: pct(stock), cash: pct(safeCash) },
    lines,
    risk: {
      maxWeight, effectiveAssets: hhi > 0 ? 1 / hhi : 0, volatilityAnnual, var95Day,
      var95DayPercent: var95Day !== null && total > 0 ? (var95Day / total) * 100 : null,
      averageCorrelation, lossAtStops,
    },
    insights,
  };
}

// ---------- Plain-language texts (French) ----------

const usd = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: 0 })} $`;
const pc = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;

export const RECOMMENDATION_LABEL: Record<Recommendation, string> = {
  sell: "Vendre ou protéger",
  protect: "Protéger (stop)",
  lighten: "Alléger",
  strengthen: "Renforcer possible",
  hold: "Conserver",
  unknown: "Données insuffisantes",
};

export const REASON_TEXT: Record<string, string> = {
  bearish_day_and_short: "Signaux baissiers en journalier et en 4 h : la tendance s'est retournée.",
  bearish_day: "Signal journalier baissier : placez ou remontez un stop pour limiter la baisse.",
  strong_bearish_short: "Forte pression vendeuse à court terme (4 h) : protégez la position.",
  overweight: `Cette ligne dépasse ${MAX_WEIGHT} % de votre patrimoine : trop dépendant d'un seul actif.`,
  take_profit: "Plus de 50 % de gain sans signal haussier : sécuriser une partie des gains.",
  bullish_day_and_short: "Signaux haussiers en journalier et en 4 h, poids encore modéré.",
  bullish_day: "Tendance journalière haussière : rien à faire.",
  neutral: "Pas de signal fort : rien à faire pour l'instant.",
  unreliable: "Sources de données absentes ou en désaccord : aucun conseil plutôt qu'un mauvais conseil.",
  no_price: "Cours introuvable pour ce symbole.",
  deep_loss: "Perte de plus de 20 % : évitez de moyenner à la baisse sans signal haussier.",
};

export function insightText(i: Insight): string {
  const v = i.values;
  switch (i.code) {
    case "empty": return "Ajoutez vos actifs pour obtenir une analyse complète.";
    case "pnl": return `Plus-value latente : ${(v.pnl as number) >= 0 ? "+" : "−"}${usd(Math.abs(v.pnl as number))} (${(v.pnlPercent as number) >= 0 ? "+" : "−"}${pc(Math.abs(v.pnlPercent as number))}).`;
    case "act_bearish": return `${v.count} ligne(s) à surveiller de près (signaux baissiers) : ${v.symbols}.`;
    case "concentration": return `${v.symbol} pèse ${pc(v.weight as number)} de votre patrimoine : une baisse de cet actif vous toucherait fortement.`;
    case "single_asset": return `Tout est investi sur ${v.symbol} : aucune diversification.`;
    case "crypto_heavy": return `${pc(v.weight as number)} en crypto : portefeuille très volatil.`;
    case "low_diversification": return `Diversification faible : l'équivalent de ${(v.effective as number).toFixed(1)} actif(s) de même poids.`;
    case "correlated": return `Vos actifs évoluent ensemble (corrélation moyenne ${(v.correlation as number).toFixed(2)}) : ils baisseront probablement en même temps.`;
    case "var": return `Lors d'une mauvaise journée (1 sur 20), vous pourriez perdre environ ${usd(v.amount as number)} (${pc(v.percent as number)}).`;
    case "stops": return `Si tous les stops conseillés étaient touchés : perte d'environ ${usd(v.amount as number)} (${pc(v.percent as number)}).`;
    case "low_cash": return `Peu de liquidités (${pc(v.weight as number)}) : aucune réserve pour saisir une opportunité.`;
    case "deep_loss": return `Lignes en perte de plus de 20 % : ${v.symbols}.`;
    case "opportunities": return `Renforcement possible selon les signaux : ${v.symbols}.`;
    case "unknown": return `Analyse impossible pour : ${v.symbols} (données insuffisantes).`;
    default: return i.code;
  }
}
