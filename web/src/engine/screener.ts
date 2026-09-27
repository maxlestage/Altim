/**
 * Selection: which stocks or cryptos to buy, and why, broken down criterion by criterion, for 8 durations
 * (30 min, 1 h, 5 h, 7 days, 14 days, 1 month, 3 months, 6 months).
 *
 * Each asset gets five scores from 0 to 100 (signal, trend, relative strength, buy zone, risk). ONE of them ranks,
 * chosen per market and duration from what was measured on the past (see SPECS and README); the others each have
 * a role (entry price, warning, stop and amount, information). Each selection is replayed on the past, without
 * look-ahead, and the page says plainly when a duration shows no edge once trading costs are paid.
 */
import { analyze, atr, sanitize, sma, type Candle } from "./signal";
import { fibZone, weekly, type ZoneStatus } from "./fibonacci";

export type Horizon = "30m" | "1h" | "5h" | "7d" | "14d" | "1m" | "3m" | "6m";
export const HORIZON_LIST: Horizon[] = ["30m", "1h", "5h", "7d", "14d", "1m", "3m", "6m"];
export const HORIZON_LABEL: Record<Horizon, string> = { "30m": "30 min", "1h": "1 h", "5h": "5 h", "7d": "7 j", "14d": "14 j", "1m": "1 mois", "3m": "3 mois", "6m": "6 mois" };
/** Former names (API compatibility). */
export type ScreenHorizon = Horizon | "short" | "medium" | "long";
export type Criterion = "signal" | "trend" | "momentum" | "zone" | "risk";
export type Market = "stock" | "crypto";
export type CandleInterval = "5m" | "15m" | "30m" | "1d";
/** What ranks: the technical signal, relative strength, its opposite (the biggest recent fall first), low risk. */
export type RankRule = "signal" | "momentum" | "reversal" | "lowRisk";

export const CRITERIA: Record<Criterion, string> = {
  signal: "Signal technique",
  trend: "Tendance de fond",
  momentum: "Force relative",
  zone: "Zone d'achat",
  risk: "Risque",
};

export interface Spec {
  interval: CandleInterval;
  /** Holding period, in candles of `interval`. */
  hold: number;
  /** Replay: a new selection every `step` candles. */
  step: number;
  rank: RankRule;
  /** Relative-strength window (candles) and skipped last candles. */
  momLen: number;
  momSkip: number;
  zone: "medium" | "long";
  /** Stop distance in ATR of `interval`; the target is at twice the risk. */
  stopAtr: number;
  /** Round-trip trading costs (fees and spread), as a fraction. */
  cost: number;
  /** What the measurement found, shown on the page. */
  evidence: string;
}

const MIN: Record<CandleInterval, number> = { "5m": 5, "15m": 15, "30m": 30, "1d": 1440 };
const S = (x: Omit<Spec, "cost" | "zone"> & { zone?: "medium" | "long" }, cost: number): Spec => ({ zone: "medium", ...x, cost });

/**
 * Choices measured in September 2026 (top 10 vs the average of the universe; stocks: 136–149 large US stocks, daily
 * 2021–2026 and 60 days of 5-minute candles; cryptos: 80–108 coins, daily 2024–2026 and 2 weeks of 5-minute candles).
 */
export const SPECS: Record<Market, Record<Horizon, Spec>> = {
  stock: {
    "30m": S({ interval: "5m", hold: 6, step: 24, rank: "reversal", momLen: 6, momSkip: 0, stopAtr: 3, evidence: "À 30 minutes, rien ne fait mieux que la moyenne une fois les frais payés : le meilleur critère (acheter ce qui vient de baisser) rapporte +0,047 % pour 0,05 % de frais." }, 0.0005),
    "1h": S({ interval: "15m", hold: 4, step: 8, rank: "reversal", momLen: 4, momSkip: 0, stopAtr: 3, evidence: "Acheter ce qui a le plus baissé dans l'heure : +0,04 % de mieux que la moyenne, mieux 59 % du temps, à peine au-dessus des frais." }, 0.0005),
    "5h": S({ interval: "30m", hold: 10, step: 10, rank: "momentum", momLen: 10, momSkip: 0, stopAtr: 2.5, evidence: "Acheter ce qui a le plus monté sur les 5 dernières heures de cotation : +0,34 % de mieux que la moyenne, mieux 64 % du temps (60 jours mesurés)." }, 0.0005),
    "7d": S({ interval: "1d", hold: 5, step: 10, rank: "momentum", momLen: 126, momSkip: 21, stopAtr: 1.5, evidence: "Force relative sur 6 mois : +0,6 % de mieux que la moyenne par semaine (2021–2026)." }, 0.0005),
    "14d": S({ interval: "1d", hold: 10, step: 10, rank: "momentum", momLen: 126, momSkip: 21, stopAtr: 1.5, evidence: "Force relative sur 6 mois : +0,6 à 0,8 % de mieux que la moyenne sur 2 semaines (2021–2026)." }, 0.0005),
    "1m": S({ interval: "1d", hold: 21, step: 21, rank: "momentum", momLen: 126, momSkip: 21, stopAtr: 2, evidence: "Force relative sur 6 mois : +2,0 % de mieux que la moyenne par mois, mieux 62 % du temps (2021–2026)." }, 0.0005),
    "3m": S({ interval: "1d", hold: 63, step: 21, rank: "momentum", momLen: 126, momSkip: 21, stopAtr: 2, evidence: "Force relative sur 6 mois : +9,9 % contre +5,3 % sur 3 mois, mieux 69 % du temps (2021–2026)." }, 0.0005),
    "6m": S({ interval: "1d", hold: 126, step: 21, rank: "momentum", momLen: 126, momSkip: 21, zone: "long", stopAtr: 3, evidence: "Force relative sur 6 mois : +20,5 % contre +11,2 % sur 6 mois, mieux 73 % du temps (2021–2026)." }, 0.0005),
  },
  crypto: {
    "30m": S({ interval: "5m", hold: 6, step: 24, rank: "reversal", momLen: 78, momSkip: 0, stopAtr: 3, evidence: "À 30 minutes, rien ne fait mieux que la moyenne une fois les frais payés : le meilleur critère (acheter ce qui a baissé sur 6 h) rapporte +0,05 % pour 0,2 % de frais." }, 0.002),
    "1h": S({ interval: "15m", hold: 4, step: 8, rank: "momentum", momLen: 4, momSkip: 0, stopAtr: 3, evidence: "Acheter ce qui a le plus monté dans l'heure : +0,09 % de mieux que la moyenne, entièrement mangé par les frais (0,2 %)." }, 0.002),
    "5h": S({ interval: "30m", hold: 10, step: 10, rank: "reversal", momLen: 10, momSkip: 0, stopAtr: 2.5, evidence: "Acheter ce qui a le plus baissé sur 5 h (retour à la moyenne) : +0,34 % de mieux que la moyenne, mieux 60 % du temps (2 semaines mesurées : échantillon court)." }, 0.002),
    "7d": S({ interval: "1d", hold: 7, step: 7, rank: "momentum", momLen: 90, momSkip: 0, stopAtr: 1.5, evidence: "Force relative sur 3 mois : +0,75 % de mieux que la moyenne par semaine, positive dans les deux moitiés de 2024–2026." }, 0.002),
    "14d": S({ interval: "1d", hold: 14, step: 14, rank: "momentum", momLen: 90, momSkip: 0, stopAtr: 1.5, evidence: "Force relative sur 3 mois : +1,1 % de mieux que la moyenne sur 2 semaines (2024–2026)." }, 0.002),
    "1m": S({ interval: "1d", hold: 30, step: 15, rank: "signal", momLen: 126, momSkip: 21, stopAtr: 2, evidence: "Signal technique d'Altim : +2,3 % contre −1,9 % par mois, mieux 60 % du temps (2024–2026)." }, 0.002),
    "3m": S({ interval: "1d", hold: 90, step: 15, rank: "signal", momLen: 126, momSkip: 21, stopAtr: 2, evidence: "Signal technique d'Altim : +2,0 % contre −6,4 % sur 3 mois, mieux 66 % du temps (2024–2026)." }, 0.002),
    "6m": S({ interval: "1d", hold: 180, step: 15, rank: "lowRisk", momLen: 126, momSkip: 21, zone: "long", stopAtr: 3, evidence: "Les cryptos les plus calmes (les grandes) : +9,4 % contre −12,8 % sur 6 mois, mieux 94 % du temps (périodes qui se chevauchent : à prendre avec prudence)." }, 0.002),
  },
};

/** Former horizons → new ones. */
export function toHorizon(h: string, market: Market): Horizon | null {
  if ((HORIZON_LIST as string[]).includes(h)) return h as Horizon;
  if (h === "short") return "14d";
  if (h === "medium") return market === "crypto" ? "1m" : "3m";
  if (h === "long") return market === "crypto" ? "3m" : "6m";
  return null;
}

/** Criterion shown as the one that ranks, and the role of each criterion. */
export function roles(spec: Spec, market: Market): Record<Criterion, string> {
  const what = market === "crypto" ? "les cryptos" : "les actions";
  const r: Record<Criterion, string> = { signal: "information", trend: "alerte si baissière", momentum: "information", zone: "fixe le prix d'entrée", risk: "règle le stop et le montant" };
  if (spec.rank === "signal") r.signal = `classe ${what}`;
  if (spec.rank === "momentum") r.momentum = `classe ${what}`;
  if (spec.rank === "reversal") r.momentum = `classe à l'envers : les plus en baisse d'abord`;
  if (spec.rank === "lowRisk") r.risk = `classe ${what} (les plus calmes d'abord)`;
  return r;
}
export const RANKED_CRITERION: Record<RankRule, Criterion> = { signal: "signal", momentum: "momentum", reversal: "momentum", lowRisk: "risk" };

/** At most this many stocks of the same sector in a selection. */
export const SECTOR_CAP = 3;

/**
 * A coin that barely moves follows a currency or another asset (stablecoin missing from the lists, pegged token):
 * volatility brought to a daily scale under 0.5 %.
 */
export function isPegged(atrPct: number | null, interval: CandleInterval): boolean {
  if (atrPct == null) return false;
  return atrPct * Math.sqrt(1440 / MIN[interval]) < 0.5;
}

/** Duration in words of `n` candles of an interval ("30 min", "5 h", "6 mois"…). */
export function span(n: number, interval: CandleInterval, market: Market): string {
  if (interval !== "1d") {
    const m = n * MIN[interval];
    return m < 60 ? `${m} min` : `${Math.round((m / 60) * 10) / 10} h`.replace(".", ",");
  }
  const perMonth = market === "stock" ? 21 : 30;
  if (n < perMonth) return `${n} ${market === "stock" ? "séances" : "jours"}`;
  const months = Math.round(n / perMonth);
  return `${months} mois`;
}

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

/** Factors of one asset known at the close of candle `i` (only candles 0…i are read). */
export function factorsAt(all: Candle[], i: number, spec: Spec): RawFactors | null {
  if (i < 210) return null;
  const c = all.slice(Math.max(0, i - 399), i + 1);
  const closes = c.map((x) => x.close);
  const last = c[c.length - 1]!;
  const s = analyze(c);
  const signal = s ? clamp(50 + s.score / 2) : 50;
  const s200 = sma(closes, 200), s50 = sma(closes, 50);
  const a = s200[s200.length - 1], b = s50[s50.length - 1], a20 = s200[s200.length - 21];
  const trend = a == null || b == null ? 50 : (last.close > a ? 40 : 0) + (b > a ? 30 : 0) + (a20 != null && a > a20 ? 30 : 0);
  const end = c.length - 1 - spec.momSkip, start = end - spec.momLen;
  const momentum = start >= 0 ? (c[end]!.close / c[start]!.close - 1) * 100 : null;
  const at = atr(c);
  const atrV = at[at.length - 1];
  const atrPct = atrV != null ? (atrV / last.close) * 100 : null;
  let peak = 0;
  for (const x of c.slice(-252)) peak = Math.max(peak, x.high);
  const drawdown = peak > 0 ? (1 - last.close / peak) * 100 : null;
  const z = spec.zone === "long" ? fibZone(weekly(c), "long", last.close, false) : fibZone(c, "medium", last.close, false);
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

/** Scores of a whole universe at the same date: momentum and risk are ranked against each other; `total` is the
 * ranking score of the spec's rule. */
export function scoreUniverse(list: (RawFactors | null)[], spec: Spec): (Scored | null)[] {
  const mom = ranks(list.map((f) => f?.momentum ?? null), true);
  const vol = ranks(list.map((f) => f?.volatility ?? null), false);
  const dd = ranks(list.map((f) => f?.drawdown ?? null), false);
  return list.map((f, i) => {
    if (!f) return null;
    if ((spec.rank === "momentum" || spec.rank === "reversal") && mom[i] == null) return null;
    if (spec.rank === "lowRisk" && vol[i] == null) return null;
    const risk = vol[i] != null && dd[i] != null ? (vol[i]! + dd[i]!) / 2 : 50;
    const scores = { signal: f.signal, trend: f.trend, momentum: mom[i] ?? 50, zone: f.zone, risk };
    const total = spec.rank === "signal" ? f.signal : spec.rank === "momentum" ? mom[i]! : spec.rank === "reversal" ? 100 - mom[i]! : vol[i]!;
    return { scores, total };
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
export function explain(f: RawFactors, s: Scored, spec: Spec, market: Market): Record<Criterion, string> {
  const pct = (v: number) => `${v >= 0 ? "+" : "−"}${Math.abs(v).toLocaleString("fr-FR", { maximumFractionDigits: 2 })} %`;
  const period = `${span(spec.momLen, spec.interval, market)}${spec.momSkip ? ` (hors ${span(spec.momSkip, spec.interval, market)})` : ""}`;
  const per = spec.interval === "1d" ? "par jour" : `par bougie de ${MIN[spec.interval]} min`;
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
    momentum: f.momentum == null ? "Historique trop court."
      : spec.rank === "reversal" ? `${pct(f.momentum)} sur ${period} : a plus baissé que ${Math.round(100 - s.scores.momentum)} % des autres (rebond attendu).`
      : `${pct(f.momentum)} sur ${period}, mieux que ${Math.round(s.scores.momentum)} % des autres.`,
    zone: zoneText[f.zoneStatus],
    risk: `Volatilité ${f.atrPct != null ? `${f.atrPct.toLocaleString("fr-FR", { maximumFractionDigits: 2 })} % ${per}` : "inconnue"}${f.drawdown != null ? `, ${Math.round(f.drawdown)} % sous son plus haut récent` : ""} : plus calme que ${Math.round(s.scores.risk)} % des autres.`,
  };
}

export interface Validation {
  horizon: Horizon;
  periods: number;
  /** Average forward return of the top N, of the whole universe, and share of periods where the top N did better. */
  top: number;
  universe: number;
  beatRate: number;
  topN: number;
  hold: number;
  /** Return of the reference (Bitcoin for cryptos) over the same periods. */
  benchmark?: number | null;
  from: number | null;
  to: number | null;
  /** Round-trip costs (%). */
  cost: number;
  /**
   * clear: ahead of the average by more than the costs, more than 55 % of the time · weak: ahead on average but about
   * one time in two (a few big winners) · none: not ahead once the costs are paid.
   */
  edge: "clear" | "weak" | "none";
  noEdge: boolean;
}

/**
 * Replays the selection on the past: every `every` sessions, score the universe with the data known that day,
 * take the top N, and compare their return over the next `HOLD[h]` sessions with the universe average.
 * Series must be aligned on the same dates (see alignSeries).
 */
export function validate(series: Candle[][], spec: Spec, h: Horizon, topN = 10, sectors?: string[], benchmark?: number): Validation | null {
  const n = Math.min(...series.map((s) => s.length));
  const { hold, step: every } = spec;
  const tops: number[] = [], alls: number[] = [], bench: number[] = [];
  let beat = 0, from: number | null = null, to: number | null = null;
  for (let i = 280; i + hold < n; i += every) {
    const f = series.map((s) => factorsAt(s, s.length - n + i, spec));
    const sc = scoreUniverse(f, spec);
    const fwd = series.map((s) => {
      const j = s.length - n + i;
      return s[j + hold]!.close / s[j]!.close - 1;
    });
    const idx = pick(sc.map((x, k) => ({ k, t: x?.total ?? null })), (x) => x.t, (x) => sectors?.[x.k], topN).map((x) => x.k);
    if (idx.length < topN) continue;
      const topR = idx.reduce((a, k) => a + fwd[k]!, 0) / idx.length;
    const all = fwd.reduce((a, b) => a + b, 0) / fwd.length;
    tops.push(topR);
    alls.push(all);
    if (benchmark != null) bench.push(fwd[benchmark]!);
    if (topR > all) beat++;
    from ??= series[0]![series[0]!.length - n + i]!.time;
    to = series[0]![series[0]!.length - n + i]!.time;
  }
  if (!tops.length) return null;
  const avg = (v: number[]) => (v.reduce((a, b) => a + b, 0) / v.length) * 100;
  const top = avg(tops), universe = avg(alls), beatRate = (beat / tops.length) * 100, cost = spec.cost * 100;
  const edge = top - universe <= cost || beatRate < 45 ? "none" : beatRate >= 55 ? "clear" : "weak";
  return { horizon: h, periods: tops.length, top, universe, beatRate, topN, hold, from, to, benchmark: bench.length ? avg(bench) : null, cost, edge, noEdge: edge !== "clear" };
}

/** Keeps the dates (daily) or the exact times (intraday) present in every series. */
export function alignSeries(series: Candle[][], intraday = false): Candle[][] {
  const clean = series.map((s) => sanitize(s));
  const day = (t: number) => (intraday ? String(t) : new Date(t).toISOString().slice(0, 10));
  const common = clean.reduce<Set<string> | null>((acc, s) => {
    const d = new Set(s.map((c) => day(c.time)));
    return acc ? new Set([...acc].filter((x) => d.has(x))) : d;
  }, null) ?? new Set();
  return clean.map((s) => s.filter((c) => common.has(day(c.time))));
}
