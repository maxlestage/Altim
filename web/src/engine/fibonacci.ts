/**
 * Buy zones by horizon, with Fibonacci retracements.
 *
 * Traders buy the pull-back of an upward move: after a rise from a low L to a high H, the price often comes back
 * to 38.2 %–65 % of the move before resuming (the "golden pocket" is 61.8 %–65 %). The move depends on the horizon:
 * - short term: 4 h candles, move of the last ~2 weeks (a few days to 2 weeks of holding);
 * - medium term: daily candles, move of the last ~4 months (a few weeks to a few months);
 * - long term: weekly candles, move of the last ~2 years (months to years).
 *
 * Fibonacci is a convention, not a law: each zone is checked on the asset's own history (how often a first entry
 * into the zone went back to the high before breaking the low, compared with random entries with the same
 * distances). No look-ahead: at each past candle, the move is recomputed with the data known at that time.
 */
import { moneyFmt } from "../money";
import { atr, sanitize, type Candle } from "./signal";
import type { Evidence } from "./guard";

export type Horizon = "short" | "medium" | "long";
export type ZoneStatus = "above" | "inZone" | "golden" | "deep" | "broken" | "downtrend" | "none";

export const RATIOS = [0.236, 0.382, 0.5, 0.618, 0.65, 0.786] as const;
export const EXTENSIONS = [1.272, 1.618] as const;

export const HORIZONS: Record<Horizon, { label: string; unit: string; holding: string; window: number; outcome: number; minAtr: number }> = {
  short: { label: "Court terme", unit: "4 h", holding: "quelques jours à 2 semaines", window: 90, outcome: 30, minAtr: 4 },
  medium: { label: "Moyen terme", unit: "1 j", holding: "quelques semaines à quelques mois", window: 120, outcome: 40, minAtr: 4 },
  long: { label: "Long terme", unit: "1 sem.", holding: "plusieurs mois à plusieurs années", window: 104, outcome: 26, minAtr: 3 },
};

export interface Swing { trend: "up" | "down"; low: number; high: number; lowIndex: number; highIndex: number; lowTime: number; highTime: number }

export interface FibZone {
  horizon: Horizon;
  label: string;
  unit: string;
  holding: string;
  status: ZoneStatus;
  swing: Swing | null;
  /** Retracement levels of the up move (price for each ratio). */
  levels: { ratio: number; price: number }[];
  /** Buy zone 38.2 %–65 % (from = lower bound), golden pocket 61.8 %–65 %. */
  zone: { from: number; to: number } | null;
  golden: { from: number; to: number } | null;
  /** Below this price (the low of the move), the zone is invalidated. */
  invalidation: number | null;
  /** Previous high, then extensions 127.2 % and 161.8 % of the move. */
  targets: number[];
  /** % to go down to reach the top of the zone (status "above"). */
  distance: number | null;
  /** How the zones behaved on this asset's history. */
  evidence: Evidence | null;
  text: string;
}

/** Weekly candles from daily ones (weeks starting Monday, UTC). The last week may be in progress. */
export function weekly(daily: Candle[]): Candle[] {
  const out: Candle[] = [];
  const MONDAY = 4 * 86_400_000; // 1970-01-05 was a Monday: epoch + 4 days
  let key = NaN;
  for (const c of sanitize(daily)) {
    const k = Math.floor((c.time - MONDAY) / (7 * 86_400_000));
    const last = out[out.length - 1];
    if (k !== key || !last) {
      out.push({ ...c });
      key = k;
    } else {
      last.high = Math.max(last.high, c.high);
      last.low = Math.min(last.low, c.low);
      last.close = c.close;
      last.volume += c.volume;
    }
  }
  return out;
}

/**
 * Last significant move known at candle `until`, within `window` candles:
 * - lowest low before the highest high → up move (buy zones in its pull-back);
 * - highest high before the lowest low → down move, unless the price has since rebounded by at least 38.2 % of the
 *   fall: the rebound from the low is then the current up move.
 * A move smaller than `minAtr` ATR is ignored.
 */
export function swingAt(c: Candle[], until: number, window: number, minAtr: number, atrSeries = atr(c)): Swing | null {
  const start = Math.max(0, until - window + 1);
  if (until - start < 20) return null;
  const a = atrSeries[until];
  if (a == null) return null;
  const argmax = (from: number) => { let k = from; for (let i = from; i <= until; i++) if (c[i]!.high >= c[k]!.high) k = i; return k; };
  let lo = start;
  for (let i = start; i <= until; i++) if (c[i]!.low <= c[lo]!.low) lo = i;
  const hi = argmax(start);
  const make = (l: number, h: number, trend: "up" | "down"): Swing =>
    ({ trend, low: c[l]!.low, high: c[h]!.high, lowIndex: l, highIndex: h, lowTime: c[l]!.time, highTime: c[h]!.time });
  if (lo < hi) return c[hi]!.high - c[lo]!.low >= minAtr * a ? make(lo, hi, "up") : null;
  if (lo === until) return c[hi]!.high - c[lo]!.low >= minAtr * a ? make(lo, hi, "down") : null;
  // Down move, then possibly a rebound from the low.
  const fall = c[hi]!.high - c[lo]!.low;
  const hi2 = argmax(lo + 1);
  const rebound = c[hi2]!.high - c[lo]!.low;
  if (rebound >= minAtr * a && rebound >= 0.382 * fall) return make(lo, hi2, "up");
  return fall >= minAtr * a ? make(lo, hi, "down") : null;
}

export const level = (s: Swing, r: number) => s.high - r * (s.high - s.low);

/** Outcome of an entry: +1 if the price reaches `up` before `down` within `bars`, 0 otherwise. */
function outcome(c: Candle[], i: number, up: number, down: number, bars: number): number {
  for (let j = i + 1; j <= Math.min(c.length - 1, i + bars); j++) {
    if (c[j]!.low <= down) return 0; // pessimistic when both are touched in the same candle
    if (c[j]!.high >= up) return 1;
  }
  return 0;
}

/**
 * History of the zones on this asset: every first entry into the 38.2–65 % zone of an up move (known at that time),
 * success = back to the high before breaking the low. Base = same distances from every candle of the history.
 */
export function zoneEvidence(c: Candle[], h: Horizon): Evidence | null {
  const { window, outcome: bars, minAtr } = HORIZONS[h];
  const a = atr(c);
  const events: { i: number; up: number; down: number }[] = [];
  const seen = new Set<number>();
  for (let i = 30; i < c.length - bars; i++) {
    const s = swingAt(c, i, window, minAtr, a);
    if (!s || s.trend !== "up" || seen.has(s.highIndex) || s.highIndex === i) continue;
    const top = level(s, 0.382), bottom = level(s, 0.65);
    const p = c[i]!.close;
    if (p <= top && p >= bottom && c[i - 1]!.close > top) {
      seen.add(s.highIndex);
      events.push({ i, up: s.high / p, down: s.low / p });
    }
  }
  if (!events.length) return null;
  const wins = events.reduce((acc, e) => acc + outcome(c, e.i, c[e.i]!.close * e.up, c[e.i]!.close * e.down, bars), 0);
  let baseWins = 0, baseN = 0;
  for (const e of events) {
    for (let j = 30; j < c.length - bars; j++) {
      baseWins += outcome(c, j, c[j]!.close * e.up, c[j]!.close * e.down, bars);
      baseN++;
    }
  }
  const rate = (wins / events.length) * 100;
  const base = baseN ? (baseWins / baseN) * 100 : 0;
  return { samples: events.length, rate, base, lift: base > 0 ? rate / base : rate > 0 ? 2 : 1 };
}

const px = (v: number) =>
  moneyFmt(v, (x) => x.toLocaleString("fr-FR", x >= 1 ? { minimumFractionDigits: 2, maximumFractionDigits: 2 } : { maximumSignificantDigits: 4 }), " ");
const pct = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} %`;

/** Position of a price against the zones of an up move: status, distance to the zone, explanation. */
export function zoneState(s: Swing, p: number): { status: ZoneStatus; distance: number | null; text: string } {
  const top = level(s, 0.382), goldenTop = level(s, 0.618), bottom = level(s, 0.65);
  const status: ZoneStatus = p > top ? "above" : p >= goldenTop ? "inZone" : p >= bottom ? "golden" : p >= s.low ? "deep" : "broken";
  const distance = status === "above" ? (1 - top / p) * 100 : null;
  const text = {
    above: `Prix au-dessus de la zone : attendre un repli vers ${px(top)} – ${px(bottom)} (−${pct(distance ?? 0)} pour l'atteindre).`,
    inZone: `Prix dans la zone d'achat (38,2 % – 61,8 % du mouvement de ${px(s.low)} à ${px(s.high)}).`,
    golden: `Prix dans la « zone d'or » (61,8 % – 65 %), le repli le plus surveillé par les traders.`,
    deep: `Repli profond (au-delà de 65 %) : dernier soutien avant ${px(s.low)} ; un passage sous ce plus bas invalide la zone.`,
    broken: `Le plus bas du mouvement (${px(s.low)}) est cassé : zone invalidée, attendre un nouveau point bas.`,
    downtrend: "",
    none: "",
  }[status];
  return { status, distance, text };
}

/** Buy zone of one horizon, from the candles of that horizon (4 h, daily or weekly). */
export function fibZone(raw: Candle[], h: Horizon, price?: number | null, withEvidence = true): FibZone {
  const c = sanitize(raw);
  const cfg = HORIZONS[h];
  const base = { horizon: h, label: cfg.label, unit: cfg.unit, holding: cfg.holding };
  const empty = { swing: null, levels: [], zone: null, golden: null, invalidation: null, targets: [], distance: null, evidence: null };
  const s = c.length >= 30 ? swingAt(c, c.length - 1, cfg.window, cfg.minAtr) : null;
  if (!s) return { ...base, ...empty, status: "none", text: c.length < 30 ? "Historique insuffisant pour cet horizon." : "Pas de mouvement assez net pour tracer des niveaux." };
  const p = price ?? c[c.length - 1]!.close;
  if (s.trend === "down") {
    return {
      ...base, ...empty, swing: s, status: "downtrend",
      levels: RATIOS.map((r) => ({ ratio: r, price: s.low + r * (s.high - s.low) })),
      text: `Mouvement baissier en cours (de ${px(s.high)} à ${px(s.low)}) : pas de zone d'achat en repli. Les niveaux de Fibonacci au-dessus du prix sont des résistances ; attendre qu'un nouveau creux se forme et que le prix reparte.`,
    };
  }
  const levels = RATIOS.map((r) => ({ ratio: r, price: level(s, r) }));
  const zone = { from: level(s, 0.65), to: level(s, 0.382) };
  const golden = { from: level(s, 0.65), to: level(s, 0.618) };
  const targets = [s.high, ...EXTENSIONS.map((e) => s.low + e * (s.high - s.low))];
  const evidence = withEvidence ? zoneEvidence(c, h) : null;
  const { status, distance, text } = zoneState(s, p);
  return { ...base, status, swing: s, levels, zone, golden, invalidation: s.low, targets, distance, evidence, text };
}

/** The three horizons: 4 h candles, daily candles, and a long daily history (≈ 3 years) turned into weeks. */
export function fibZones(h4: Candle[], daily: Candle[], longDaily: Candle[], price?: number | null, withEvidence = true): FibZone[] {
  const w = weekly(longDaily.length > daily.length ? longDaily : daily);
  return [fibZone(h4, "short", price, withEvidence), fibZone(daily, "medium", price, withEvidence), fibZone(w, "long", price, withEvidence)];
}
