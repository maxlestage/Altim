/**
 * Automatic journal (engine/journal.ts), kept in this browser only (localStorage "altim.journal.v1"); nothing is
 * sent to the server. Unreadable saved data is ignored (kept aside as "altim.journal.v1.invalid" at the next write).
 * Entries are written when a simulated purchase is confirmed or a real purchase / sale is saved in « Mes avoirs »,
 * then completed in the background with the market data of that moment (macro stress, ATR, relative volume, and
 * the decision when none was loaded yet); a failed fetch only leaves the field unknown.
 */
import { useSyncExternalStore } from "react";
import {
  addEntry, createEntry, emptyJournal, isJournalState, marketFromCandles, marketFromDecision, patchEntry, removeEntry, snapshotDecision,
  type JournalMarket, type JournalState, type NewEntry,
} from "../engine/journal";
import type { Kind } from "../engine/reliability";
import { api } from "./api";
import { cachedDecision, type Decision } from "./decision";
import { newId } from "./paper-ui";

export const JOURNAL_KEY = "altim.journal.v1";
/** A decision cached on the device is used for an entry when it is younger than this. */
const DECISION_MAX_AGE = 6 * 3_600_000;

type Saved = { state: JournalState; error: string | null };

export function parseSavedJournal(raw: string | null): Saved {
  if (raw == null || raw === "") return { state: emptyJournal(), error: null };
  try {
    const v: unknown = JSON.parse(raw);
    if (isJournalState(v)) return { state: v, error: null };
  } catch {}
  return { state: emptyJournal(), error: "Le journal enregistré dans ce navigateur est illisible : il a été ignoré." };
}

function read(): Saved {
  try {
    return parseSavedJournal(localStorage.getItem(JOURNAL_KEY));
  } catch {
    return { state: emptyJournal(), error: null };
  }
}

let current: Saved = typeof window === "undefined" ? { state: emptyJournal(), error: null } : read();
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

if (typeof window !== "undefined") {
  window.addEventListener("storage", (e) => {
    if (e.key !== JOURNAL_KEY) return;
    current = read();
    emit();
  });
}

export function useJournal(): Saved {
  return useSyncExternalStore(
    (l) => (listeners.add(l), () => listeners.delete(l)),
    () => current,
    () => current,
  );
}

function save(update: (s: JournalState) => JournalState) {
  const state = update(current.state);
  try {
    if (current.error) {
      const bad = localStorage.getItem(JOURNAL_KEY);
      if (bad != null) localStorage.setItem(`${JOURNAL_KEY}.invalid`, bad);
    }
    localStorage.setItem(JOURNAL_KEY, JSON.stringify(state));
  } catch {}
  current = { state, error: null };
  emit();
}

export const setJournalNote = (id: string, note: string) => save((s) => patchEntry(s, id, { note }));
export const deleteJournalEntry = (id: string) => save((s) => removeEntry(s, id));

/** Market data of the moment, fetched right after the entry is written (cached by the server). */
async function enrich(id: string, symbol: string, kind: Kind, at: number, hasDecision: boolean) {
  const [macro, candles, decision] = await Promise.all([
    api.macro().catch(() => null),
    api.candles(symbol, kind, "1d").catch(() => null),
    hasDecision ? Promise.resolve(null) : api.decision(symbol, kind).catch(() => null),
  ]);
  const market: Partial<JournalMarket> = {};
  if (macro) {
    market.macroScore = macro.score;
    market.macroLevel = macro.level;
  }
  if (candles?.candles.length) {
    const m = marketFromCandles(candles.candles, at);
    market.atrPct = m.atrPct;
    const entry = current.state.entries.find((e) => e.id === id);
    if (entry?.market.relativeVolume == null) market.relativeVolume = m.relativeVolume;
  }
  if (decision) Object.assign(market, Object.fromEntries(Object.entries(marketFromDecision(decision)).filter(([, v]) => v != null)));
  const entry = current.state.entries.find((e) => e.id === id);
  if (entry && entry.market.regime == null && market.regime == null && macro?.regime) {
    market.regime = macro.regime.kind;
    market.regimeLabel = macro.regime.label;
  }
  save((s) => patchEntry(s, id, { market, ...(decision ? { decision: snapshotDecision(decision) } : {}) }));
}

/** Writes an entry and completes it in the background. Returns its id. */
export function record(n: Omit<NewEntry, "id" | "now"> & { now?: number }): string {
  const e = createEntry({ ...n, id: newId(), now: n.now ?? Date.now() });
  save((s) => addEntry(s, e));
  void enrich(e.id, e.symbol, e.kind, e.createdAt, e.decision != null).catch(() => {});
  return e.id;
}

/** The decision seen on this device for this asset, when recent enough to be "the one of the moment". */
export function recentDecision(kind: Kind, symbol: string, now = Date.now()): Decision | null {
  const c = cachedDecision(kind, symbol);
  return c && now - c.at <= DECISION_MAX_AGE && now - c.decision.asOf <= DECISION_MAX_AGE ? c.decision : null;
}

/** A real purchase or sale saved in « Mes avoirs ». */
export function recordRealTrade(t: { side: "buy" | "sell"; symbol: string; kind: Kind; name: string; price: number; quantity: number; stop?: number | null; note?: string; refId?: string | null }) {
  if (!(t.price > 0) || !(t.quantity > 0)) return null;
  return record({
    source: "real", side: t.side, symbol: t.symbol, kind: t.kind, name: t.name, price: t.price, quantity: t.quantity,
    amount: t.price * t.quantity, stop: t.stop ?? null, note: t.note, refId: t.refId ?? null, decision: recentDecision(t.kind, t.symbol),
  });
}
