/**
 * Market guard: what a short-term bot cannot see on its own.
 *
 * 1. Regime   — the background trend (daily + 4 h), for the long-term bot.
 * 2. Shock    — abnormal volatility, price jumps, volume spikes, compression before a breakout, news bursts,
 *               market-wide fear (VIX): when to reduce or pause short-term trading.
 * 3. Reversal — a move AGAINST the current trend: technical exhaustion (RSI divergences, over-extension,
 *               rejection candles), crowd positioning (funding, long/short ratio, open interest), sentiment
 *               extremes (Fear & Greed, StockTwits) and news tone.
 *
 * Nothing predicts a true surprise. This measures the conditions under which large or counter-trend moves are
 * more likely, and every technical score is calibrated on the asset's own history (how often it was followed by
 * an adverse move). Same rules as Guard.swift.
 */
import { adx, atr, ema, rsi, sanitize, type Candle } from "./signal";
import type { Kind } from "./reliability";

export type Trend = "up" | "down" | "range";
export type ShockLevel = "calm" | "agitated" | "shock";

export interface Positioning {
  /** Current perpetual funding rate per 8 h, as a fraction (0.0001 = 0.01 %). */
  fundingRate?: number | null;
  /** Long/short account ratio, oldest first. */
  longShortRatio?: number[];
  /** Open interest (USD), hourly, oldest first. */
  openInterest?: number[];
}

export interface SentimentInput {
  /** Fear & Greed index, oldest first. */
  fearGreed?: number[];
  /** Share of bullish messages (StockTwits), 0-100. */
  socialBullish?: number | null;
  socialSample?: number;
}

export interface NewsItem { title: string; time: number; source?: string }

export interface GuardInput {
  kind: Kind;
  daily: Candle[];
  h4: Candle[];
  h1: Candle[];
  positioning?: Positioning | null;
  sentiment?: SentimentInput | null;
  news?: NewsItem[] | null;
  /** VIX daily closes, oldest first (stocks). */
  vix?: number[] | null;
  now?: number;
}

/**
 * How a factor did on the asset's own history: share of past occurrences followed by the event it warns about
 * (large move for shock factors, counter-trend move for reversal factors), against the usual share.
 */
export interface Evidence { samples: number; rate: number; base: number; lift: number }

/**
 * verified: worked on this asset (counted, up to full points at lift ≥ 1.5) · unproven: too few past cases (half) ·
 * rejected: present but never predictive here (not counted) · unverifiable: no history available (news, positioning…).
 */
export type FactorStatus = "verified" | "unproven" | "rejected" | "unverifiable";

export interface GuardFactor {
  code: string;
  /** Points actually counted (base points × weight from the evidence). */
  points: number;
  basePoints: number;
  text: string;
  status: FactorStatus;
  evidence?: Evidence | null;
}

export interface GuardResult {
  regime: { trend: Trend; strength: number; text: string };
  shock: { score: number; level: ShockLevel; factors: GuardFactor[] };
  reversal: { score: number; direction: "down" | "up" | null; factors: GuardFactor[] };
  policy: { scalping: "ok" | "reduce" | "pause"; sizeMultiplier: number; stopMultiplier: number; notes: string[] };
}

// ---------- Thresholds (identical in Guard.swift) ----------

export const GUARD = {
  shockAgitated: 35,
  shockLevel: 65,
  reversalHigh: 50,
  /** Minimum past occurrences for a factor's evidence to be trusted. */
  minSamples: 20,
  /** Weight of a factor with no history available. */
  unverifiableWeight: 0.75,
  fundingHot: 0.0003,
  fundingVeryHot: 0.0006,
  fundingCold: -0.0001,
  fundingVeryCold: -0.0003,
};

const last = <T>(a: T[]) => a[a.length - 1];
const lastValue = (s: (number | null)[]) => {
  for (let i = s.length - 1; i >= 0; i--) if (s[i] !== null && s[i] !== undefined) return s[i]!;
  return null;
};
const mean = (v: number[]) => (v.length ? v.reduce((a, b) => a + b, 0) / v.length : 0);
const std = (v: number[]) => {
  if (v.length < 2) return 0;
  const m = mean(v);
  return Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (v.length - 1));
};
const pct = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;
const one = (v: number) => v.toLocaleString("fr-FR", { maximumFractionDigits: 1 });
/** Percentile rank (0-100) of the last value among all values. */
export function percentileRank(values: number[], x: number): number {
  if (!values.length) return 50;
  return (values.filter((v) => v < x).length + 0.5 * values.filter((v) => v === x).length) / values.length * 100;
}

// ---------- 1. Regime ----------

export function regime(dailyRaw: Candle[], h4Raw: Candle[]): GuardResult["regime"] {
  const d = sanitize(dailyRaw);
  const closes = d.map((c) => c.close);
  if (closes.length < 60) return { trend: "range", strength: 0, text: "Historique journalier insuffisant pour juger la tendance de fond." };
  const long = closes.length >= 200 ? 200 : 100;
  const eL = ema(closes, long), e50 = ema(closes, 50);
  const close = last(closes)!;
  const l = lastValue(eL) ?? lastValue(e50)!;
  const m = lastValue(e50)!;
  const m10 = e50[e50.length - 11] ?? m;
  const slope = m10 ? ((m - m10) / m10) * 100 : 0;
  const a = lastValue(adx(d)) ?? 0;
  let trend: Trend = "range";
  if (close > l && m > l && slope > 0) trend = "up";
  else if (close < l && m < l && slope < 0) trend = "down";
  // 4 h confirmation: EMA 20 vs EMA 50 in the same direction.
  const h4 = sanitize(h4Raw).map((c) => c.close);
  const f20 = lastValue(ema(h4, 20)), f50 = lastValue(ema(h4, 50));
  const aligned = f20 !== null && f50 !== null && ((trend === "up" && f20 > f50) || (trend === "down" && f20 < f50));
  const strength = trend === "range" ? Math.round(Math.min(a, 40)) : Math.round(Math.min(100, a * 2.5 + (aligned ? 10 : 0)));
  const text = trend === "range"
    ? `Pas de tendance de fond nette (prix ${close > l ? "au-dessus" : "en dessous"} de la moyenne ${long} jours, ADX ${Math.round(a)}) : marché sans direction.`
    : `Tendance de fond ${trend === "up" ? "haussière" : "baissière"} (moyennes 50 et ${long} jours alignées, ADX ${Math.round(a)})${aligned ? ", confirmée en 4 h" : ", pas encore confirmée en 4 h"}.`;
  return { trend, strength, text };
}

// ---------- News tone ----------

const NEGATIVE = [
  "hack", "hacked", "exploit", "breach", "stolen", "lawsuit", "sues", "sued", "sec charges", "investigation", "probe", "fraud",
  "bankrupt", "bankruptcy", "insolvency", "liquidation", "liquidated", "delist", "ban", "banned", "crackdown", "crash", "plunge",
  "plunges", "tumble", "tumbles", "sell-off", "selloff", "downgrade", "downgraded", "misses", "miss estimates", "cuts guidance",
  "layoffs", "recall", "halt", "halted", "outage", "default", "warning", "subpoena", "indictment", "sanction",
  "piratage", "faillite", "enquête", "plainte", "effondrement", "chute", "interdiction", "fraude",
];
const POSITIVE = [
  "approval", "approved", "approves", "etf inflows", "record high", "all-time high", "surge", "surges", "soars", "rally",
  "rallies", "upgrade", "upgraded", "beats", "beat estimates", "raises guidance", "buyback", "partnership", "adoption",
  "launch", "launches", "breakthrough", "acquisition", "wins",
  "approbation", "hausse", "partenariat", "rachat",
];

/** Headline tone: count of negative and positive headlines (a headline counts once). */
export function newsTone(items: NewsItem[]): { negative: number; positive: number } {
  let negative = 0, positive = 0;
  for (const it of items) {
    const t = ` ${it.title.toLowerCase()} `;
    const has = (words: string[]) => words.some((w) => new RegExp(`[^a-zà-ü]${w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}[^a-zà-ü]`).test(t));
    const neg = has(NEGATIVE), pos = has(POSITIVE);
    if (neg && !pos) negative++;
    else if (pos && !neg) positive++;
  }
  return { negative, positive };
}

// ---------- Evidence (self-validation on the asset's own history) ----------

/** Weight of a factor from its evidence (same rule in Guard.swift). */
export function weigh(e: Evidence | null | undefined, historical: boolean): { weight: number; status: FactorStatus } {
  if (!historical) return { weight: GUARD.unverifiableWeight, status: "unverifiable" };
  if (!e || e.samples < GUARD.minSamples) return { weight: 0.5, status: "unproven" };
  if (e.lift < 1.1) return { weight: 0, status: "rejected" };
  return { weight: Math.min(1, Math.max(0.2, (e.lift - 1) / 0.5)), status: "verified" };
}

type Raw = { code: string; points: number; text: string };

function finalize(raw: Raw[], evidence: Record<string, Evidence>, historical: (code: string) => boolean): GuardFactor[] {
  return raw.map((f) => {
    const e = evidence[f.code] ?? null;
    const { weight, status } = weigh(e, historical(f.code));
    return { code: f.code, basePoints: f.points, points: Math.round(f.points * weight), text: f.text, status, evidence: e };
  });
}

function tally(stats: Map<string, { n: number; hit: number }>, key: string, hit: boolean) {
  const s = stats.get(key) ?? { n: 0, hit: 0 };
  s.n++;
  if (hit) s.hit++;
  stats.set(key, s);
}

function toEvidence(stats: Map<string, { n: number; hit: number }>): Record<string, Evidence> {
  const base = stats.get("_base");
  const out: Record<string, Evidence> = {};
  if (!base?.n) return out;
  const b = (base.hit / base.n) * 100;
  for (const [k, v] of stats) {
    if (k === "_base") continue;
    const rate = (v.hit / v.n) * 100;
    out[k] = { samples: v.n, rate, base: b, lift: b > 0 ? rate / b : 0 };
  }
  return out;
}

// ---------- 2. Shock ----------

/** Hourly factors at candle i (returns r[j] = log(close j+1 / close j)), no look-ahead. */
export function hourlyShockFactors(h1: Candle[], r: number[], i: number): Raw[] {
  const f: Raw[] = [];
  if (i < 96) return f;
  const base = r.slice(Math.max(0, i - 120), i - 24), recent = r.slice(i - 24, i);
  const sLong = std(base), sNow = std(recent);
  const ratio = sLong > 0 ? sNow / sLong : 1;
  if (ratio >= 2) f.push({ code: "vol2", points: 35, text: `Volatilité des dernières 24 h ${one(ratio)} fois supérieure à la normale.` });
  else if (ratio >= 1.5) f.push({ code: "vol15", points: 20, text: `Volatilité des dernières 24 h ${one(ratio)} fois supérieure à la normale.` });
  const jump = sLong > 0 ? Math.max(...r.slice(i - 6, i).map((x) => Math.abs(x))) / sLong : 0;
  if (jump >= 4) f.push({ code: "jump4", points: 35, text: `Saut de prix de ${one(jump)} écarts-types en une heure (mouvement anormal).` });
  else if (jump >= 3) f.push({ code: "jump3", points: 20, text: `Mouvement horaire de ${one(jump)} écarts-types, inhabituel.` });
  const vols = h1.slice(Math.max(0, i - 100), i - 2).map((c) => c.volume);
  const vNow = mean(h1.slice(i - 2, i + 1).map((c) => c.volume));
  const z = std(vols) > 0 ? (vNow - mean(vols)) / std(vols) : 0;
  if (z >= 3) f.push({ code: "volume", points: 15, text: `Volume des 3 dernières heures très au-dessus de la normale (${one(z)} écarts-types).` });
  return f;
}

/** Bollinger bandwidth (20, 2σ) of 4 h closes, relative to the mean. */
function bandwidth(closes: number[]): (number | null)[] {
  return closes.map((_, i) => {
    if (i < 19) return null;
    const w = closes.slice(i - 19, i + 1), m = mean(w);
    return m > 0 ? (4 * std(w)) / m : null;
  });
}

function squeezeFactor(bw: (number | null)[], i: number): Raw[] {
  const now = bw[i];
  const hist = bw.slice(Math.max(0, i - 200), i).filter((x): x is number => x !== null);
  if (now === null || now === undefined || hist.length < 100) return [];
  const rank = percentileRank(hist, now);
  return rank <= 10
    ? [{ code: "squeeze", points: 15, text: `Volatilité anormalement comprimée (bandes de Bollinger 4 h plus étroites que ${Math.round(100 - rank)} % du temps) : un mouvement brutal peut suivre, sans direction connue.` }]
    : [];
}

/**
 * Evidence of the shock factors on the asset's history. Hourly factors: followed by a move of 3 standard
 * deviations within the next 6 hours? Squeeze: followed by a 6-candle (24 h) range of 4 ATR?
 */
export function shockEvidence(h1Raw: Candle[], h4Raw: Candle[]): Record<string, Evidence> {
  const out: Record<string, Evidence> = {};
  const h1 = sanitize(h1Raw);
  const r = h1.slice(1).map((c, i) => Math.log(c.close / h1[i]!.close));
  const s1 = new Map<string, { n: number; hit: number }>();
  for (let i = 96; i < r.length - 6; i++) {
    const sLong = std(r.slice(Math.max(0, i - 120), i - 24));
    if (!(sLong > 0)) continue;
    const hit = Math.max(...r.slice(i, i + 6).map((x) => Math.abs(x))) >= 3 * sLong;
    tally(s1, "_base", hit);
    for (const f of hourlyShockFactors(h1, r, i)) tally(s1, f.code, hit);
  }
  Object.assign(out, toEvidence(s1));
  const h4 = sanitize(h4Raw);
  const bw = bandwidth(h4.map((c) => c.close)), a = atr(h4);
  const s4 = new Map<string, { n: number; hit: number }>();
  for (let i = 120; i < h4.length - 6; i++) {
    const at = a[i];
    if (!at) continue;
    const next = h4.slice(i + 1, i + 7);
    const hit = Math.max(...next.map((c) => c.high)) - Math.min(...next.map((c) => c.low)) >= 4 * at;
    tally(s4, "_base", hit);
    for (const f of squeezeFactor(bw, i)) tally(s4, f.code, hit);
  }
  const e4 = toEvidence(s4);
  if (e4.squeeze) out.squeeze = e4.squeeze;
  return out;
}

export function shock(input: GuardInput): GuardResult["shock"] {
  const h1 = sanitize(input.h1), h4 = sanitize(input.h4);
  const r = h1.slice(1).map((c, i) => Math.log(c.close / h1[i]!.close));
  const raw: Raw[] = [];
  if (r.length >= 96) raw.push(...hourlyShockFactors(h1, r, r.length));
  if (h4.length >= 120) {
    const bw = bandwidth(h4.map((c) => c.close));
    raw.push(...squeezeFactor(bw, bw.length - 1));
  }
  // News burst: headlines of the last 6 h vs the average 6 h of the week.
  const now = input.now ?? Date.now();
  const news = (input.news ?? []).filter((n) => n.time <= now && n.time >= now - 7 * 86_400_000);
  if (news.length >= 5) {
    const recent = news.filter((n) => n.time >= now - 6 * 3_600_000).length;
    const avg = news.length / 28;
    const burst = avg > 0 ? recent / avg : 0;
    if (burst >= 3 && recent >= 4) raw.push({ code: "newsBurst", points: 20, text: `Rafale d'actualités : ${recent} articles en 6 h, ${one(burst)} fois plus que d'habitude.` });
    else if (burst >= 2 && recent >= 3) raw.push({ code: "newsBusy", points: 10, text: `Actualité plus chargée que d'habitude (${recent} articles en 6 h).` });
  }
  // Market-wide fear (stocks): VIX.
  const vix = input.vix ?? [];
  if (input.kind === "stock" && vix.length >= 2) {
    const v = last(vix)!, prev = vix[vix.length - 2]!;
    if (v >= 30) raw.push({ code: "vixHigh", points: 20, text: `Peur généralisée sur les marchés (VIX à ${one(v)}).` });
    if (prev > 0 && v / prev - 1 >= 0.2) raw.push({ code: "vixJump", points: 15, text: `Le VIX a bondi de ${pct((v / prev - 1) * 100)} en une séance.` });
  }
  const historical = new Set(["vol2", "vol15", "jump4", "jump3", "volume", "squeeze"]);
  const factors = finalize(raw, raw.length ? shockEvidence(h1, h4) : {}, (c) => historical.has(c));
  const score = Math.min(100, factors.reduce((acc, f) => acc + f.points, 0));
  const level: ShockLevel = score >= GUARD.shockLevel ? "shock" : score >= GUARD.shockAgitated ? "agitated" : "calm";
  return { score, level, factors };
}

// ---------- 3. Reversal ----------

/** Swing highs / lows: index whose value is the extreme of `k` bars on each side. */
export function pivots(values: number[], kind: "high" | "low", k = 3, until = values.length - 1): number[] {
  const out: number[] = [];
  for (let i = k; i <= until - k; i++) {
    let ok = true;
    for (let j = i - k; j <= i + k && ok; j++) {
      if (j === i) continue;
      ok = kind === "high" ? values[i]! > values[j]! : values[i]! < values[j]!;
    }
    if (ok) out.push(i);
  }
  return out;
}

/**
 * Regular divergence on the last two swings (within `window` bars): price makes a higher high while RSI makes a
 * lower high (bearish), or a lower low while RSI makes a higher low (bullish).
 */
export function divergence(c: Candle[], rsiSeries: (number | null)[], direction: "down" | "up", until = c.length - 1, window = 60): boolean {
  const from = Math.max(0, until - window);
  const values = direction === "down" ? c.map((x) => x.high) : c.map((x) => x.low);
  const p = pivots(values, direction === "down" ? "high" : "low", 3, until).filter((i) => i >= from);
  if (p.length < 2) return false;
  const [a, b] = [p[p.length - 2]!, p[p.length - 1]!];
  const ra = rsiSeries[a], rb = rsiSeries[b];
  if (ra === null || rb === null || ra === undefined || rb === undefined) return false;
  return direction === "down" ? values[b]! > values[a]! && rb < ra - 3 : values[b]! < values[a]! && rb > ra + 3;
}

/** Technical reversal factors at candle i (data up to i only), on 4 h or daily candles. */
export function technicalReversal(
  c: Candle[], r: (number | null)[], e20: (number | null)[], a: (number | null)[], i: number, direction: "down" | "up", tf: "4h" | "1d",
): Raw[] {
  const f: Raw[] = [];
  const up = direction === "down"; // reversing an up move
  const label = tf === "4h" ? "4 h" : "journalier";
  const [hi, lo] = tf === "4h" ? [80, 20] : [75, 25];
  const rv = r[i];
  if (rv !== null && rv !== undefined && (up ? rv > hi : rv < lo)) f.push({ code: `rsi${tf}`, points: tf === "4h" ? 10 : 15, text: `RSI ${label} extrême (${Math.round(rv)}).` });
  if (divergence(c, r, direction, i)) f.push({ code: `div${tf}`, points: 20, text: `Divergence ${up ? "baissière" : "haussière"} en ${label} : le prix fait un nouveau ${up ? "sommet" : "creux"} mais pas le RSI (essoufflement).` });
  const m = e20[i], at = a[i];
  if (m !== null && m !== undefined && at && at > 0) {
    const ext = (c[i]!.close - m) / at;
    if (up ? ext > 3 : ext < -3) f.push({ code: `extension${tf}`, points: 15, text: `Prix très éloigné de sa moyenne 20 périodes en ${label} (${one(Math.abs(ext))} ATR).` });
  }
  const k = c[i]!;
  const body = Math.abs(k.close - k.open) || k.close * 1e-6;
  const wick = up ? k.high - Math.max(k.open, k.close) : Math.min(k.open, k.close) - k.low;
  const vols = c.slice(Math.max(0, i - 50), i).map((x) => x.volume);
  const z = std(vols) > 0 ? (k.volume - mean(vols)) / std(vols) : 0;
  if (wick > 2 * body && z > 2) f.push({ code: `rejection${tf}`, points: 10, text: `Bougie de rejet sur fort volume en ${label} (longue mèche ${up ? "haute" : "basse"}).` });
  return f;
}

/**
 * Evidence of the technical reversal factors on the asset's history: at each past candle, direction = against the
 * short trend (EMA 20 vs EMA 50); event = counter-trend move of 3 ATR within the horizon (18 × 4 h = 3 days,
 * 5 days in daily). Same factors and rules as live, no look-ahead.
 */
export function reversalEvidence(raw: Candle[], tf: "4h" | "1d"): Record<string, Evidence> {
  const c = sanitize(raw);
  const horizon = tf === "4h" ? 18 : 5;
  if (c.length < 100) return {};
  const closes = c.map((x) => x.close);
  const r = rsi(closes), e20 = ema(closes, 20), e50 = ema(closes, 50), a = atr(c);
  const stats = new Map<string, { n: number; hit: number }>();
  for (let i = 60; i < c.length - horizon; i++) {
    const m20 = e20[i], m50 = e50[i], at = a[i];
    if (m20 === null || m50 === null || m20 === undefined || m50 === undefined || !at) continue;
    const direction = m20 > m50 ? "down" : "up";
    const future = c.slice(i + 1, i + 1 + horizon);
    const hit = direction === "down"
      ? Math.min(...future.map((x) => x.low)) <= c[i]!.close - 3 * at
      : Math.max(...future.map((x) => x.high)) >= c[i]!.close + 3 * at;
    tally(stats, "_base", hit);
    for (const f of technicalReversal(c, r, e20, a, i, direction, tf)) tally(stats, f.code, hit);
  }
  return toEvidence(stats);
}

export function reversal(input: GuardInput, trend: Trend): GuardResult["reversal"] {
  const d = sanitize(input.daily);
  const h4 = sanitize(input.h4);
  const closesD = d.map((x) => x.close);
  // Direction: against the background trend; in a range, against the last 5 days.
  let direction: "down" | "up" | null = trend === "up" ? "down" : trend === "down" ? "up" : null;
  if (!direction && closesD.length > 6) {
    const move = last(closesD)! / closesD[closesD.length - 6]! - 1;
    if (Math.abs(move) >= 0.03) direction = move > 0 ? "down" : "up";
  }
  if (!direction) return { score: 0, direction: null, factors: [] };
  const up = direction === "down";
  const raw: Raw[] = [];

  // Technical (daily and 4 h), self-validated below.
  if (d.length > 60) raw.push(...technicalReversal(d, rsi(closesD), ema(closesD, 20), atr(d), d.length - 1, direction, "1d"));
  if (h4.length > 60) {
    const c4 = h4.map((x) => x.close);
    raw.push(...technicalReversal(h4, rsi(c4), ema(c4, 20), atr(h4), h4.length - 1, direction, "4h"));
  }

  // Crowd positioning (crypto derivatives).
  const p = input.positioning;
  if (p?.fundingRate !== null && p?.fundingRate !== undefined) {
    const fr = p.fundingRate;
    const frText = `${(fr * 100).toLocaleString("fr-FR", { maximumFractionDigits: 4 })} % par 8 h`;
    if (up && fr >= GUARD.fundingVeryHot) raw.push({ code: "funding", points: 25, text: `Financement des contrats perpétuels très élevé (${frText}) : les acheteurs à levier sont surchargés, risque de liquidations en cascade.` });
    else if (up && fr >= GUARD.fundingHot) raw.push({ code: "funding", points: 15, text: `Financement élevé (${frText}) : beaucoup d'acheteurs à levier.` });
    else if (!up && fr <= GUARD.fundingVeryCold) raw.push({ code: "funding", points: 25, text: `Financement très négatif (${frText}) : les vendeurs à découvert sont surchargés, risque de rachat brutal (short squeeze).` });
    else if (!up && fr <= GUARD.fundingCold) raw.push({ code: "funding", points: 15, text: `Financement négatif (${frText}) : beaucoup de vendeurs à découvert.` });
  }
  const ls = p?.longShortRatio ?? [];
  if (ls.length >= 24) {
    const rank = percentileRank(ls.slice(0, -1), last(ls)!);
    if (up && rank >= 90) raw.push({ code: "longShort", points: 10, text: `Ratio acheteurs/vendeurs à ${one(last(ls)!)}, parmi les plus hauts du mois : la foule est déjà acheteuse.` });
    if (!up && rank <= 10) raw.push({ code: "longShort", points: 10, text: `Ratio acheteurs/vendeurs à ${one(last(ls)!)}, parmi les plus bas du mois : la foule est déjà vendeuse.` });
  }
  const oi = p?.openInterest ?? [];
  if (oi.length >= 25 && h4.length > 7) {
    const oiChange = last(oi)! / oi[oi.length - 25]! - 1;
    const priceChange = Math.abs(last(h4)!.close / h4[h4.length - 7]!.close - 1);
    if (oiChange >= 0.1 && priceChange < 0.01) raw.push({ code: "openInterest", points: 10, text: `Positions à levier en hausse de ${pct(oiChange * 100)} en 24 h sans que le prix avance : situation fragile.` });
  }

  // Sentiment (contrarian at the extremes).
  const fg = input.sentiment?.fearGreed ?? [];
  if (fg.length) {
    const v = last(fg)!;
    if (up && v >= 80) raw.push({ code: "fearGreed", points: 15, text: `Avidité extrême (Fear & Greed ${v}) : historiquement proche des sommets.` });
    else if (up && v >= 75) raw.push({ code: "fearGreed", points: 8, text: `Forte avidité (Fear & Greed ${v}).` });
    else if (!up && v <= 20) raw.push({ code: "fearGreed", points: 15, text: `Peur extrême (Fear & Greed ${v}) : historiquement proche des creux.` });
    else if (!up && v <= 25) raw.push({ code: "fearGreed", points: 8, text: `Forte peur (Fear & Greed ${v}).` });
  }
  const sb = input.sentiment?.socialBullish, ss = input.sentiment?.socialSample ?? 0;
  if (sb !== null && sb !== undefined && ss >= 20) {
    if (up && sb >= 85) raw.push({ code: "social", points: 10, text: `Réseaux sociaux quasi unanimement optimistes (${Math.round(sb)} % haussiers sur StockTwits).` });
    if (!up && sb <= 30) raw.push({ code: "social", points: 10, text: `Réseaux sociaux très pessimistes (${Math.round(sb)} % haussiers seulement sur StockTwits).` });
  }

  // News tone against the trend (last 24 h).
  const now = input.now ?? Date.now();
  const tone = newsTone((input.news ?? []).filter((n) => n.time >= now - 86_400_000 && n.time <= now));
  if (up && tone.negative >= 2 && tone.negative > tone.positive) raw.push({ code: "newsTone", points: 15, text: `${tone.negative} actualités négatives en 24 h alors que la tendance est haussière.` });
  if (!up && tone.positive >= 2 && tone.positive > tone.negative) raw.push({ code: "newsTone", points: 15, text: `${tone.positive} actualités positives en 24 h alors que la tendance est baissière.` });

  const technical = raw.filter((f) => /(4h|1d)$/.test(f.code));
  const evidence = {
    ...(technical.some((f) => f.code.endsWith("1d")) ? reversalEvidence(d, "1d") : {}),
    ...(technical.some((f) => f.code.endsWith("4h")) ? reversalEvidence(h4, "4h") : {}),
  };
  const factors = finalize(raw, evidence, (c) => /(4h|1d)$/.test(c));
  return { score: Math.min(100, factors.reduce((acc, x) => acc + x.points, 0)), direction, factors };
}

// ---------- Everything together ----------

export function guard(input: GuardInput): GuardResult {
  const reg = regime(input.daily, input.h4);
  const sh = shock(input);
  const rev = reversal(input, reg.trend);
  const notes: string[] = [];
  let scalping: GuardResult["policy"]["scalping"] = "ok", size = 1, stop = 1;
  if (sh.level === "shock") {
    scalping = "pause";
    size = 0;
    stop = 2;
    notes.push("Marché en choc : suspendre les nouvelles positions à court terme, laisser passer la tempête.");
  } else if (sh.level === "agitated") {
    scalping = "reduce";
    size = 0.5;
    stop = 1.5;
    notes.push("Marché agité : diviser la taille des positions par deux et élargir les stops (×1,5) pour ne pas être sorti par le bruit.");
  }
  if (rev.score >= GUARD.reversalHigh && rev.direction) {
    if (scalping === "ok") {
      scalping = "reduce";
      size = 0.5;
      stop = 1;
    }
    notes.push(`Risque de retournement à la ${rev.direction === "down" ? "baisse" : "hausse"} élevé : ne pas ouvrir de position dans le sens de la tendance actuelle, resserrer les stops des positions existantes.`);
  }
  if (!notes.length) notes.push("Conditions normales : pas de signal de choc ni de retournement.");
  if (reg.trend !== "range") notes.push(`Pour le long terme : ${reg.trend === "up" ? "privilégier les achats sur repli" : "privilégier la prudence, les rebonds sont fragiles"} tant que la tendance de fond tient.`);
  return { regime: reg, shock: sh, reversal: rev, policy: { scalping, sizeMultiplier: size, stopMultiplier: stop, notes } };
}
