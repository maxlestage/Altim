/**
 * Configuration changes of the watched assets ("🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT"):
 * the last decision seen per asset (verdict, level, unmet setup steps, buy conditions) is kept in this browser and
 * compared with each new one (Radar refresh, asset page). The server is stateless: the diff is done here.
 * Each snapshot also keeps the decision's measurements (family scores, composite score, nearest support and
 * resistance, relative volume, RSI, news tone) so that a change of verdict or rating is explained ("Momentum
 * −18 pts", "Volume en baisse (1,4× → 0,7× la moyenne)", …): `explainChange`.
 * Pure functions + a small localStorage store (versioned, validated on read; version 1 entries are migrated),
 * tested with `bun test`.
 */
import { useSyncExternalStore } from "react";
import type { Kind } from "../engine/reliability";
import { num, signedScore, usd as usdFr, type Decision, type Level, type Rating, type Verdict } from "./decision";

const KEY = "altim.configChanges.v1";
/** Transitions kept (newest first). */
export const MAX_TRANSITIONS = 50;

const VERDICTS: Verdict[] = ["buy", "buyZone", "wait", "noPosition", "trim", "sell"];
const RATINGS: Rating[] = ["strongBuy", "buy", "hold", "reduce", "sell", "strongSell"];
/** Version of the stored state (1: without measurements nor explanations, migrated on read). */
export const STATE_VERSION = 2;

/** Measurements of a decision kept to explain a later change (the server's `snapshot`, else read from the decision). */
export interface SignalMetrics {
  price: number | null;
  composite: number | null;
  families: { key: string; label: string; score: number | null }[];
  /** Last daily volume ÷ 20-day average. */
  relVolume: number | null;
  rsi: number | null;
  support: { price: number; touches: number } | null;
  resistance: { price: number; touches: number } | null;
  newsScore: number | null;
  topNews: { title: string; tone: string } | null;
}
const LEVELS: Level[] = ["strong", "moderate", "waiting", "highRisk", "exit"];

export interface ConfigSnapshot {
  verdict: Verdict; level: Level; label: string; levelLabel: string;
  /** Setup steps not met yet ("Cassure de la résistance : pas encore"). */
  missing: string[];
  /** Conditions that would change the decision (the decision's `toBuy`). */
  triggers: string[];
  at: number;
  /** Added in version 2 (absent from migrated entries). */
  rating?: Rating; ratingLabel?: string; metrics?: SignalMetrics;
}

export interface ConfigTransition {
  symbol: string; kind: Kind; name: string;
  /** Personal mode (with the user's average cost and weights) or market data only: compared separately. */
  personal: boolean;
  at: number;
  /** When the previous configuration was seen. */
  since: number;
  from: Pick<ConfigSnapshot, "verdict" | "level" | "label" | "levelLabel" | "rating" | "ratingLabel">;
  to: Pick<ConfigSnapshot, "verdict" | "level" | "label" | "levelLabel" | "rating" | "ratingLabel">;
  missing: string[];
  triggers: string[];
  /** What changed in the measurements ("Momentum −18 pts (+40 → +22)"); empty when unknown (older entries). */
  changes: string[];
}

export interface ConfigState { version: typeof STATE_VERSION; last: Record<string, ConfigSnapshot>; transitions: ConfigTransition[] }

const empty = (): ConfigState => ({ version: STATE_VERSION, last: {}, transitions: [] });
const stepState: Record<string, string> = { no: "pas encore", unknown: "non vérifiable" };
const usd = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: v >= 1 ? 2 : 6 })} $`;

const fin = (v: unknown): number | null => (typeof v === "number" && Number.isFinite(v) ? v : null);

/** The server's compact snapshot when given, else the same numbers read from the decision (older servers). */
export function metricsOf(d: Decision): SignalMetrics {
  const s = d.snapshot;
  const fam = d.families.map((f) => ({ key: f.key, label: f.label, score: fin(f.score) }));
  const level = (l: { price: number; touches: number } | null | undefined) => (l && fin(l.price) != null ? { price: l.price, touches: l.touches } : null);
  if (s) {
    const labels = new Map(d.families.map((f) => [f.key, f.label]));
    return {
      price: fin(s.price), composite: fin(s.composite),
      families: s.families.map((f) => ({ key: f.key, label: f.label || labels.get(f.key) || f.key, score: fin(f.score) })),
      relVolume: fin(s.relativeVolume), rsi: fin(s.rsi), support: level(s.nearestSupport), resistance: level(s.nearestResistance),
      newsScore: fin(s.newsScore), topNews: s.topNews ? { title: s.topNews.title, tone: s.topNews.tone } : null,
    };
  }
  return {
    price: fin(d.price), composite: fin(d.score?.value), families: fam, relVolume: fin(d.liquidity?.relativeVolume), rsi: null,
    support: level(d.structure?.nearestSupport), resistance: level(d.structure?.nearestResistance),
    newsScore: fam.find((f) => f.key === "news")?.score ?? null, topNews: null,
  };
}

/** Thresholds under which a change is not worth telling. */
export const CHANGE_LIMITS = { composite: 5, family: 10, volume: 0.3, rsi: 10, news: 15, level: 0.005 } as const;
const x1 = (v: number) => `${num(v, 1)}×`;
const sameLevel = (a: number, b: number) => Math.abs(a - b) / Math.max(Math.abs(a), 1e-9) <= CHANGE_LIMITS.level;

/**
 * What changed between two snapshots, most telling first: composite score, families (largest moves first, 4 at
 * most), relative volume, RSI, support / resistance (broken, crossed, confirmed or replaced) and news. Pure.
 */
export function explainChange(prev: SignalMetrics, next: SignalMetrics): string[] {
  const out: string[] = [];
  if (prev.composite != null && next.composite != null && Math.abs(next.composite - prev.composite) >= CHANGE_LIMITS.composite) {
    out.push(`Score composite ${signedScore(prev.composite)} → ${signedScore(next.composite)}`);
  }
  const before = new Map(prev.families.map((f) => [f.key, f]));
  const moves: { d: number; text: string }[] = [];
  for (const f of next.families) {
    const p = before.get(f.key);
    if (!p) continue;
    if (p.score != null && f.score != null) {
      const d = f.score - p.score;
      if (Math.abs(d) >= CHANGE_LIMITS.family) moves.push({ d: Math.abs(d), text: `${f.label} ${signedScore(d)} pts (${signedScore(p.score)} → ${signedScore(f.score)})` });
    } else if (p.score != null && f.score == null) moves.push({ d: 0, text: `${f.label} : plus mesuré(e) (données indisponibles)` });
    else if (p.score == null && f.score != null) moves.push({ d: 0, text: `${f.label} : de nouveau mesuré(e) (${signedScore(f.score)})` });
  }
  out.push(...moves.sort((a, b) => b.d - a.d).slice(0, 4).map((m) => m.text));
  if (prev.relVolume != null && next.relVolume != null && Math.abs(next.relVolume - prev.relVolume) >= CHANGE_LIMITS.volume) {
    out.push(`Volume en ${next.relVolume > prev.relVolume ? "hausse" : "baisse"} (${x1(prev.relVolume)} → ${x1(next.relVolume)} la moyenne)`);
  }
  if (prev.rsi != null && next.rsi != null && Math.abs(next.rsi - prev.rsi) >= CHANGE_LIMITS.rsi) {
    out.push(`RSI ${num(prev.rsi, 0)} → ${num(next.rsi, 0)}`);
  }
  const r0 = prev.resistance, r1 = next.resistance;
  if (r0 && next.price != null && next.price > r0.price && (prev.price == null || prev.price <= r0.price)) {
    out.push(`Résistance ${usdFr(r0.price)} franchie (prix ${usdFr(next.price)})`);
  } else if (r0 && r1 && sameLevel(r0.price, r1.price) && r1.touches > r0.touches) {
    out.push(`Résistance ${usdFr(r1.price)} confirmée (touchée ${r1.touches} fois)`);
  } else if (r0 && r1 && !sameLevel(r0.price, r1.price)) out.push(`Résistance la plus proche : ${usdFr(r0.price)} → ${usdFr(r1.price)}`);
  const s0 = prev.support, s1 = next.support;
  if (s0 && next.price != null && next.price < s0.price && (prev.price == null || prev.price >= s0.price)) {
    out.push(`Support ${usdFr(s0.price)} cassé (prix ${usdFr(next.price)})`);
  } else if (s0 && s1 && sameLevel(s0.price, s1.price) && s1.touches > s0.touches) {
    out.push(`Support ${usdFr(s1.price)} confirmé (touché ${s1.touches} fois)`);
  } else if (s0 && s1 && !sameLevel(s0.price, s1.price)) out.push(`Support le plus proche : ${usdFr(s0.price)} → ${usdFr(s1.price)}`);
  const n = next.topNews;
  if (n && n.tone !== "neutral" && n.title !== prev.topNews?.title) {
    out.push(`Actualité ${n.tone === "negative" ? "négative" : "positive"} : « ${n.title} »`);
  } else if (prev.newsScore != null && next.newsScore != null && Math.abs(next.newsScore - prev.newsScore) >= CHANGE_LIMITS.news) {
    out.push(`Ton des actualités ${signedScore(prev.newsScore)} → ${signedScore(next.newsScore)}`);
  }
  return out;
}

/** What the decision says is missing and what would change it, with its measurements. */
export function snapshotOf(d: Decision, at: number): ConfigSnapshot {
  return {
    verdict: d.verdict, level: d.level, label: d.label, levelLabel: d.levelLabel, at,
    ...(d.rating ? { rating: d.rating, ratingLabel: d.ratingLabel || d.rating } : {}),
    metrics: metricsOf(d),
    missing: d.setup.steps.filter((s) => s.state !== "ok").map((s) => `${s.label} : ${stepState[s.state] ?? s.state}${s.detail ? ` (${s.detail})` : ""}`),
    triggers: d.toBuy.map((c) => (c.level != null && !c.text.includes("$") ? `${c.text} (${usd(c.level)})` : c.text)),
  };
}

export const snapshotKey = (kind: Kind, symbol: string, personal: boolean) => `${kind}:${symbol}:${personal ? "p" : "i"}`;

/**
 * A transition when the verdict, the level or the rating (when both snapshots have one) changed; null for the first
 * sighting or the same configuration. `changes` explains it when both snapshots kept their measurements.
 */
export function diffConfiguration(prev: ConfigSnapshot | undefined, next: ConfigSnapshot, asset: { symbol: string; kind: Kind; name: string; personal: boolean }): ConfigTransition | null {
  const ratingChanged = !!prev?.rating && !!next.rating && prev.rating !== next.rating;
  if (!prev || (prev.verdict === next.verdict && prev.level === next.level && !ratingChanged)) return null;
  const pick = (s: ConfigSnapshot) => ({
    verdict: s.verdict, level: s.level, label: s.label, levelLabel: s.levelLabel, ...(s.rating ? { rating: s.rating, ratingLabel: s.ratingLabel } : {}),
  });
  const changes = prev.metrics && next.metrics ? explainChange(prev.metrics, next.metrics) : [];
  return { ...asset, at: next.at, since: prev.at, from: pick(prev), to: pick(next), missing: next.missing, triggers: next.triggers, changes };
}

// ---------- Validation on read (a damaged or older entry is dropped, never trusted) ----------

const isStrings = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === "string");
const isConfig = (v: unknown) => {
  const x = v as ConfigSnapshot;
  return !!x && VERDICTS.includes(x.verdict) && LEVELS.includes(x.level) && typeof x.label === "string" && typeof x.levelLabel === "string" &&
    (x.rating === undefined || RATINGS.includes(x.rating));
};
const isNum = (v: unknown) => v === null || (typeof v === "number" && Number.isFinite(v));
const isLevel = (v: unknown) =>
  v === null || (!!v && typeof v === "object" && Number.isFinite((v as { price: number }).price) && Number.isFinite((v as { touches: number }).touches));
const isMetrics = (v: unknown): v is SignalMetrics => {
  const x = v as SignalMetrics;
  return !!x && typeof x === "object" && isNum(x.price) && isNum(x.composite) && isNum(x.relVolume) && isNum(x.rsi) && isNum(x.newsScore) &&
    Array.isArray(x.families) && x.families.every((f) => !!f && typeof f.key === "string" && typeof f.label === "string" && isNum(f.score)) &&
    isLevel(x.support) && isLevel(x.resistance) &&
    (x.topNews === null || (!!x.topNews && typeof x.topNews.title === "string" && typeof x.topNews.tone === "string"));
};
const isSnapshot = (v: unknown): v is ConfigSnapshot => {
  const x = v as ConfigSnapshot;
  return isConfig(x) && isStrings(x.missing) && isStrings(x.triggers) && Number.isFinite(x.at);
};
const isTransition = (v: unknown): v is ConfigTransition => {
  const x = v as ConfigTransition;
  return !!x && typeof x.symbol === "string" && (x.kind === "crypto" || x.kind === "stock") && typeof x.name === "string" &&
    typeof x.personal === "boolean" && Number.isFinite(x.at) && Number.isFinite(x.since) && isConfig(x.from) && isConfig(x.to) &&
    isStrings(x.missing) && isStrings(x.triggers) && (x.changes === undefined || isStrings(x.changes));
};

/**
 * Stored state, validated: version 2, or version 1 migrated (its snapshots have no measurements, its transitions
 * no explanation). A damaged entry is dropped (damaged measurements alone are dropped from their snapshot); an
 * unknown version gives an empty state.
 */
export function parseState(raw: string | null): ConfigState {
  try {
    const p = raw ? (JSON.parse(raw) as { version?: number; last?: unknown; transitions?: unknown }) : null;
    if (!p || (p.version !== 1 && p.version !== STATE_VERSION) || typeof p.last !== "object" || !p.last || !Array.isArray(p.transitions)) return empty();
    const last = Object.fromEntries(Object.entries(p.last as Record<string, unknown>).filter(([, v]) => isSnapshot(v)).map(([k, v]) => {
      const s = v as ConfigSnapshot;
      if (s.metrics === undefined || isMetrics(s.metrics)) return [k, s];
      const { metrics: _bad, ...rest } = s;
      return [k, rest];
    })) as Record<string, ConfigSnapshot>;
    const transitions = (p.transitions as unknown[]).filter(isTransition).slice(0, MAX_TRANSITIONS).map((t) => ({ ...t, changes: t.changes ?? [] }));
    return { version: STATE_VERSION, last, transitions };
  } catch {
    return empty();
  }
}

/** New state after seeing a decision (pure): baseline replaced, transition prepended when the configuration changed. */
export function applyDecision(state: ConfigState, d: Decision, personal: boolean, now: number): { state: ConfigState; transition: ConfigTransition | null } {
  const key = snapshotKey(d.kind, d.symbol, personal);
  const next = snapshotOf(d, now);
  const transition = diffConfiguration(state.last[key], next, { symbol: d.symbol, kind: d.kind, name: d.name || d.symbol, personal });
  return {
    state: { version: STATE_VERSION, last: { ...state.last, [key]: next }, transitions: transition ? [transition, ...state.transitions].slice(0, MAX_TRANSITIONS) : state.transitions },
    transition,
  };
}

// ---------- Store (this browser only) ----------

type Storage = Pick<globalThis.Storage, "getItem" | "setItem">;
const local = (): Storage | null => {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
};
const listeners = new Set<() => void>();
let cached: { raw: string | null; state: ConfigState } | null = null;

export function readConfigState(s: Storage | null = local()): ConfigState {
  let raw: string | null = null;
  try {
    raw = s?.getItem(KEY) ?? null;
  } catch {
    /* blocked storage: nothing remembered */
  }
  if (cached && cached.raw === raw && s === local()) return cached.state;
  const state = parseState(raw);
  if (s === local()) cached = { raw, state };
  return state;
}

/** Called with every decision received (see `cacheDecision`). */
export function recordConfiguration(d: Decision, personal: boolean, now = Date.now(), s: Storage | null = local()): ConfigTransition | null {
  if (!s) return null;
  const { state, transition } = applyDecision(readConfigState(s), d, personal, now);
  try {
    s.setItem(KEY, JSON.stringify(state));
  } catch {
    return null;
  }
  listeners.forEach((l) => l());
  return transition;
}

export function clearTransitions(s: Storage | null = local()) {
  if (!s) return;
  const state = readConfigState(s);
  try {
    s.setItem(KEY, JSON.stringify({ ...state, transitions: [] }));
  } catch {
    /* quota or private browsing */
  }
  listeners.forEach((l) => l());
}

export function useTransitions(): ConfigTransition[] {
  return useSyncExternalStore(
    (l) => (listeners.add(l), () => listeners.delete(l)),
    () => readConfigState().transitions,
    () => NONE,
  );
}
const NONE: ConfigTransition[] = [];

/** "🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT" (the level, then the rating, when only it changed). */
export function transitionTitle(t: ConfigTransition): string {
  const [a, b] = t.from.verdict !== t.to.verdict
    ? [t.from.label, t.to.label]
    : t.from.level !== t.to.level || !t.from.ratingLabel || !t.to.ratingLabel
      ? [t.from.levelLabel, t.to.levelLabel]
      : [`note ${t.from.ratingLabel}`, `note ${t.to.ratingLabel}`];
  return `🚨 ${t.symbol} — changement de configuration : ${a} → ${b}`;
}

/**
 * The latest transition of this asset if it led to the configuration shown now (same verdict and level), for
 * "Pourquoi le signal a changé depuis …"; null otherwise.
 */
export function latestChange(transitions: ConfigTransition[], d: Pick<Decision, "symbol" | "kind" | "verdict" | "level" | "mode">): ConfigTransition | null {
  const personal = d.mode === "personal";
  const t = transitions.find((x) => x.symbol === d.symbol && x.kind === d.kind && x.personal === personal);
  return t && t.to.verdict === d.verdict && t.to.level === d.level ? t : null;
}
