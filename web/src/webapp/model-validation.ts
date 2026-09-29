/**
 * « Validation du modèle » (/api/validation): JSON contract and the pure helpers of the screen (sorting, texts),
 * kept apart from the React view so they can be tested and ported to the mobile apps. Percentages are in %, times
 * in ms. The server computes everything; nothing here recomputes a statistic.
 */
import type { Kind } from "../engine/reliability";

export type AssetClass = "stock" | "btc" | "eth" | "altcoin";
export type Regime = "bull" | "bear" | "range" | "crisis" | "unknown";
export type Verdict = "insufficient" | "edge" | "negative" | "unproven";

/** Trade statistics pooled over every trade of a group (each trade weighs the same). */
export type Pooled = {
  trades: number;
  winRate: number | null;
  profitFactor: number | null;
  /** Mean net return per trade (%), costs included. */
  expectancy: number | null;
  avgR: number | null;
  /** Mean ÷ standard error of the net trade returns. */
  tStat: number | null;
};

export type RegimeGroup = Pooled & {
  regime: Regime;
  label: string;
  /** Asset-days in this regime (regime read the day before). */
  days: number;
  /** Assets with ≥ 20 days in it; those where the signal made more than buy-and-hold over these days. */
  assets: number;
  beatHold: number;
  beatShare: number | null;
  /** Medians over those assets, compounded over the regime's days (%). */
  medianSignal: number | null;
  medianHold: number | null;
  lowSample: boolean;
  verdict: Verdict;
  verdictLabel: string;
};

export type Named = { symbol: string; value: number };

export type GroupStat = Pooled & {
  /** "all" or the asset class. */
  id: "all" | AssetClass;
  label: string;
  assets: number;
  years: number | null;
  medianReturn: number | null;
  worstReturn: Named | null;
  medianHold: number | null;
  /** ≤ 0. */
  medianDrawdown: number | null;
  worstDrawdown: Named | null;
  medianHoldDrawdown: number | null;
  medianSharpe: number | null;
  medianSortino: number | null;
  medianExposure: number | null;
  beatHold: number;
  beatShare: number | null;
  medianAfterTax: number | null;
  medianHoldAfterTax: number | null;
  lowSample: boolean;
  verdict: Verdict;
  verdictLabel: string;
  regimes: RegimeGroup[];
};

export type RegimeDays = { regime: Regime; days: number; signal: number; hold: number };

export type AssetResult = {
  symbol: string;
  name: string;
  kind: Kind;
  class: AssetClass;
  group: string;
  from: number;
  to: number;
  bars: number;
  trades: number;
  winRate: number | null;
  profitFactor: number | null;
  expectancy: number | null;
  avgR: number | null;
  maxDrawdown: number;
  holdMaxDrawdown: number;
  sharpe: number | null;
  sortino: number | null;
  totalReturn: number;
  buyAndHold: number;
  exposure: number;
  beatHold: boolean;
  afterTax: number;
  holdAfterTax: number;
  lowSample: boolean;
  regimeDays: RegimeDays[];
  source: string;
};

export type Failure = { symbol: string; name: string; kind: Kind; class: AssetClass; error: string };

export type ValidationReport = {
  asOf: number;
  basketFixedOn: string;
  headline: string;
  from: number | null;
  to: number | null;
  years: number | null;
  minYears: number | null;
  maxYears: number | null;
  overall: GroupStat;
  classes: GroupStat[];
  /** Basket order (by class), never ranked by performance. */
  assets: AssetResult[];
  failures: Failure[];
  basket: { symbol: string; name: string; kind: Kind; class: AssetClass; group: string }[];
  parameters: {
    feesPct: number;
    slippagePct: number;
    spreadStockPct: number;
    spreadCryptoPct: number;
    lookback: number;
    warmup: number;
    rewardRisk: number;
    taxRatePct: number;
    minTrades: number;
    tEdge: number;
    regimeRule: string;
  };
  protections: string[];
  outOfSample: { status: "verified" | "notVerifiable"; note: string };
  limits: string[];
  source: string;
};

export const validationUrl = () => "/api/validation";

export const CLASS_ORDER: AssetClass[] = ["stock", "btc", "eth", "altcoin"];
export const CLASS_SHORT: Record<AssetClass, string> = { stock: "Actions", btc: "Bitcoin", eth: "Ethereum", altcoin: "Altcoins" };

export type SortKey = "class" | "name" | "gap";
export const SORTS: [SortKey, string][] = [["class", "Par classe"], ["name", "Par nom"], ["gap", "Écart avec la détention"]];

/** Rows in the chosen order: by class (the basket's own order, the default), by name, or by the gap signal −
 * buy-and-hold (largest first). Never mutates the report. */
export function sortAssets(assets: AssetResult[], key: SortKey): AssetResult[] {
  const rows = [...assets];
  if (key === "name") rows.sort((a, b) => a.symbol.localeCompare(b.symbol, "fr"));
  else if (key === "gap") rows.sort((a, b) => b.totalReturn - b.buyAndHold - (a.totalReturn - a.buyAndHold));
  else rows.sort((a, b) => CLASS_ORDER.indexOf(a.class) - CLASS_ORDER.indexOf(b.class));
  return rows;
}

export const NNBSP = " ";
const fr = (v: number, max: number) => v.toLocaleString("fr-FR", { minimumFractionDigits: 0, maximumFractionDigits: max });

/** "+12,3 %", "−4 %", "—". */
export function signedPct(v: number | null | undefined, digits = 1): string {
  if (v == null || !Number.isFinite(v)) return "—";
  return `${v < 0 ? "−" : v > 0 ? "+" : ""}${fr(Math.abs(v), digits)}${NNBSP}%`;
}

export function plain(v: number | null | undefined, digits = 2): string {
  if (v == null || !Number.isFinite(v)) return "—";
  return `${v < 0 ? "−" : ""}${fr(Math.abs(v), digits)}`;
}

export function years(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return "—";
  return `${fr(v, 1)}${NNBSP}an${v >= 2 ? "s" : ""}`;
}

/** Tone of a verdict for the chips: good only for a measured positive gain, warning for a measured loss. */
export function verdictTone(v: Verdict): "good" | "warning" | "info" {
  return v === "edge" ? "good" : v === "negative" ? "warning" : "info";
}

/** "6 sur 34 (18 %)". */
export function beatText(beat: number, n: number, share: number | null): string {
  if (n === 0) return "aucun actif comparable";
  return `${beat} sur ${n}${share == null ? "" : ` (${fr(share, 0)}${NNBSP}%)`}`;
}

/** What the regime days say, in one sentence (nothing when no asset spent 20 days in it). */
export function regimeDaysText(g: RegimeGroup): string {
  if (g.assets === 0) return "Aucun actif n'a passé 20 jours dans ce régime : pas de comparaison avec la détention.";
  return `Pendant ces jours, le signal a fait mieux que la détention sur ${beatText(g.beatHold, g.assets, g.beatShare)} actifs (médianes : signal ${signedPct(g.medianSignal)}, détention ${signedPct(g.medianHold)}).`;
}

/** Trades line of a pooled group: "650 trades · réussite 38 % · espérance +0,2 % · t = 2,2". */
export function pooledText(p: Pooled): string {
  if (p.trades === 0) return "Aucun trade.";
  const parts = [`${p.trades} trade${p.trades > 1 ? "s" : ""}`, `réussite ${fr(p.winRate ?? 0, 0)}${NNBSP}%`, `espérance ${signedPct(p.expectancy, 2)}`];
  if (p.profitFactor != null) parts.push(`facteur de profit ${plain(p.profitFactor)}`);
  if (p.tStat != null) parts.push(`t = ${plain(p.tStat, 1)}`);
  return parts.join(" · ");
}

const MONTHS = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
export function monthYear(ms: number | null | undefined): string {
  if (ms == null) return "?";
  const d = new Date(ms);
  return `${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
}

/** Regimes of a group, the "unknown" one (history too short to classify) last. */
export function regimesOf(g: GroupStat): RegimeGroup[] {
  return [...g.regimes].sort((a, b) => Number(a.regime === "unknown") - Number(b.regime === "unknown"));
}
