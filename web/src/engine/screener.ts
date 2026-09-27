/**
 * Stock selection: which stocks to buy, and why, broken down criterion by criterion.
 *
 * Each stock of the universe (the largest US companies) gets five scores from 0 to 100, each with its own role:
 * - momentum : 6-month performance, last month excluded, ranked against the others. It is the ONLY criterion that
 *              ranks: measured on 136 large US stocks (2021–2026), the 10 best did better than the average over
 *              10, 63 and 126 sessions, whereas the others did not (see README);
 * - zone     : Fibonacci buy zone → the entry price (now, or a limit order at the top of the zone);
 * - trend    : background trend (price vs 200-day average, 50 vs 200, slope of the 200) → warning if bearish;
 * - signal   : Altim's technical signal on daily candles → shown, with its track record on the stock;
 * - risk     : volatility and drawdown against the others → width of the stop and size of the position.
 * The selection is replayed on the past (no look-ahead, same sector cap) to show what it would have done.
 */
import { analyze, atr, sanitize, sma, type Candle } from "./signal";
import { fibZone, weekly, type ZoneStatus } from "./fibonacci";

export type ScreenHorizon = "short" | "medium" | "long";
export type Criterion = "signal" | "trend" | "momentum" | "zone" | "risk";

export const CRITERIA: Record<Criterion, string> = {
  signal: "Signal technique",
  trend: "Tendance de fond",
  momentum: "Force relative",
  zone: "Zone d'achat",
  risk: "Risque",
};

/** What each criterion is used for (shown next to its score). */
export const ROLE: Record<Criterion, string> = {
  momentum: "classe les actions",
  zone: "fixe le prix d'entrée",
  trend: "alerte si baissière",
  signal: "information",
  risk: "règle le stop et le montant",
};

/** 6-month momentum, last month excluded (the most regular variant measured, see README). */
const MOMENTUM = { length: 126, skip: 21 };
/** Stop distance in daily ATR, per horizon; the target is at twice the risk. */
export const STOP_ATR: Record<ScreenHorizon, number> = { short: 1.5, medium: 2, long: 3 };
/** At most this many stocks of the same sector in the selection. */
export const SECTOR_CAP = 3;

/** Forward window used to judge a selection on the past (sessions). */
export const HOLD: Record<ScreenHorizon, number> = { short: 10, medium: 63, long: 126 };

export interface RawFactors {
  signal: number;
  trend: number;
  zone: number;
  zoneStatus: ZoneStatus;
  zoneDistance: number | null;
  /** Raw values, turned into ranks across the universe. */
  momentum: number | null;
  volatility: number | null;
  drawdown: number | null;
  price: number;
  action: string;
  signalScore: number;
  confidence: number;
  atrPct: number | null;
}

const clamp = (v: number) => Math.max(0, Math.min(100, v));

/** Factors of one stock known at the close of candle `i` (only candles 0…i are read). */
export function factorsAt(all: Candle[], i: number, h: ScreenHorizon): RawFactors | null {
  if (i < 210) return null;
  const c = all.slice(Math.max(0, i - 399), i + 1);
  const closes = c.map((x) => x.close);
  const last = c[c.length - 1]!;
  const s = analyze(c);
  const signal = s ? clamp(50 + s.score / 2) : 50;
  const s200 = sma(closes, 200), s50 = sma(closes, 50);
  const a = s200[s200.length - 1], b = s50[s50.length - 1], a20 = s200[s200.length - 21];
  const trend = a == null || b == null ? 50 : (last.close > a ? 40 : 0) + (b > a ? 30 : 0) + (a20 != null && a > a20 ? 30 : 0);
  const m = MOMENTUM;
  const end = c.length - 1 - m.skip, start = end - m.length;
  const momentum = start >= 0 ? (c[end]!.close / c[start]!.close - 1) * 100 : null;
  const at = atr(c);
  const atrV = at[at.length - 1];
  const atrPct = atrV != null ? (atrV / last.close) * 100 : null;
  let peak = 0;
  for (const x of c.slice(-252)) peak = Math.max(peak, x.high);
  const drawdown = peak > 0 ? (1 - last.close / peak) * 100 : null;
  const z = h === "long" ? fibZone(weekly(c), "long", last.close, false) : fibZone(c, "medium", last.close, false);
  const zone = {
    inZone: 100, golden: 100, deep: 60, none: 40, broken: 0, downtrend: 10,
    above: clamp(100 - (z.distance ?? 0) * 6),
  }[z.status];
  return {
    signal, trend, zone, zoneStatus: z.status, zoneDistance: z.distance, momentum, volatility: atrPct, drawdown,
    price: last.close, action: s?.action ?? "hold", signalScore: s?.score ?? 0, confidence: s?.confidence ?? 0, atrPct,
  };
}

/** Percentile rank (0–100) of each value among the others; null stays null. */
export function ranks(values: (number | null)[], higherIsBetter: boolean): (number | null)[] {
  const known = values.filter((v): v is number => v != null && Number.isFinite(v)).sort((x, y) => x - y);
  if (known.length < 2) return values.map((v) => (v == null ? null : 50));
  return values.map((v) => {
    if (v == null || !Number.isFinite(v)) return null;
    let below = 0, equal = 0;
    for (const k of known) {
      if (k < v) below++;
      else if (k === v) equal++;
    }
    const p = ((below + (equal - 1) / 2) / (known.length - 1)) * 100;
    return higherIsBetter ? p : 100 - p;
  });
}

export interface Scored {
  scores: Record<Criterion, number>;
  total: number;
}

/** Scores of a whole universe at the same date: momentum and risk are ranked against each other; the ranking
 * score (`total`) is the momentum rank. */
export function scoreUniverse(list: (RawFactors | null)[], _h: ScreenHorizon): (Scored | null)[] {
  const mom = ranks(list.map((f) => f?.momentum ?? null), true);
  const vol = ranks(list.map((f) => f?.volatility ?? null), false);
  const dd = ranks(list.map((f) => f?.drawdown ?? null), false);
  return list.map((f, i) => {
    if (!f || mom[i] == null) return null;
    const risk = vol[i] != null && dd[i] != null ? (vol[i]! + dd[i]!) / 2 : 50;
    const scores = { signal: f.signal, trend: f.trend, momentum: mom[i]!, zone: f.zone, risk };
    return { scores, total: mom[i]! };
  });
}

/** Best `topN` by ranking score, at most SECTOR_CAP per sector (sectors optional). */
export function pick<T>(items: T[], score: (t: T) => number | null, sector: (t: T) => string | undefined, topN: number): T[] {
  const sorted = items.map((t) => ({ t, v: score(t) })).filter((x) => x.v != null).sort((a, b) => b.v! - a.v!);
  const per = new Map<string, number>();
  const out: T[] = [];
  for (const { t } of sorted) {
    const sec = sector(t);
    if (sec) {
      if ((per.get(sec) ?? 0) >= SECTOR_CAP) continue;
      per.set(sec, (per.get(sec) ?? 0) + 1);
    }
    out.push(t);
    if (out.length >= topN) break;
  }
  return out;
}

/** Plain-language explanation of each criterion's score. */
export function explain(f: RawFactors, s: Scored, h: ScreenHorizon): Record<Criterion, string> {
  const pct = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;
  const period = "6 mois (hors dernier mois)";
  const zoneText: Record<ZoneStatus, string> = {
    inZone: "Dans la zone d'achat Fibonacci.",
    golden: "Dans la « zone d'or » (61,8–65 %).",
    deep: "Repli profond, proche du dernier soutien.",
    above: `Au-dessus de la zone : repli de ${pct(-(f.zoneDistance ?? 0))} pour l'atteindre.`,
    broken: "Plus bas cassé : zone invalidée.",
    downtrend: "Mouvement baissier : pas de zone d'achat.",
    none: "Pas de mouvement net pour tracer une zone.",
  };
  return {
    signal: f.action === "strongBuy" ? `Achat fort (score ${Math.round(f.signalScore)}, confiance ${Math.round(f.confidence)} %).`
      : f.action === "buy" ? `Achat (score ${Math.round(f.signalScore)}, confiance ${Math.round(f.confidence)} %).`
      : f.action === "hold" ? `Neutre (score ${Math.round(f.signalScore)}).` : `Vente (score ${Math.round(f.signalScore)}).`,
    trend: s.scores.trend >= 100 ? "Haussière : au-dessus de la moyenne 200 jours, qui monte."
      : s.scores.trend >= 70 ? "Plutôt haussière." : s.scores.trend >= 40 ? "Mitigée." : "Baissière : sous la moyenne 200 jours.",
    momentum: f.momentum == null ? "Historique trop court." : `${pct(f.momentum)} sur ${period}, mieux que ${Math.round(s.scores.momentum)} % des autres.`,
    zone: zoneText[f.zoneStatus],
    risk: `Volatilité ${f.atrPct != null ? `${f.atrPct.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} % par jour` : "inconnue"}, ${f.drawdown != null ? `${Math.round(f.drawdown)} % sous son plus haut sur 1 an` : ""} : plus calme que ${Math.round(s.scores.risk)} % des autres.`,
  };
}

export interface Validation {
  horizon: ScreenHorizon;
  periods: number;
  /** Average forward return of the top N, of the whole universe, and share of periods where the top N did better. */
  top: number;
  universe: number;
  beatRate: number;
  topN: number;
  hold: number;
  from: number | null;
  to: number | null;
}

/**
 * Replays the selection on the past: every `every` sessions, score the universe with the data known that day,
 * take the top N, and compare their return over the next `HOLD[h]` sessions with the universe average.
 * Series must be aligned on the same dates (see alignSeries).
 */
export function validate(series: Candle[][], h: ScreenHorizon, topN = 10, every = 21, sectors?: string[]): Validation | null {
  const n = Math.min(...series.map((s) => s.length));
  const hold = HOLD[h];
  const tops: number[] = [], alls: number[] = [];
  let beat = 0, from: number | null = null, to: number | null = null;
  for (let i = 280; i + hold < n; i += every) {
    const f = series.map((s) => factorsAt(s, s.length - n + i, h));
    const sc = scoreUniverse(f, h);
    const fwd = series.map((s) => {
      const j = s.length - n + i;
      return s[j + hold]!.close / s[j]!.close - 1;
    });
    const idx = pick(sc.map((x, k) => ({ k, t: x?.total ?? null })), (x) => x.t, (x) => sectors?.[x.k], topN).map((x) => x.k);
    if (idx.length < topN) continue;
    const top = idx.reduce((a, k) => a + fwd[k]!, 0) / idx.length;
    const all = fwd.reduce((a, b) => a + b, 0) / fwd.length;
    tops.push(top);
    alls.push(all);
    if (top > all) beat++;
    from ??= series[0]![series[0]!.length - n + i]!.time;
    to = series[0]![series[0]!.length - n + i]!.time;
  }
  if (!tops.length) return null;
  const avg = (v: number[]) => (v.reduce((a, b) => a + b, 0) / v.length) * 100;
  return { horizon: h, periods: tops.length, top: avg(tops), universe: avg(alls), beatRate: (beat / tops.length) * 100, topN, hold, from, to };
}

/** Keeps the dates present in every series (same sessions for everyone). */
export function alignSeries(series: Candle[][]): Candle[][] {
  const clean = series.map((s) => sanitize(s));
  const day = (t: number) => new Date(t).toISOString().slice(0, 10);
  const common = clean.reduce<Set<string> | null>((acc, s) => {
    const d = new Set(s.map((c) => day(c.time)));
    return acc ? new Set([...acc].filter((x) => d.has(x))) : d;
  }, null) ?? new Set();
  return clean.map((s) => s.filter((c) => common.has(day(c.time))));
}
