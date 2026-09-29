/**
 * Web app state, persisted in localStorage (stays in the browser, nothing sent to the server).
 */
import { useMemo, useSyncExternalStore } from "react";
import { convert, displayCurrency, storedCurrency, type Currency } from "../money";
import { useFx } from "./fx";
import { DEFAULT_RISK, type RiskSettings } from "../engine/risk";
import type { Kind } from "../engine/reliability";
import type { Holding } from "../engine/holdings";
import { DEFAULT_SCORE_WEIGHTS, sanitizeScoreWeights, type ScoreWeights } from "./decision";

export type Interval = "1h" | "4h" | "1d";
export type WatchItem = { symbol: string; kind: Kind; name: string };
/** Investment horizon of the user: which buy zones come first (Fibonacci on 4 h, daily or weekly candles). */
export type HorizonPref = "short" | "medium" | "long";

export interface AppState {
  version: 1;
  acceptedDisclaimer: boolean;
  interval: Interval;
  watchlist: WatchItem[];
  risk: RiskSettings;
  horizon: HorizonPref;
  /** Weights of the decision's composite score (Réglages), sent as `w=` when not the defaults. */
  scoreWeights: ScoreWeights;
  /** Display currency of every amount (Réglages): euros by default, dollars on request. */
  currency: Currency;
}

export const DEFAULT_WATCHLIST: WatchItem[] = [
  { symbol: "BTC", kind: "crypto", name: "Bitcoin" },
  { symbol: "ETH", kind: "crypto", name: "Ethereum" },
  { symbol: "SOL", kind: "crypto", name: "Solana" },
  { symbol: "BNB", kind: "crypto", name: "BNB" },
  { symbol: "XRP", kind: "crypto", name: "XRP" },
  { symbol: "AAPL", kind: "stock", name: "Apple" },
  { symbol: "NVDA", kind: "stock", name: "NVIDIA" },
  { symbol: "MSFT", kind: "stock", name: "Microsoft" },
];

const KEY = "altim.webapp.v1";

const initial = (): AppState => ({
  version: 1,
  acceptedDisclaimer: false,
  interval: "4h",
  watchlist: DEFAULT_WATCHLIST,
  risk: DEFAULT_RISK,
  horizon: "medium",
  scoreWeights: DEFAULT_SCORE_WEIGHTS,
  currency: "EUR",
});

function load(): AppState {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<AppState>;
      if (parsed.version === 1) {
        return {
          ...initial(),
          acceptedDisclaimer: !!parsed.acceptedDisclaimer,
          interval: parsed.interval ?? "4h",
          watchlist: Array.isArray(parsed.watchlist) ? parsed.watchlist : DEFAULT_WATCHLIST,
          risk: { ...DEFAULT_RISK, ...parsed.risk },
          horizon: ["short", "medium", "long"].includes(parsed.horizon as string) ? parsed.horizon! : "medium",
          scoreWeights: sanitizeScoreWeights(parsed.scoreWeights),
          currency: parsed.currency === "USD" ? "USD" : "EUR",
        };
      }
    }
  } catch {}
  return initial();
}

let state: AppState = typeof window === "undefined" ? initial() : load();
const listeners = new Set<() => void>();

export function setState(update: Partial<AppState> | ((s: AppState) => Partial<AppState>)) {
  const patch = typeof update === "function" ? update(state) : update;
  state = { ...state, ...patch };
  try {
    localStorage.setItem(KEY, JSON.stringify(state));
  } catch {}
  listeners.forEach((l) => l());
}

/** Current state outside React (the alert checks). */
export const getAppState = () => state;

export function useAppState(): AppState {
  return useSyncExternalStore(
    (l) => (listeners.add(l), () => listeners.delete(l)),
    () => state,
    () => state,
  );
}

export const assetKey = (a: { symbol: string; kind: Kind }) => `${a.kind}:${a.symbol}`;

// ---------- My holdings (real portfolio entered by the user) ----------


/**
 * A line as saved: `averagePrice` and `stop` are in the currency they were typed in (`costCurrency`, `stopCurrency`;
 * absent = dollars, the only currency before the euro display). The engines get dollars (`useHoldings`).
 */
export type StoredHolding = Holding & { costCurrency?: Currency; stopCurrency?: Currency };

export interface HoldingsState {
  version: 1;
  /** Cash in `cashCurrency` (absent = dollars). */
  cash: number;
  cashCurrency?: Currency;
  holdings: StoredHolding[];
  updatedAt: number;
}

const HOLDINGS_KEY = "altim.holdings.v1";

function loadHoldings(): HoldingsState {
  try {
    const raw = localStorage.getItem(HOLDINGS_KEY);
    if (raw) {
      const p = JSON.parse(raw) as HoldingsState;
      if (p.version === 1 && Array.isArray(p.holdings)) return { ...p, holdings: p.holdings.filter(isValidHolding).map(cleanStop) };
    }
  } catch {}
  return { version: 1, cash: 0, holdings: [], updatedAt: 0 };
}

/** The optional stop is dropped when it is not a positive number (the line itself stays); unknown currency tags too. */
export function cleanStop<H extends StoredHolding>(h: H): H {
  if (h.costCurrency !== undefined && h.costCurrency !== "EUR" && h.costCurrency !== "USD") h = { ...h, costCurrency: undefined };
  if (h.stopCurrency !== undefined && h.stopCurrency !== "EUR" && h.stopCurrency !== "USD") h = { ...h, stopCurrency: undefined };
  if (h.stop === undefined || (Number.isFinite(h.stop) && h.stop > 0)) return h;
  const { stop: _, stopCurrency: __, ...rest } = h;
  return rest as H;
}

export function isValidHolding(h: unknown): h is Holding {
  const x = h as Holding;
  return !!x && typeof x.id === "string" && typeof x.symbol === "string" && (x.kind === "crypto" || x.kind === "stock") &&
    Number.isFinite(x.quantity) && x.quantity > 0 && Number.isFinite(x.averagePrice) && x.averagePrice >= 0;
}

let holdingsState: HoldingsState = typeof window === "undefined" ? { version: 1, cash: 0, holdings: [], updatedAt: 0 } : loadHoldings();
const holdingsListeners = new Set<() => void>();

export function setHoldings(update: Partial<Omit<HoldingsState, "version">> | ((s: HoldingsState) => Partial<HoldingsState>)) {
  const patch = typeof update === "function" ? update(holdingsState) : update;
  holdingsState = { ...holdingsState, ...patch, version: 1, updatedAt: Date.now() };
  try {
    localStorage.setItem(HOLDINGS_KEY, JSON.stringify(holdingsState));
  } catch {}
  holdingsListeners.forEach((l) => l());
}

/** Assets held, outside React (the alert checks). */
export const getHoldingAssets = () => holdingsState.holdings.map((h) => ({ symbol: h.symbol, kind: h.kind, name: h.name }));

/** Holdings exactly as saved (amounts in their own currency): the editing forms. */
export function useStoredHoldings(): HoldingsState {
  return useSyncExternalStore(
    (l) => (holdingsListeners.add(l), () => holdingsListeners.delete(l)),
    () => holdingsState,
    () => holdingsState,
  );
}

/** Dollars view for the engines and the server, plus what could not be converted. */
export interface UsdHoldings {
  holdings: Holding[];
  cash: number;
  updatedAt: number;
  /** Symbols whose euro cost could not be converted (no rate): their cost is left at 0 and their P&L is not shown. */
  unconverted: string[];
  /** The euro cash could not be converted (no rate): counted as 0. */
  cashUnconverted: boolean;
}

/**
 * Saved amounts in dollars at the current rate: a euro cost basis becomes `cost ÷ rate`, so that the dollar P&L,
 * converted back at the same rate, is exactly the euro P&L (price in euros today − cost in euros, currency effect
 * included). Pure: `rate` = euros per dollar, null when unknown.
 */
export function toUsdHoldings(s: HoldingsState): UsdHoldings {
  const unconverted: string[] = [];
  const holdings = s.holdings.map((h) => {
    const { costCurrency, stopCurrency, ...line } = h;
    let averagePrice = convert(h.averagePrice, storedCurrency(costCurrency), "USD");
    if (!Number.isFinite(averagePrice)) {
      unconverted.push(h.symbol);
      averagePrice = 0;
    }
    const out: Holding = { ...line, averagePrice };
    if (h.stop !== undefined) {
      const stop = convert(h.stop, storedCurrency(stopCurrency), "USD");
      if (Number.isFinite(stop) && stop > 0) out.stop = stop;
      else delete out.stop;
    }
    return out;
  });
  const cash = convert(s.cash, storedCurrency(s.cashCurrency), "USD");
  return { holdings, cash: Number.isFinite(cash) ? cash : 0, updatedAt: s.updatedAt, unconverted, cashUnconverted: !Number.isFinite(cash) };
}

/** Holdings in dollars (engines, server, decision): recomputed when the holdings or the rate change. */
export function useHoldings(): UsdHoldings {
  const s = useStoredHoldings();
  const { fx } = useFx();
  const cur = displayCurrency();
  return useMemo(() => toUsdHoldings(s), [s, fx, cur]);
}

/** A saved amount shown in the display currency (NaN when no rate allows it). */
export const shown = (v: number, c: Currency | undefined) => convert(v, storedCurrency(c), displayCurrency());

type NewLine = Omit<StoredHolding, "id">;

/**
 * A second purchase merged into a line: weighted average cost in the currency of the new purchase (the previous cost
 * converted at the current rate when it was typed in another currency). Null when no rate allows the conversion.
 */
export function mergeLine(x: StoredHolding, h: Pick<NewLine, "quantity" | "averagePrice" | "costCurrency">): StoredHolding | null {
  const cur = storedCurrency(h.costCurrency);
  const prev = convert(x.averagePrice, storedCurrency(x.costCurrency), cur);
  if (!Number.isFinite(prev)) return null;
  const qty = x.quantity + h.quantity;
  return { ...x, quantity: qty, averagePrice: (x.quantity * prev + h.quantity * h.averagePrice) / qty, costCurrency: cur };
}

/** Adds or updates a line (a second purchase of the same asset recomputes the weighted average cost). */
export function upsertHolding(h: NewLine & { id?: string }, merge = false) {
  setHoldings((s) => {
    const existing = s.holdings.find((x) => (h.id ? x.id === h.id : x.symbol === h.symbol && x.kind === h.kind));
    if (existing && merge && !h.id) {
      const merged = mergeLine(existing, h);
      if (merged) return { holdings: s.holdings.map((x) => (x.id === existing.id ? merged : x)) };
      return { holdings: [...s.holdings, { ...h, id: crypto.randomUUID() }] };
    }
    if (existing) return { holdings: s.holdings.map((x) => (x.id === existing.id ? cleanStop({ ...existing, ...h, id: existing.id }) : x)) };
    return { holdings: [...s.holdings, { ...h, id: crypto.randomUUID() }] };
  });
}

/** Adds several lines at once; an asset already held is merged (quantities added, weighted average cost). */
export function addHoldings(items: NewLine[]) {
  setHoldings((s) => {
    const holdings = [...s.holdings];
    for (const h of items) {
      const i = holdings.findIndex((x) => x.symbol === h.symbol && x.kind === h.kind);
      const merged = i < 0 ? null : mergeLine(holdings[i]!, h);
      if (merged) holdings[i] = merged;
      else holdings.push({ ...h, id: crypto.randomUUID() });
    }
    return { holdings };
  });
}

export function exportHoldings(): string {
  return JSON.stringify({ app: "altim", ...holdingsState }, null, 2);
}

export function importHoldings(json: string): string | null {
  try {
    const p = JSON.parse(json) as Partial<HoldingsState>;
    if (!Array.isArray(p.holdings)) return "Fichier invalide.";
    const holdings = p.holdings.filter(isValidHolding).map(cleanStop);
    setHoldings({
      holdings, cash: Number.isFinite(p.cash) && (p.cash as number) >= 0 ? (p.cash as number) : 0,
      cashCurrency: p.cashCurrency === "EUR" ? "EUR" : undefined,
    });
    return null;
  } catch {
    return "Fichier illisible.";
  }
}
