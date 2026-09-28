/**
 * Configuration changes of the watched assets ("🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT"):
 * the last decision seen per asset (verdict, level, unmet setup steps, buy conditions) is kept in this browser and
 * compared with each new one (Radar refresh, asset page). The server is stateless: the diff is done here.
 * Pure functions + a small localStorage store (versioned key, validated on read), tested with `bun test`.
 */
import { useSyncExternalStore } from "react";
import type { Kind } from "../engine/reliability";
import type { Decision, Level, Verdict } from "./decision";

const KEY = "altim.configChanges.v1";
/** Transitions kept (newest first). */
export const MAX_TRANSITIONS = 50;

const VERDICTS: Verdict[] = ["buy", "buyZone", "wait", "noPosition", "trim", "sell"];
const LEVELS: Level[] = ["strong", "moderate", "waiting", "highRisk", "exit"];

export interface ConfigSnapshot {
  verdict: Verdict; level: Level; label: string; levelLabel: string;
  /** Setup steps not met yet ("Cassure de la résistance : pas encore"). */
  missing: string[];
  /** Conditions that would change the decision (the decision's `toBuy`). */
  triggers: string[];
  at: number;
}

export interface ConfigTransition {
  symbol: string; kind: Kind; name: string;
  /** Personal mode (with the user's average cost and weights) or market data only: compared separately. */
  personal: boolean;
  at: number;
  /** When the previous configuration was seen. */
  since: number;
  from: Pick<ConfigSnapshot, "verdict" | "level" | "label" | "levelLabel">;
  to: Pick<ConfigSnapshot, "verdict" | "level" | "label" | "levelLabel">;
  missing: string[];
  triggers: string[];
}

export interface ConfigState { version: 1; last: Record<string, ConfigSnapshot>; transitions: ConfigTransition[] }

const empty = (): ConfigState => ({ version: 1, last: {}, transitions: [] });
const stepState: Record<string, string> = { no: "pas encore", unknown: "non vérifiable" };
const usd = (v: number) => `${v.toLocaleString("fr-FR", { maximumFractionDigits: v >= 1 ? 2 : 6 })} $`;

/** What the decision says is missing and what would change it. */
export function snapshotOf(d: Decision, at: number): ConfigSnapshot {
  return {
    verdict: d.verdict, level: d.level, label: d.label, levelLabel: d.levelLabel, at,
    missing: d.setup.steps.filter((s) => s.state !== "ok").map((s) => `${s.label} : ${stepState[s.state] ?? s.state}${s.detail ? ` (${s.detail})` : ""}`),
    triggers: d.toBuy.map((c) => (c.level != null && !c.text.includes("$") ? `${c.text} (${usd(c.level)})` : c.text)),
  };
}

export const snapshotKey = (kind: Kind, symbol: string, personal: boolean) => `${kind}:${symbol}:${personal ? "p" : "i"}`;

/** A transition when the verdict or the level changed; null for the first sighting or the same configuration. */
export function diffConfiguration(prev: ConfigSnapshot | undefined, next: ConfigSnapshot, asset: { symbol: string; kind: Kind; name: string; personal: boolean }): ConfigTransition | null {
  if (!prev || (prev.verdict === next.verdict && prev.level === next.level)) return null;
  const pick = (s: ConfigSnapshot) => ({ verdict: s.verdict, level: s.level, label: s.label, levelLabel: s.levelLabel });
  return { ...asset, at: next.at, since: prev.at, from: pick(prev), to: pick(next), missing: next.missing, triggers: next.triggers };
}

// ---------- Validation on read (a damaged or older entry is dropped, never trusted) ----------

const isStrings = (v: unknown): v is string[] => Array.isArray(v) && v.every((x) => typeof x === "string");
const isConfig = (v: unknown) => {
  const x = v as ConfigSnapshot;
  return !!x && VERDICTS.includes(x.verdict) && LEVELS.includes(x.level) && typeof x.label === "string" && typeof x.levelLabel === "string";
};
const isSnapshot = (v: unknown): v is ConfigSnapshot => {
  const x = v as ConfigSnapshot;
  return isConfig(x) && isStrings(x.missing) && isStrings(x.triggers) && Number.isFinite(x.at);
};
const isTransition = (v: unknown): v is ConfigTransition => {
  const x = v as ConfigTransition;
  return !!x && typeof x.symbol === "string" && (x.kind === "crypto" || x.kind === "stock") && typeof x.name === "string" &&
    typeof x.personal === "boolean" && Number.isFinite(x.at) && Number.isFinite(x.since) && isConfig(x.from) && isConfig(x.to) &&
    isStrings(x.missing) && isStrings(x.triggers);
};

export function parseState(raw: string | null): ConfigState {
  try {
    const p = raw ? (JSON.parse(raw) as Partial<ConfigState>) : null;
    if (!p || p.version !== 1 || typeof p.last !== "object" || !p.last || !Array.isArray(p.transitions)) return empty();
    const last = Object.fromEntries(Object.entries(p.last).filter(([, v]) => isSnapshot(v))) as Record<string, ConfigSnapshot>;
    return { version: 1, last, transitions: p.transitions.filter(isTransition).slice(0, MAX_TRANSITIONS) };
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
    state: { version: 1, last: { ...state.last, [key]: next }, transitions: transition ? [transition, ...state.transitions].slice(0, MAX_TRANSITIONS) : state.transitions },
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

/** "🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT" (the level when only it changed). */
export function transitionTitle(t: ConfigTransition): string {
  const [a, b] = t.from.verdict === t.to.verdict ? [t.from.levelLabel, t.to.levelLabel] : [t.from.label, t.to.label];
  return `🚨 ${t.symbol} — changement de configuration : ${a} → ${b}`;
}
