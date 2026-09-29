/**
 * « Alertes » of the web app (same rules as the iPhone and Android Alerts tab, AltimKit PriceAlerts / AlertTracker /
 * NewsAlertTracker): price alerts chosen by the user, which buy alerts and news were already notified, and the
 * journal of the alerts received with what each one gave since. Kept in this browser only (localStorage
 * "altim.alerts.v1"); the pure functions are tested with `bun test`.
 */
import { useSyncExternalStore } from "react";
import { convert, storedCurrency, type Currency } from "../money";
import type { Kind } from "../engine/reliability";
import type { BuyAlert } from "./api";
import type { NewsItem } from "../engine/news";

export const ALERTS_KEY = "altim.alerts.v1";

/**
 * A price alert. `price` is in `currency` (the display currency when it was created): the current dollar price is
 * converted at the current rate before comparing, so a threshold in euros is reached by the price in euros.
 */
export interface PriceTarget {
  id: string;
  symbol: string;
  kind: Kind;
  name: string;
  /** true: reached when the price rises to or above `price`; false: when it falls to or below. */
  above: boolean;
  price: number;
  currency: Currency;
  created: number;
  triggered: number | null;
  /** Move alert: reached when the price moves by at least this many % (up or down) from `price`. */
  move: number | null;
}

export type JournalSource = "buy" | "strongBuy" | "target";
/** One alert received; `price` in dollars (the sources' currency) when it was sent. */
export interface AlertEntry { id: string; symbol: string; kind: Kind; name: string; source: JournalSource; title: string; price: number; date: number }

export interface AlertsState {
  version: 1;
  /** Notifications while the tab is open (the browser cannot run Altim when it is closed). */
  notify: { buy: boolean; strongOnly: boolean; news: boolean };
  targets: PriceTarget[];
  journal: AlertEntry[];
  /** Asset → reasons already notified ("signal+zone:medium"), and since when it is no longer buyable. */
  tracker: { notified: Record<string, string>; lost: Record<string, number> };
  newsSeen: { id: string; words: string[]; time: number }[];
  lastCheck: number | null;
}

export const emptyAlerts = (): AlertsState => ({
  version: 1, notify: { buy: false, strongOnly: false, news: false }, targets: [], journal: [], tracker: { notified: {}, lost: {} }, newsSeen: [], lastCheck: null,
});

const id = (a: { symbol: string; kind: Kind }) => `${a.kind}:${a.symbol}`;

// ---------- Price alerts ----------

/** "Au-dessus de 80 000 €" / "Variation de ±5 % (depuis 212,40 €)". */
export function targetLabel(t: PriceTarget, fmt: (v: number, c: Currency) => string): string {
  if (t.move != null) return `Variation de ±${t.move.toLocaleString("fr-FR", { maximumFractionDigits: 1 })} % (depuis ${fmt(t.price, t.currency)})`;
  return `${t.above ? "Au-dessus de" : "En dessous de"} ${fmt(t.price, t.currency)}`;
}

/** The dollar price in the alert's currency (NaN when no rate allows it: the alert then waits). */
export const inTargetCurrency = (t: PriceTarget, usd: number) => convert(usd, "USD", storedCurrency(t.currency));

export function isReached(t: PriceTarget, usd: number): boolean {
  const p = inTargetCurrency(t, usd);
  if (!Number.isFinite(p)) return false;
  if (t.move != null) return t.price > 0 && Math.abs(p / t.price - 1) * 100 >= t.move;
  return t.above ? p >= t.price : p <= t.price;
}

/** Re-armed: a move alert starts again from the current price. */
export function rearm(t: PriceTarget, usd: number | null): PriceTarget {
  const p = usd == null ? NaN : inTargetCurrency(t, usd);
  return { ...t, triggered: null, ...(t.move != null && Number.isFinite(p) && p > 0 ? { price: p } : {}) };
}

/** Armed targets reached at the given dollar prices (key "kind:SYMBOL"): updated list and those just reached. */
export function evaluateTargets(targets: PriceTarget[], prices: Record<string, number>, now: number): { targets: PriceTarget[]; fired: { target: PriceTarget; price: number }[] } {
  const fired: { target: PriceTarget; price: number }[] = [];
  const updated = targets.map((t) => {
    const p = prices[id(t)];
    if (t.triggered != null || p == null || !Number.isFinite(p) || !isReached(t, p)) return t;
    const done = { ...t, triggered: now };
    fired.push({ target: done, price: p });
    return done;
  });
  return { targets: updated, fired };
}

// ---------- Buy alerts: each reason notified once (AlertTracker) ----------

export const BUY_COOLDOWN_MS = 6 * 3_600_000;
const parts = (key: string) => new Set(key.split("+").filter(Boolean));

export function newBuyAlerts(tracker: AlertsState["tracker"], items: BuyAlert[], onlyStrong: boolean, now: number): { tracker: AlertsState["tracker"]; fresh: BuyAlert[] } {
  const notified = { ...tracker.notified };
  const lost = { ...tracker.lost };
  const fresh: BuyAlert[] = [];
  for (const it of items) {
    const k = id(it);
    if (it.buy && (!onlyStrong || it.strong)) {
      delete lost[k];
      const before = notified[k] != null ? parts(notified[k]!) : null;
      const reasons = parts(it.key ?? "");
      if (!before || [...reasons].some((r) => !before.has(r))) fresh.push(it);
      notified[k] = [...new Set([...(before ?? []), ...reasons])].sort().join("+");
    } else if (!it.buy && notified[k] != null) {
      const since = lost[k] ?? now;
      lost[k] = since;
      if (now - since >= BUY_COOLDOWN_MS) {
        delete notified[k];
        delete lost[k];
      }
    }
  }
  return { tracker: { notified, lost }, fresh };
}

// ---------- News worth a notification (NewsAlertTracker) ----------

export const NEWS_FRESH_MS = 6 * 3_600_000;
const NEWS_MEMORY_MS = 48 * 3_600_000;
const words = (title: string) => new Set(title.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter((w) => w.length >= 4));
const similar = (a: Set<string>, b: Set<string>) => {
  if (!a.size || !b.size) return false;
  const inter = [...a].filter((w) => b.has(w)).length;
  return inter / new Set([...a, ...b]).size >= 0.5;
};

/** A grave escalation told by at least 2 sources, or a story on one of the user's assets told by at least 3. */
export function newsReason(item: NewsItem, owned: Set<string>): "alert" | "asset" | null {
  if (item.alert && item.alsoIn.length > 0) return "alert";
  if (item.alsoIn.length >= 2 && item.assets.some((a) => owned.has(a))) return "asset";
  return null;
}

export function newNews(seen: AlertsState["newsSeen"], items: NewsItem[], owned: Set<string>, now: number): { seen: AlertsState["newsSeen"]; fresh: NewsItem[] } {
  const kept = seen.filter((s) => now - s.time < NEWS_MEMORY_MS);
  const fresh: NewsItem[] = [];
  for (const item of items) {
    if (now - item.time > NEWS_FRESH_MS || !newsReason(item, owned)) continue;
    const w = words(item.title);
    if (kept.some((s) => s.id === item.id || similar(new Set(s.words), w))) continue;
    kept.push({ id: item.id, words: [...w].sort(), time: now });
    fresh.push(item);
  }
  return { seen: kept, fresh };
}

// ---------- Journal ----------

export const JOURNAL_LIMIT = 200;
export const addToJournal = (entries: AlertEntry[], journal: AlertEntry[]) => [...entries, ...journal].sort((a, b) => b.date - a.date).slice(0, JOURNAL_LIMIT);

/** Change since the alert, in % (dollar prices both: the currency does not bias it). */
export const changeSince = (e: AlertEntry, usd: number | null | undefined) => (usd != null && Number.isFinite(usd) && e.price > 0 ? (usd / e.price - 1) * 100 : null);

/**
 * Buy alerts only (price targets are the user's own thresholds): how many went up since, average change. Alerts of
 * less than an hour say nothing yet. Without fees nor exit rule: an indication, not a backtest.
 */
export function journalSummary(journal: AlertEntry[], prices: Record<string, number>, now: number, minAgeMs = 3_600_000) {
  const changes = journal
    .filter((e) => e.source !== "target" && now - e.date >= minAgeMs)
    .map((e) => changeSince(e, prices[id(e)]))
    .filter((c): c is number => c != null);
  if (!changes.length) return null;
  const up = changes.filter((c) => c > 0).length;
  return { count: changes.length, up, upShare: (up / changes.length) * 100, average: changes.reduce((a, b) => a + b, 0) / changes.length };
}

// ---------- Storage ----------

/** A saved state, fields checked one by one (localStorage can be edited by hand); anything unreadable is dropped. */
export function parseAlerts(raw: string | null): AlertsState {
  const base = emptyAlerts();
  if (!raw) return base;
  try {
    const p = JSON.parse(raw) as Partial<AlertsState>;
    if (p.version !== 1) return base;
    const kindOk = (k: unknown): k is Kind => k === "crypto" || k === "stock";
    const targets = (Array.isArray(p.targets) ? p.targets : []).filter((t): t is PriceTarget =>
      !!t && typeof t.id === "string" && typeof t.symbol === "string" && kindOk(t.kind) && typeof t.price === "number" && Number.isFinite(t.price) && t.price > 0,
    ).map((t) => ({ ...t, currency: storedCurrency(t.currency), triggered: typeof t.triggered === "number" ? t.triggered : null, move: typeof t.move === "number" && t.move > 0 ? t.move : null, above: !!t.above }));
    const journal = (Array.isArray(p.journal) ? p.journal : []).filter((e): e is AlertEntry =>
      !!e && typeof e.symbol === "string" && kindOk(e.kind) && typeof e.price === "number" && typeof e.date === "number",
    );
    const n = (p.notify ?? {}) as Partial<AlertsState["notify"]>;
    return {
      ...base,
      notify: { buy: !!n.buy, strongOnly: !!n.strongOnly, news: !!n.news },
      targets, journal: journal.slice(0, JOURNAL_LIMIT),
      tracker: p.tracker && typeof p.tracker === "object" ? { notified: { ...p.tracker.notified }, lost: { ...p.tracker.lost } } : base.tracker,
      newsSeen: Array.isArray(p.newsSeen) ? p.newsSeen.filter((s) => s && typeof s.id === "string" && Array.isArray(s.words)) : [],
      lastCheck: typeof p.lastCheck === "number" ? p.lastCheck : null,
    };
  } catch {
    return base;
  }
}

function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

let state: AlertsState = parseAlerts(storage()?.getItem(ALERTS_KEY) ?? null);
const listeners = new Set<() => void>();

export const getAlerts = () => state;

export function setAlerts(update: Partial<AlertsState> | ((s: AlertsState) => Partial<AlertsState>)) {
  state = { ...state, ...(typeof update === "function" ? update(state) : update), version: 1 };
  try {
    storage()?.setItem(ALERTS_KEY, JSON.stringify(state));
  } catch {}
  listeners.forEach((l) => l());
}

export function useAlerts(): AlertsState {
  return useSyncExternalStore(
    (l) => (listeners.add(l), () => listeners.delete(l)),
    () => state,
    () => state,
  );
}
