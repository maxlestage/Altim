/**
 * Strategy comparator (/api/strategies): JSON contract and the pure helpers of the card (colours, chart geometry,
 * direct labels, list view), kept apart from the React view so they can be tested and ported to the mobile apps.
 */
import type { Kind } from "../engine/reliability";

export type StrategyId = "trend" | "momentum" | "breakout" | "meanReversion" | "swing" | "dca" | "buyHold" | "value";

export type RegimeStat = {
  regime: "bull" | "bear" | "range" | "crisis" | "unknown";
  label: string;
  trades: number;
  winRate: number | null;
  avgReturn: number | null;
  lowSample: boolean;
};

export type StrategyMetrics = {
  /** % over the period, costs included (DCA: gain ÷ total invested). */
  totalReturn: number;
  /** % per year, null under one year (DCA: internal rate of return). */
  cagr: number | null;
  /** %, negative. */
  maxDrawdown: number;
  sharpe: number | null;
  sortino: number | null;
  winRate: number | null;
  profitFactor: number | null;
  /** % per trade. */
  expectancy: number | null;
  avgR: number | null;
  trades: number;
  /** % of the tested days invested. */
  exposure: number;
};

export type StrategyResult = {
  id: StrategyId;
  name: string;
  rule: string;
  params: string;
  available: boolean;
  unavailable: string | null;
  metrics: StrategyMetrics | null;
  regimes: RegimeStat[];
  /** [ms, value of 100 invested], ≤ 200 points, same dates for every strategy. */
  equity: [number, number][];
  lowSample: boolean;
  note: string | null;
};

export type StrategiesReport = {
  symbol: string;
  kind: Kind;
  asOf: number;
  from: number;
  to: number;
  bars: number;
  period: string;
  source: string;
  feesPct: number;
  slippagePct: number;
  spreadPct: number;
  notes: string[];
  strategies: StrategyResult[];
};

export const strategiesUrl = (symbol: string, kind: Kind) => `/api/strategies?symbol=${encodeURIComponent(symbol)}&kind=${kind}`;

/** Fixed order of the chips and of the cards. */
export const ORDER: StrategyId[] = ["trend", "momentum", "breakout", "meanReversion", "swing", "dca", "buyHold", "value"];
export const DEFAULT_SELECTION: StrategyId[] = ["trend", "breakout", "meanReversion", "dca", "buyHold"];

/**
 * One colour per strategy, whatever is selected (colour follows the entity). Palette validated on the card surface
 * (#0c1021, dark): slots blue, yellow, magenta, green, violet, orange, aqua pass the adjacent checks; the four default
 * colours pass all pairs (CVD ΔE 6.9, hence the direct labels). Buy and hold is the neutral, dashed reference.
 */
export const COLORS: Record<StrategyId, string> = {
  trend: "#3987e5",
  breakout: "#c98500",
  meanReversion: "#d55181",
  dca: "#008300",
  momentum: "#9085e9",
  swing: "#d95926",
  value: "#199e70",
  buyHold: "#9aa0b4",
};
export const REFERENCE: StrategyId = "buyHold";

/** Short names of the direct labels. */
export const SHORT: Record<StrategyId, string> = {
  trend: "Tendance",
  momentum: "Momentum",
  breakout: "Cassure",
  meanReversion: "Moyenne",
  swing: "Swing",
  dca: "DCA",
  buyHold: "Conserver",
  value: "PER",
};

export const NNBSP = " ";
const fmt = (v: number, digits: number) => v.toLocaleString("fr-FR", { minimumFractionDigits: digits, maximumFractionDigits: digits }).replace(/ /g, NNBSP);
/** "+12,3 %" / "−4,0 %" / "—". */
export const signedPct = (v: number | null | undefined, digits = 1) =>
  v == null ? "—" : `${v > 0 ? "+" : v < 0 ? "−" : ""}${fmt(Math.abs(v), digits)}${NNBSP}%`;
export const plainPct = (v: number | null | undefined, digits = 0) => (v == null ? "—" : `${fmt(v, digits)}${NNBSP}%`);
export const ratio = (v: number | null | undefined) => (v == null ? "—" : `${v < 0 ? "−" : ""}${fmt(Math.abs(v), 2)}`);
export const shortDate = (t: number) => new Date(t).toLocaleDateString("fr-FR", { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });

/** Strategies to draw: selected, available, with a curve, in the fixed order. */
export function drawable(report: StrategiesReport, selected: StrategyId[]): StrategyResult[] {
  return ORDER.map((id) => report.strategies.find((s) => s.id === id)).filter(
    (s): s is StrategyResult => !!s && selected.includes(s.id) && s.available && s.equity.length > 1,
  );
}

export type Geometry = { lo: number; hi: number; x: (i: number) => number; y: (v: number) => number; n: number };

/** One axis (value of 100 invested), 100 always inside the range, a little air above and below. */
export function geometry(series: StrategyResult[], box: { w: number; h: number; l: number; r: number; t: number; b: number }): Geometry | null {
  const n = Math.max(0, ...series.map((s) => s.equity.length));
  if (n < 2) return null;
  const values = series.flatMap((s) => s.equity.map((p) => p[1]));
  let lo = Math.min(100, ...values);
  let hi = Math.max(100, ...values);
  const pad = (hi - lo) * 0.06 || 5;
  lo -= pad;
  hi += pad;
  return {
    lo, hi, n,
    x: (i) => box.l + (i / (n - 1)) * (box.w - box.l - box.r),
    y: (v) => box.t + (1 - (v - lo) / (hi - lo)) * (box.h - box.t - box.b),
  };
}

/** Vertical positions of the end labels, pushed apart by `gap` and kept inside [top, bottom]; same order as `ys`. */
export function placeLabels(ys: number[], gap: number, top: number, bottom: number): number[] {
  const order = ys.map((y, i) => ({ y, i })).sort((a, b) => a.y - b.y);
  const out = order.map((o) => Math.min(Math.max(o.y, top), bottom));
  for (let k = 1; k < out.length; k++) out[k] = Math.max(out[k]!, out[k - 1]! + gap);
  // Overflow at the bottom: shift the stack up.
  const over = out.length ? out[out.length - 1]! - bottom : 0;
  if (over > 0) for (let k = 0; k < out.length; k++) out[k] = out[k]! - over;
  for (let k = out.length - 2; k >= 0; k--) out[k] = Math.min(out[k]!, out[k + 1]! - gap);
  const res = new Array<number>(ys.length);
  order.forEach((o, k) => (res[o.i] = out[k]!));
  return res;
}

/** Point index under a pointer at `px` (viewBox units). */
export function indexAt(px: number, g: Geometry, box: { w: number; l: number; r: number }): number {
  return Math.max(0, Math.min(g.n - 1, Math.round(((px - box.l) / (box.w - box.l - box.r)) * (g.n - 1))));
}

/** List view: value of 100 invested at `steps` evenly spaced dates (first and last included). */
export function checkpoints(s: StrategyResult, steps = 5): { t: number; v: number }[] {
  const n = s.equity.length;
  if (!n) return [];
  const idx = [...new Set(Array.from({ length: Math.min(steps, n) }, (_, k) => Math.round((k * (n - 1)) / Math.max(1, Math.min(steps, n) - 1))))];
  return idx.map((i) => ({ t: s.equity[i]![0], v: s.equity[i]![1] }));
}

/** "échantillon trop faible" for the signal strategies with fewer than 10 trades. */
export const lowSampleText = (s: StrategyResult) => (s.lowSample ? `échantillon trop faible (${s.metrics?.trades ?? 0} trade${(s.metrics?.trades ?? 0) > 1 ? "s" : ""})` : null);
