/**
 * "Décision" (/api/decision): contract types (mirror of backend/src/engine/decision_types.rs, serde camelCase),
 * a light runtime check of the answer, French formatting and the portfolio weights sent in personal mode.
 * Pure functions (no React, no fetch) so they are tested with `bun test`.
 */
import { currencySymbol, moneyCompact, toDisplay } from "../money";
import type { Kind } from "../engine/reliability";
import type { CalendarEvent } from "./calendar";
import { recordConfiguration } from "./config-changes";
import { isBotView, type BotView } from "./model-bot";

export type Verdict = "buy" | "buyZone" | "wait" | "noPosition" | "trim" | "sell";
export type Level = "strong" | "moderate" | "waiting" | "highRisk" | "exit";
export type FamilyStatus = "positive" | "neutral" | "negative" | "unavailable";
export type StepState = "ok" | "no" | "unknown";
export type ScenarioKind = "bull" | "neutral" | "bear";
export type Uncertainty = "low" | "medium" | "high";
export type ExitKind = "profit" | "defensive" | "macro";

export interface Family { key: string; label: string; score: number | null; status: FamilyStatus; summary: string; points: string[]; source: string }
export interface Veto { code: string; label: string; active: boolean; verifiable: boolean; detail: string }
export interface Step { label: string; state: StepState; detail: string }
export interface Setup { name: string; steps: Step[]; met: number; total: number }
export interface Plan {
  zoneFrom: number; zoneTo: number; entry: number; stop: number; target1: number; target2: number | null;
  riskPct: number; reward1Pct: number; reward2Pct: number | null; riskReward: number; minRiskReward: number;
  acceptable: boolean; horizon: string;
  /** Next level beyond target 2 (or target 2 + (target 2 − target 1)); absent from older answers. */
  target3?: number | null; reward3Pct?: number | null; target3Source?: string | null;
}
export interface Condition { text: string; level: number | null }
export type CheckState = "met" | "unmet" | "unknown";
/** A scenario condition checked on the current data (daily candles, closed). */
export interface ScenarioCheck { text: string; state: CheckState; detail: string }
export interface Scenario {
  kind: ScenarioKind; title: string; condition: string; consequence: string; level: number | null;
  /** Added later (absent from older answers): 3 conditions (bull, bear) or 2 (neutral), how many are met, the one unfolding. */
  conditions?: ScenarioCheck[]; met?: number; unfolding?: boolean;
}
export interface WhyNot { risks: string[]; uncertainty: Uncertainty; invalidation: string[] }
export interface EarningsDate { date: number; estimated: boolean }
export interface EarningsSurprise { quarter: string; eps: number; consensus: number; surprisePct: number }
export interface Revisions { monthAgo: number; now: number; changePct: number }
export interface Sector { label: string; sic: string; sicDescription: string; source: string }
/** A ratio against its own daily history; `percentile`: % of days at or below today's value. */
export interface RatioHistory { current: number; median: number; min: number; max: number; percentile: number; days: number; from: number; to: number }
export interface ValuationHistory { per: RatioHistory | null; ps: RatioHistory | null; method: string; source: string }
export interface Peer {
  symbol: string; name: string; per: number | null; ps: number | null; operatingMargin: number | null; netMargin: number | null;
  revenueGrowth: number | null; periodEnd: number;
}
export interface PeerComparison {
  group: string; peers: Peer[]; medianPer: number | null; medianPs: number | null; medianOperatingMargin: number | null;
  medianNetMargin: number | null; medianRevenueGrowth: number | null; date: number; source: string;
}
export interface DevActivity {
  repo: string | null; commits4w: number | null; pullRequestsMerged: number | null; contributors: number | null; stars: number | null;
  additions4w: number | null; deletions4w: number | null; smartContractPlatform: boolean; source: string;
}
export interface StablecoinFlows {
  scope: string; date: number; total: number; change7d: number | null; change7dPct: number | null; change30d: number | null;
  change30dPct: number | null; source: string;
}
export interface StockFundamentals {
  kind: "stock"; period: string;
  revenue: number | null; revenueGrowth: number | null; netIncome: number | null; eps: number | null; epsGrowth: number | null;
  grossMargin: number | null; operatingMargin: number | null; netMargin: number | null; freeCashFlow: number | null; fcfMargin: number | null;
  debt: number | null; cash: number | null; netDebt: number | null; roe: number | null; per: number | null; peg: number | null;
  evEbitda: number | null; dividendYield: number | null; shareChange: number | null; nextEarnings: EarningsDate | null;
  surprises: EarningsSurprise[]; revisions: Revisions | null; sectorNote: string; source: string;
  // Added later: absent from decisions cached by an older version.
  ps?: number | null; pb?: number | null; roic?: number | null; roicTaxRate?: number | null; roicTaxStatutory?: boolean;
  periodEnd?: number | null; filedAt?: number | null; sector?: Sector | null; valuationHistory?: ValuationHistory | null;
  peers?: PeerComparison | null; valuationVerdict?: string | null; guidance?: string;
}
export interface CryptoFundamentals {
  kind: "crypto";
  marketCap: number | null; fdv: number | null; mcFdv: number | null; circulatingSupply: number | null; totalSupply: number | null;
  maxSupply: number | null; circulatingPct: number | null; tvl: number | null; fees30d: number | null; btcDominance: number | null;
  fundingRate: number | null; openInterest: number | null; txPerDay: number | null; hashRate: number | null; unlocks: string; source: string;
  devActivity?: DevActivity | null; stablecoins?: StablecoinFlows | null; chainStablecoins?: StablecoinFlows | null; notCovered?: string;
}
export type Fundamentals = StockFundamentals | CryptoFundamentals;
export interface Liquidity { spreadPct: number | null; dailyValue: number | null; relativeVolume: number | null; source: string }
export interface Track {
  period: string; trades: number; winRate: number; avgWin: number | null; avgLoss: number | null; profitFactor: number | null;
  sharpe: number | null; sortino: number | null; maxDrawdown: number; totalReturn: number; buyAndHold: number;
  feesPct: number; slippagePct: number; losingStreak: number; note: string;
  // Added later (absent from older answers): spread cost, expectancy, R multiples, results by market regime.
  spreadPct?: number; spreadMeasured?: boolean; spreadNote?: string; expectancy?: number | null; avgR?: number | null;
  regimes?: import("../engine/backtest").RegimeStat[]; testedBars?: number; biasNotes?: string[];
}
export interface Exit { kind: ExitKind; share: number; trigger: string; price: number | null; now: boolean }
export interface Position { cost: number; pnlPct: number | null; advice: string; exits: Exit[] }
export interface Exposure { factor: string; weight: number; assets: string[]; correlation: number | null; warning: string | null }
export interface DataSource { name: string; ok: boolean; detail: string }

// Summaries and technical structure (added fields: absent from older servers' answers and from cached decisions).
export type Rating = "strongBuy" | "buy" | "hold" | "reduce" | "sell" | "strongSell";
export interface ScoreFactor { key: string; label: string; weight: number; applied: number; value: number | null; contribution: number | null; sources: string[] }
export interface CompositeScore { value: number | null; label: string; factors: ScoreFactor[]; missing: string[]; custom: boolean; text: string }
export interface Degraded { active: boolean; headline: string; reasons: string[] }
export type RegimeKind = "riskOn" | "riskOff" | "neutral";
export interface MarketRegime { kind: RegimeKind; label: string; benchmark: string | null; reasons: string[] }
export type HorizonKind = "scalping" | "dayTrading" | "swing" | "mediumTerm" | "longTerm";
export interface HorizonClass { kind: HorizonKind; label: string; atrDistance: number; detail: string }
export type Bias = "bullish" | "neutral" | "bearish";
export type Side = "up" | "down";
export interface Ichimoku {
  tenkan: number; kijun: number; senkouA: number; senkouB: number; futureA: number; futureB: number;
  position: "above" | "inside" | "below"; tkCross: Side | null; tkCrossBars: number | null; bias: Bias; reading: string;
}
export interface Supertrend { direction: Side; level: number; bars: number; bias: Bias; reading: string }
export interface Donchian { upper: number; lower: number; mid: number; breakout: Side | null; bias: Bias; reading: string }
export interface Vwap { value: number; bars: number; deviationPct: number; bias: Bias; reading: string }
export interface VolumeProfile { poc: number; valueAreaHigh: number; valueAreaLow: number; bars: number; bins: number; bias: Bias; reading: string; note: string }
export interface FloorPivots { pivot: number; r1: number; r2: number; s1: number; s2: number; from: number; reading: string }
export interface SrLevel { price: number; touches: number; kind: "support" | "resistance"; distancePct: number }
export interface Breakout { kind: "confirmed" | "unconfirmed" | "fake" | "none"; side: Side | null; level: number | null; volumeRatio: number | null; bias: Bias; reading: string }
export interface MarketStructure { trend: "up" | "down" | "mixed"; highs: number[]; lows: number[]; bias: Bias; reading: string }
export interface RsPeriod { label: string; days: number; assetPct: number; benchmarkPct: number; diff: number }
export interface RelativeStrength { benchmark: string; symbol: string; periods: RsPeriod[]; correlation: number | null; bias: Bias; reading: string }
export interface Structure {
  timeframe: string; score: number | null;
  ichimoku: Ichimoku | null; supertrend: Supertrend | null; donchian: Donchian | null; vwap: Vwap | null;
  volumeProfile: VolumeProfile | null; pivots: FloorPivots | null;
  levels: SrLevel[]; nearestSupport: SrLevel | null; nearestResistance: SrLevel | null; levelsReading: string;
  breakout: Breakout | null; marketStructure: MarketStructure | null; relative: RelativeStrength[]; relativeNote: string | null;
}

// Guidance (added fields, absent from older answers): never changes the verdict.
export type NoTradeCode =
  | "volatility" | "liquidity" | "spread" | "earnings" | "announcement" | "event" | "trendless" | "weakSignal" | "degraded" | "marketClosed";
export interface NoTradeReason { code: NoTradeCode | (string & {}); label: string; detail: string }
/** "Quand ne PAS trader": `headline` empty when not active; `unchecked`: checks without data. */
export interface NoTrade { active: boolean; headline: string; reasons: NoTradeReason[]; unchecked: string[] }
export type ActionZoneKind = "invalidation" | "exit" | "buy" | "wait" | "profit";
/** A band of the ladder, USD (from = to for a single level). */
export interface ActionZone { kind: ActionZoneKind; label: string; from: number; to: number; note: string }
/** `zones` ascending by price; `here`: kind of the zone holding the price (null between zones or outside). */
export interface ActionZones { price: number; zones: ActionZone[]; here: ActionZoneKind | null; hereText: string }
export interface Unfolding { kind: ScenarioKind; title: string; met: number; total: number; text: string }
/** kind "level": value is a price (USD); "volume": a daily volume in units of the asset; else value null. */
export interface Invalidator { kind: "level" | "volume" | "event" | "condition"; text: string; value: number | null }
export interface CounterArgument { favourable: number; unfavourable: number; text: string; familiesText: string; invalidators: Invalidator[] }
export interface SnapshotLevel { price: number; touches: number }
export interface SnapshotNews { title: string; tone: "positive" | "negative" | "neutral"; time: number }
/** Compact numbers kept to explain a later change of the signal. */
export interface DecisionSnapshot {
  price: number | null; composite: number | null; families: { key: string; label: string; score: number | null }[];
  momentum: number | null; relativeVolume: number | null; rsi: number | null; adx: number | null;
  nearestSupport: SnapshotLevel | null; nearestResistance: SnapshotLevel | null; newsScore: number | null; topNews: SnapshotNews | null;
}

/** Verdict of the cross-asset validation (`/api/validation`) on a group of trades. */
export type ProofVerdict = "insufficient" | "edge" | "negative" | "unproven";
/**
 * « Preuve du modèle »: what the cross-asset validation says about the asset's class and the regime it is in now
 * (validation's rule on the last closed daily candle). `available` false: no report computed yet (text says so).
 * `weak` (class verdict "negative" or buy-and-hold better on more than two thirds of the class): confidence capped at 60.
 */
export interface ModelEvidence {
  available: boolean; assetClass: "stock" | "btc" | "eth" | "altcoin"; classLabel: string;
  classVerdict: ProofVerdict | null; classVerdictLabel: string | null;
  /** Assets of the class tested; those where the signal beat buy-and-hold ("n/m" in beatHold). */
  assets: number; beatHoldCount: number; beatHold: string | null;
  trades: number; tStat: number | null;
  regime: "bull" | "bear" | "range" | "crisis" | "unknown"; regimeLabel: string;
  regimeVerdict: ProofVerdict | null; regimeTrades: number; regimeTStat: number | null;
  weak: boolean; text: string; asOf: number | null; link: string;
}

export interface Decision {
  symbol: string; kind: Kind; name: string; asOf: number; price: number | null;
  mode: "informational" | "personal" | (string & {});
  verdict: Verdict; label: string; level: Level; levelLabel: string;
  confidence: number; confidenceText: string; headline: string;
  families: Family[]; vetoes: Veto[]; blocked: boolean; setup: Setup; plan: Plan | null;
  whyWait: string[]; toBuy: Condition[]; toSell: Condition[]; scenarios: Scenario[];
  pros: string[]; cons: string[]; whyNot: WhyNot;
  fundamentals: Fundamentals | null; liquidity: Liquidity | null; track: Track | null;
  position: Position | null; exposure: Exposure | null; sources: DataSource[]; disclaimer: string;
  rating?: Rating; ratingLabel?: string; score?: CompositeScore; degraded?: Degraded;
  marketRegime?: MarketRegime | null; horizon?: HorizonClass | null; structure?: Structure | null;
  /** Next 7 days' events (economy, central banks; a stock's earnings, dividends, splits); null: calendar not loaded. */
  events?: CalendarEvent[] | null;
  noTrade?: NoTrade; actionZones?: ActionZones | null; unfolding?: Unfolding | null; counterArgument?: CounterArgument; snapshot?: DecisionSnapshot;
  /** Why the model's evidence lowered a strong rating (ACHAT FORT → ACHAT, VENTE FORTE → VENDRE); null otherwise. */
  ratingReason?: string | null;
  modelEvidence?: ModelEvidence;
  /** « Bot Altim »: the learned model's ACHETER / ATTENDRE / VENDRE and whether it counts (only with an out-of-sample edge). */
  bot?: BotView;
}

// ---------- Runtime check (a wrong answer shows an error instead of a broken card) ----------

const VERDICTS: Verdict[] = ["buy", "buyZone", "wait", "noPosition", "trim", "sell"];
const LEVELS: Level[] = ["strong", "moderate", "waiting", "highRisk", "exit"];
const RATINGS: Rating[] = ["strongBuy", "buy", "hold", "reduce", "sell", "strongSell"];

/** Checks the fields the card relies on; throws a French message naming the first bad field. */
export function parseDecision(raw: unknown): Decision {
  const d = raw as Record<string, unknown>;
  const bad = (f: string) => new Error(`Réponse de décision invalide (${f})`);
  if (!d || typeof d !== "object") throw bad("corps");
  if (typeof d.symbol !== "string") throw bad("symbol");
  if (d.kind !== "crypto" && d.kind !== "stock") throw bad("kind");
  if (!VERDICTS.includes(d.verdict as Verdict)) throw bad("verdict");
  if (!LEVELS.includes(d.level as Level)) throw bad("level");
  if (typeof d.confidence !== "number") throw bad("confidence");
  if (typeof d.asOf !== "number") throw bad("asOf");
  for (const f of ["families", "vetoes", "whyWait", "toBuy", "toSell", "scenarios", "pros", "cons", "sources"]) {
    if (!Array.isArray(d[f])) throw bad(f);
  }
  const setup = d.setup as Setup | undefined;
  if (!setup || !Array.isArray(setup.steps)) throw bad("setup");
  const wn = d.whyNot as WhyNot | undefined;
  if (!wn || !Array.isArray(wn.risks) || !Array.isArray(wn.invalidation)) throw bad("whyNot");
  const f = d.fundamentals as { kind?: string } | null | undefined;
  if (f != null && f.kind !== "stock" && f.kind !== "crypto") throw bad("fundamentals.kind");
  const p = d.position as Position | null | undefined;
  if (p != null && !Array.isArray(p.exits)) throw bad("position.exits");
  if (typeof d.disclaimer !== "string" || !d.disclaimer) throw bad("disclaimer");
  // Added fields: optional, checked when present.
  if (d.rating != null && !RATINGS.includes(d.rating as Rating)) throw bad("rating");
  const sc = d.score as CompositeScore | null | undefined;
  if (sc != null && !Array.isArray(sc.factors)) throw bad("score.factors");
  const dg = d.degraded as Degraded | null | undefined;
  if (dg != null && !Array.isArray(dg.reasons)) throw bad("degraded.reasons");
  const st = d.structure as Structure | null | undefined;
  if (st != null && (!Array.isArray(st.levels) || !Array.isArray(st.relative))) throw bad("structure");
  if (d.events != null && !Array.isArray(d.events)) throw bad("events");
  const nt = d.noTrade as NoTrade | null | undefined;
  if (nt != null && (!Array.isArray(nt.reasons) || !Array.isArray(nt.unchecked))) throw bad("noTrade");
  const az = d.actionZones as ActionZones | null | undefined;
  if (az != null && (!Array.isArray(az.zones) || typeof az.price !== "number")) throw bad("actionZones");
  const ca = d.counterArgument as CounterArgument | null | undefined;
  if (ca != null && !Array.isArray(ca.invalidators)) throw bad("counterArgument");
  const sn = d.snapshot as DecisionSnapshot | null | undefined;
  if (sn != null && !Array.isArray(sn.families)) throw bad("snapshot");
  const me = d.modelEvidence as ModelEvidence | null | undefined;
  if (me != null && (typeof me.available !== "boolean" || typeof me.text !== "string")) throw bad("modelEvidence");
  if (d.bot != null && !isBotView(d.bot)) throw bad("bot");
  return d as unknown as Decision;
}

// ---------- Personal mode: what is sent (never quantities nor amounts) ----------

export type WeightInput = { symbol: string; kind: Kind; quantity: number };
export type Weight = { symbol: string; kind: Kind; weight: number };

/**
 * Share of each line in the total portfolio value (lines with a known price + cash), %, rounded to 0.1,
 * largest first, at most `max` (the server accepts 20). Lines without a price are left out.
 */
export function portfolioWeights(holdings: WeightInput[], prices: Record<string, number>, cash = 0, max = 20): Weight[] {
  const valued = holdings.flatMap((h) => {
    const p = prices[`${h.kind}:${h.symbol}`];
    return p != null && Number.isFinite(p) && p > 0 && h.quantity > 0 ? [{ symbol: h.symbol, kind: h.kind, value: h.quantity * p }] : [];
  });
  // Same asset on two lines (e.g. two brokers): one weight.
  const merged = new Map<string, { symbol: string; kind: Kind; value: number }>();
  for (const v of valued) {
    const k = `${v.kind}:${v.symbol}`;
    const m = merged.get(k);
    merged.set(k, m ? { ...m, value: m.value + v.value } : v);
  }
  const total = [...merged.values()].reduce((a, v) => a + v.value, 0) + (Number.isFinite(cash) && cash > 0 ? cash : 0);
  if (total <= 0) return [];
  return [...merged.values()]
    .map((v) => ({ symbol: v.symbol, kind: v.kind, weight: Math.round((v.value / total) * 1000) / 10 }))
    .filter((w) => w.weight > 0)
    .sort((a, b) => b.weight - a.weight || a.symbol.localeCompare(b.symbol))
    .slice(0, max);
}

/** Average cost of an asset over all its lines (weighted by quantity); null when not held. */
export function averageCost(holdings: (WeightInput & { averagePrice: number })[], symbol: string, kind: Kind): number | null {
  const lines = holdings.filter((h) => h.symbol === symbol && h.kind === kind && h.quantity > 0);
  const q = lines.reduce((a, h) => a + h.quantity, 0);
  if (!q) return null;
  const c = lines.reduce((a, h) => a + h.quantity * h.averagePrice, 0) / q;
  return c > 0 ? c : null;
}

/** "BTC:crypto:35.2,AAPL:stock:10" */
export const weightsParam = (w: Weight[]) => w.map((x) => `${x.symbol}:${x.kind}:${x.weight}`).join(",");

export type PersonalInput = { cost: number | null; weights: Weight[] };

// ---------- Composite score weights (Réglages, sent as w=) ----------

export type ScoreWeights = { tech: number; mom: number; fund: number; sent: number; news: number; macro: number };
/** Same factors and default weights as the server (backend/src/engine/synthesis.rs). */
export const SCORE_FACTORS: { key: keyof ScoreWeights; label: string; hint: string; def: number }[] = [
  { key: "tech", label: "Technique", hint: "tendance, volume, volatilité, structure", def: 32 },
  { key: "mom", label: "Momentum", hint: "MACD, RSI, variation sur 1 mois", def: 18 },
  { key: "fund", label: "Fondamentaux", hint: "valorisation, comptes ou réseau", def: 20 },
  { key: "sent", label: "Sentiment", hint: "Fear & Greed, financement, StockTwits", def: 10 },
  { key: "news", label: "Actualités", hint: "ton des titres sur 24 h", def: 10 },
  { key: "macro", label: "Macro", hint: "stress des marchés, VIX, taux", def: 10 },
];
export const DEFAULT_SCORE_WEIGHTS = Object.fromEntries(SCORE_FACTORS.map((f) => [f.key, f.def])) as ScoreWeights;

/** Whole numbers 0 – 100 (a bad or missing value takes its default); all at 0 → the defaults. */
export function sanitizeScoreWeights(raw: unknown): ScoreWeights {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const w = Object.fromEntries(SCORE_FACTORS.map((f) => {
    const v = r[f.key];
    return [f.key, typeof v === "number" && Number.isFinite(v) ? Math.min(100, Math.max(0, Math.round(v))) : f.def];
  })) as ScoreWeights;
  return SCORE_FACTORS.some((f) => w[f.key] > 0) ? w : { ...DEFAULT_SCORE_WEIGHTS };
}

/** "tech:40,mom:18,…", or null for the default weights (the URL stays the same). */
export function scoreWeightsParam(w: ScoreWeights | null | undefined): string | null {
  if (!w) return null;
  const s = sanitizeScoreWeights(w);
  if (SCORE_FACTORS.every((f) => s[f.key] === f.def)) return null;
  return SCORE_FACTORS.map((f) => `${f.key}:${s[f.key]}`).join(",");
}

/** URL of the endpoint; personal inputs only when given (average cost rounded to the cent, weights), score weights when not the defaults. */
export function decisionUrl(symbol: string, kind: Kind, personal?: PersonalInput | null, scoreWeights?: ScoreWeights | null): string {
  let u = `/api/decision?symbol=${encodeURIComponent(symbol)}&kind=${kind}`;
  if (personal?.cost != null && Number.isFinite(personal.cost) && personal.cost > 0) u += `&cost=${Math.round(personal.cost * 100) / 100}`;
  if (personal?.weights.length) u += `&weights=${encodeURIComponent(weightsParam(personal.weights))}`;
  const w = scoreWeightsParam(scoreWeights);
  if (w) u += `&w=${encodeURIComponent(w)}`;
  return u;
}

// ---------- French formatting (fr-FR, narrow no-break spaces) ----------

export const NNBSP = " ";
const fr = (v: number, min: number, max: number) => v.toLocaleString("fr-FR", { minimumFractionDigits: min, maximumFractionDigits: max });

/** A dollar price in the display currency (money.ts): no decimals when whole, else 2 (4 or 8 below 1). */
export function usd(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return "—";
  const x = toDisplay(v);
  const a = Math.abs(x);
  const digits = a >= 1 ? (Number.isInteger(x) ? 0 : 2) : a >= 0.01 ? 4 : 8;
  return `${fr(x, digits, digits)}${NNBSP}${currencySymbol()}`;
}

/** Large dollar amounts in the display currency: 421 Md€, 3,16 Md€, 850 M€. */
export function usdCompact(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return "—";
  return moneyCompact(v, NNBSP);
}

/** Percentage; `sign` adds + for positive values (− is the typographic minus). */
export function pct(v: number | null | undefined, digits = 1, sign = false): string {
  if (v == null || !Number.isFinite(v)) return "—";
  const s = v < 0 ? "−" : sign && v > 0 ? "+" : "";
  return `${s}${fr(Math.abs(v), 0, digits)}${NNBSP}%`;
}

/** Plain number (ratios, counts). */
export function num(v: number | null | undefined, digits = 2): string {
  if (v == null || !Number.isFinite(v)) return "—";
  return `${v < 0 ? "−" : ""}${fr(Math.abs(v), 0, digits)}`;
}

/** Large counts: 19,93 millions. */
export function count(v: number | null | undefined): string {
  if (v == null || !Number.isFinite(v)) return "—";
  if (v >= 1e9) return `${fr(v / 1e9, 0, 2)} milliards`;
  if (v >= 1e6) return `${fr(v / 1e6, 0, 2)} millions`;
  return fr(v, 0, 0);
}

/** Hash rate (H/s) in EH/s. */
export const hashRate = (v: number | null | undefined) => (v == null || !Number.isFinite(v) ? "—" : `${fr(v / 1e18, 0, 0)}${NNBSP}EH/s`);

/** "28 septembre 2026" (Paris time). */
export const longDate = (ms: number) => new Date(ms).toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric", timeZone: "Europe/Paris" });
/** "28/09 à 14:02" (Paris time). */
export const shortDateTime = (ms: number) =>
  `${new Date(ms).toLocaleDateString("fr-FR", { day: "2-digit", month: "2-digit", timeZone: "Europe/Paris" })} à ${new Date(ms).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit", timeZone: "Europe/Paris" })}`;
/** Earnings date: New York calendar day (a timestamp at 00:00 UTC is a bare date, read as such). */
export const nyDate = (ms: number) =>
  new Date(ms).toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric", timeZone: ms % 86_400_000 === 0 ? "UTC" : "America/New_York" });

// ---------- Labels (colour never alone: an icon and a word each time) ----------

export const LEVEL_UI: Record<Level, { icon: string; label: string }> = {
  strong: { icon: "🟢", label: "Signal fort" },
  moderate: { icon: "🟡", label: "Signal modéré" },
  waiting: { icon: "⚪", label: "Attente" },
  highRisk: { icon: "🟠", label: "Risque élevé" },
  exit: { icon: "🔴", label: "Sortie / risque d'invalidation" },
};

/** 6-level rating: an icon and a word, with the colour of the matching level. */
export const RATING_UI: Record<Rating, { icon: string; label: string; tone: Level }> = {
  strongBuy: { icon: "🟢", label: "ACHAT FORT", tone: "strong" },
  buy: { icon: "🟢", label: "ACHAT", tone: "strong" },
  hold: { icon: "⚪", label: "ATTENDRE", tone: "waiting" },
  reduce: { icon: "🟠", label: "ALLÉGER", tone: "highRisk" },
  sell: { icon: "🔴", label: "VENDRE", tone: "exit" },
  strongSell: { icon: "🔴", label: "VENTE FORTE", tone: "exit" },
};

export const BIAS_UI: Record<Bias, { icon: string; label: string }> = {
  bullish: { icon: "↗", label: "haussier" },
  neutral: { icon: "→", label: "neutre" },
  bearish: { icon: "↘", label: "baissier" },
};
export const REGIME_UI: Record<RegimeKind, { icon: string; label: string }> = {
  riskOn: { icon: "🟢", label: "Risk-on (appétit pour le risque)" },
  neutral: { icon: "⚪", label: "Neutre" },
  riskOff: { icon: "🔴", label: "Risk-off (aversion au risque)" },
};

/** "+34", "−12", "0" (scores −100 … +100). */
export const signedScore = (v: number) => `${v > 0 ? "+" : v < 0 ? "−" : ""}${Math.abs(Math.round(v))}`;

export const VERDICT_LABEL: Record<Verdict, string> = {
  buy: "ACHETER", buyZone: "ZONE D'ACHAT", wait: "ATTENDRE", noPosition: "AUCUNE POSITION", trim: "ALLÉGER", sell: "VENDRE",
};

export type Tone = "strong" | "moderate" | "highRisk" | "exit" | "na";
/** Family status → tone, icon and word. A negative score beyond −50 is "très défavorable". */
export function familyTone(f: Pick<Family, "status" | "score">): { tone: Tone; icon: string; text: string } {
  switch (f.status) {
    case "positive": return { tone: "strong", icon: "🟢", text: "favorable" };
    case "neutral": return { tone: "moderate", icon: "🟡", text: "neutre" };
    case "negative": return f.score != null && f.score <= -50
      ? { tone: "exit", icon: "🔴", text: "très défavorable" }
      : { tone: "highRisk", icon: "🟠", text: "défavorable" };
    default: return { tone: "na", icon: "⚪", text: "non disponible" };
  }
}

/** The summary row of the mock: Tendance, Momentum, Volume, Fondamentaux, Macro, Risque, Valorisation (when present). */
const SUMMARY_KEYS: [string, string][] = [
  ["trend", "Tendance"], ["momentum", "Momentum"], ["volume", "Volume"], ["fundamentals", "Fondamentaux"],
  ["macro", "Macro"], ["volatility", "Risque"], ["valuation", "Valorisation"],
];
export function summaryFamilies(families: Family[]): { key: string; name: string; family: Family }[] {
  return SUMMARY_KEYS.flatMap(([key, name]) => {
    const family = families.find((f) => f.key === key);
    return family ? [{ key, name, family }] : [];
  });
}

/** Active vetoes first, then the checks that pass, then those that cannot be verified. */
export function sortVetoes(v: Veto[]): Veto[] {
  const rank = (x: Veto) => (x.active ? 0 : x.verifiable ? 1 : 2);
  return [...v].sort((a, b) => rank(a) - rank(b));
}

export const STEP_UI: Record<StepState, { icon: string; label: string }> = {
  ok: { icon: "✓", label: "fait" },
  no: { icon: "✕", label: "pas encore" },
  unknown: { icon: "?", label: "inconnu" },
};
export const SCENARIO_UI: Record<ScenarioKind, { icon: string; label: string }> = {
  bull: { icon: "↗", label: "Haussier" },
  neutral: { icon: "→", label: "Neutre" },
  bear: { icon: "↘", label: "Baissier" },
};
export const UNCERTAINTY_LABEL: Record<Uncertainty, string> = { low: "faible", medium: "moyenne", high: "élevée" };
export const EXIT_KIND_LABEL: Record<ExitKind, string> = { profit: "Prise de bénéfices", defensive: "Défensive", macro: "Choc de marché" };

/** "Vendre 20 % si Objectif 1 atteint (338 $)" */
export const exitText = (e: Exit) => `Vendre ${num(e.share, 0)}${NNBSP}% si ${e.trigger.charAt(0).toLowerCase()}${e.trigger.slice(1)}`;

/** "1,2 (minimum 2)" */
export const riskRewardText = (p: Plan) => `${num(p.riskReward, 1)} (minimum ${num(p.minRiskReward, 1)})`;

export const MODE_TEXT = {
  personal: "Mode personnel : calculé avec votre prix d'achat et vos pondérations, jamais conservés",
  informational: "Mode informationnel : données de marché uniquement, sans votre position",
} as const;
export const modeText = (mode: string) => (mode === "personal" ? MODE_TEXT.personal : MODE_TEXT.informational);

// ---------- Offline cache (last good decision per asset, in this browser only) ----------

const CACHE_KEY = "altim.decision.v1";
const CACHE_MAX = 40;
export type CachedDecision = { at: number; personal: boolean; decision: Decision };

type Storage = Pick<globalThis.Storage, "getItem" | "setItem">;
const store = (): Storage | null => {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
};

function readAll(s: Storage | null = store()): Record<string, CachedDecision> {
  try {
    const raw = s?.getItem(CACHE_KEY);
    const p = raw ? (JSON.parse(raw) as Record<string, CachedDecision>) : {};
    return p && typeof p === "object" ? p : {};
  } catch {
    return {};
  }
}

export function cacheDecision(d: Decision, personal: boolean, now = Date.now(), s: Storage | null = store()) {
  if (!s) return;
  const all = readAll(s);
  all[`${d.kind}:${d.symbol}`] = { at: now, personal, decision: d };
  recordConfiguration(d, personal, now, s);
  const keep = Object.entries(all).sort((a, b) => b[1].at - a[1].at).slice(0, CACHE_MAX);
  try {
    s.setItem(CACHE_KEY, JSON.stringify(Object.fromEntries(keep)));
  } catch {
    /* quota or private browsing: not cached */
  }
}

export function cachedDecision(kind: Kind, symbol: string, s: Storage | null = store()): CachedDecision | null {
  const c = readAll(s)[`${kind}:${symbol}`];
  try {
    return c ? { ...c, decision: parseDecision(c.decision) } : null;
  } catch {
    return null;
  }
}

/** Verdict seen on the asset page less than `maxAgeMs` ago (Radar badge, no extra request). */
export function recentVerdict(kind: Kind, symbol: string, now = Date.now(), maxAgeMs = 12 * 3_600_000, s: Storage | null = store()) {
  const c = cachedDecision(kind, symbol, s);
  return c && now - c.at <= maxAgeMs ? { verdict: c.decision.verdict, label: c.decision.label, level: c.decision.level, at: c.at } : null;
}
