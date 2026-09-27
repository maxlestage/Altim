/**
 * Web app state, persisted in localStorage (stays in the browser, nothing sent to the server).
 */
import { useSyncExternalStore } from "react";
import { DEFAULT_RISK, type RiskSettings } from "../engine/risk";
import type { Kind } from "../engine/reliability";
import type { Holding } from "../engine/holdings";

export type Interval = "1h" | "4h" | "1d";
export type WatchItem = { symbol: string; kind: Kind; name: string };
export type Position = { symbol: string; kind: Kind; name: string; quantity: number; averagePrice: number; stopLoss?: number; takeProfit?: number };
export type JournalEntry = { id: string; time: number; symbol: string; kind: Kind; side: "buy" | "sell"; quantity: number; price: number; fee: number };

export interface AppState {
  version: 1;
  acceptedDisclaimer: boolean;
  interval: Interval;
  watchlist: WatchItem[];
  risk: RiskSettings;
  cash: number;
  positions: Position[];
  journal: JournalEntry[];
}

export const STARTING_CASH = 10_000;
export const FEE_RATE = 0.001;

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
  cash: STARTING_CASH,
  positions: [],
  journal: [],
});

function load(): AppState {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<AppState>;
      if (parsed.version === 1) return { ...initial(), ...parsed, risk: { ...DEFAULT_RISK, ...parsed.risk } };
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

export function resetDemo() {
  setState({ cash: STARTING_CASH, positions: [], journal: [] });
}

export function useAppState(): AppState {
  return useSyncExternalStore(
    (l) => (listeners.add(l), () => listeners.delete(l)),
    () => state,
    () => state,
  );
}

export const assetKey = (a: { symbol: string; kind: Kind }) => `${a.kind}:${a.symbol}`;

/** Demo order execution (simulated fill at the consensus price, 0.1 % fees). */
export function executeDemo(order: { symbol: string; kind: Kind; name: string; side: "buy" | "sell"; quantity: number; price: number; stopLoss?: number; takeProfit?: number }): string | null {
  const s = state;
  const notional = order.quantity * order.price;
  const fee = notional * FEE_RATE;
  if (!(order.quantity > 0) || !(order.price > 0)) return "Quantité ou prix invalide.";
  const key = assetKey(order);
  const existing = s.positions.find((p) => assetKey(p) === key);
  if (order.side === "buy") {
    if (notional + fee > s.cash + 1e-9) return "Solde insuffisant.";
    const qty = (existing?.quantity ?? 0) + order.quantity;
    const avg = existing ? (existing.quantity * existing.averagePrice + notional) / qty : order.price;
    const position: Position = { symbol: order.symbol, kind: order.kind, name: order.name, quantity: qty, averagePrice: avg, stopLoss: order.stopLoss, takeProfit: order.takeProfit };
    setState({
      cash: s.cash - notional - fee,
      positions: [...s.positions.filter((p) => assetKey(p) !== key), position],
    });
  } else {
    if (!existing || existing.quantity + 1e-12 < order.quantity) return "Quantité détenue insuffisante.";
    const remaining = existing.quantity - order.quantity;
    setState({
      cash: s.cash + notional - fee,
      positions: remaining > 1e-12 ? s.positions.map((p) => (assetKey(p) === key ? { ...p, quantity: remaining } : p)) : s.positions.filter((p) => assetKey(p) !== key),
    });
  }
  setState((st) => ({
    journal: [{ id: crypto.randomUUID(), time: Date.now(), symbol: order.symbol, kind: order.kind, side: order.side, quantity: order.quantity, price: order.price, fee }, ...st.journal].slice(0, 200),
  }));
  return null;
}

/** P&L realized today (average-cost method), as in TradeJournal.swift. */
export function realizedPnLToday(journal: JournalEntry[]): number {
  const pos = new Map<string, { qty: number; cost: number }>();
  const today = new Date().toDateString();
  let pnl = 0;
  for (const e of [...journal].reverse()) {
    const k = assetKey(e);
    const p = pos.get(k) ?? { qty: 0, cost: 0 };
    if (e.side === "buy") {
      p.qty += e.quantity;
      p.cost += e.quantity * e.price;
    } else if (p.qty > 0) {
      const avg = p.cost / p.qty;
      const sold = Math.min(e.quantity, p.qty);
      if (new Date(e.time).toDateString() === today) pnl += sold * (e.price - avg);
      p.qty -= sold;
      p.cost -= sold * avg;
    }
    pos.set(k, p);
  }
  return pnl;
}

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
