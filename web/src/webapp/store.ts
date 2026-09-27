/**
 * Web app state, persisted in localStorage (stays in the browser, nothing sent to the server).
 */
import { useSyncExternalStore } from "react";
import { DEFAULT_RISK, type RiskSettings } from "../engine/risk";
import type { Kind } from "../engine/reliability";
import type { Holding } from "../engine/holdings";

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

export function useAppState(): AppState {
  return useSyncExternalStore(
    (l) => (listeners.add(l), () => listeners.delete(l)),
    () => state,
    () => state,
  );
}

export const assetKey = (a: { symbol: string; kind: Kind }) => `${a.kind}:${a.symbol}`;

// ---------- My holdings (real portfolio entered by the user) ----------


export interface HoldingsState {
  version: 1;
  cash: number;
  holdings: Holding[];
  updatedAt: number;
}

const HOLDINGS_KEY = "altim.holdings.v1";

function loadHoldings(): HoldingsState {
  try {
    const raw = localStorage.getItem(HOLDINGS_KEY);
    if (raw) {
      const p = JSON.parse(raw) as HoldingsState;
      if (p.version === 1 && Array.isArray(p.holdings)) return { ...p, holdings: p.holdings.filter(isValidHolding) };
    }
  } catch {}
  return { version: 1, cash: 0, holdings: [], updatedAt: 0 };
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

export function useHoldings(): HoldingsState {
  return useSyncExternalStore(
    (l) => (holdingsListeners.add(l), () => holdingsListeners.delete(l)),
    () => holdingsState,
    () => holdingsState,
  );
}

/** Adds or updates a line (a second purchase of the same asset recomputes the weighted average cost). */
export function upsertHolding(h: Omit<Holding, "id"> & { id?: string }, merge = false) {
  setHoldings((s) => {
    const existing = s.holdings.find((x) => (h.id ? x.id === h.id : x.symbol === h.symbol && x.kind === h.kind));
    if (existing && merge && !h.id) {
      const qty = existing.quantity + h.quantity;
      const avg = (existing.quantity * existing.averagePrice + h.quantity * h.averagePrice) / qty;
      return { holdings: s.holdings.map((x) => (x.id === existing.id ? { ...x, quantity: qty, averagePrice: avg } : x)) };
    }
    if (existing) return { holdings: s.holdings.map((x) => (x.id === existing.id ? { ...existing, ...h, id: existing.id } : x)) };
    return { holdings: [...s.holdings, { ...h, id: crypto.randomUUID() }] };
  });
}

/** Adds several lines at once; an asset already held is merged (quantities added, weighted average cost). */
export function addHoldings(items: Omit<Holding, "id">[]) {
  setHoldings((s) => {
    const holdings = [...s.holdings];
    for (const h of items) {
      const i = holdings.findIndex((x) => x.symbol === h.symbol && x.kind === h.kind);
      if (i < 0) {
        holdings.push({ ...h, id: crypto.randomUUID() });
        continue;
      }
      const x = holdings[i]!;
      const qty = x.quantity + h.quantity;
      holdings[i] = { ...x, quantity: qty, averagePrice: (x.quantity * x.averagePrice + h.quantity * h.averagePrice) / qty };
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
    const holdings = p.holdings.filter(isValidHolding);
    setHoldings({ holdings, cash: Number.isFinite(p.cash) && (p.cash as number) >= 0 ? (p.cash as number) : 0 });
    return null;
  } catch {
    return "Fichier illisible.";
  }
}
