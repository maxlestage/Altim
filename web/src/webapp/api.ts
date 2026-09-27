/** Client for the Altim APIs (multi-source consensus computed on the server). */
import type { Candle } from "../engine/signal";
import type { Kind, QualityReport, Reliability } from "../engine/reliability";
import type { Interval, WatchItem } from "./store";

export type SourceStatus = { name: string; ok: boolean; deviation?: number; error?: string };
export type Snapshot = {
  symbol: string; kind: Kind; interval: Interval; candles: Candle[]; source: string;
  sources: SourceStatus[]; agreeing: number; quality: QualityReport; reliability: Reliability;
};
export type RadarRow = {
  symbol: string; kind: Kind; name: string; price: number | null; change: number | null; priceSources: string | null;
  signal?: { action: import("../engine/signal").Action; score: number; confidence: number } | null;
  reliability?: Reliability; agreeing?: number; sources?: number; sparkline?: number[]; error?: string;
};
export type Quote = { symbol: string; kind: Kind; name: string; price: number; change: number | null; agreeing: number; total: number; sources: { name: string; ok: boolean; price?: number; error?: string }[] };
export type Sentiment = { fearGreed?: { value: number; label: string }; social?: { bullishPercent: number | null; sample: number } };

export type GuardReport = import("../engine/guard").GuardResult & {
  symbol: string; kind: Kind; asOf: number; price: number | null;
  inputs: {
    fundingRate: number | null; longShortRatio: number | null; openInterestUsd: number | null; fearGreed: number | null;
    socialBullish: number | null; socialSample: number; news24h: number; newsTone: { negative: number; positive: number };
    headlines: { title: string; time: number; source?: string }[]; vix: number | null;
  };
  macro: MacroInfo | null;
};

export type MacroInfo = import("../engine/macro").MacroReport & { evidence?: import("../engine/guard").Evidence | null };
export type ZonesReport = {
  symbol: string; kind: Kind; price: number | null; asOf: number;
  zones: (import("../engine/fibonacci").FibZone & { macroNote: string | null })[];
  macro: MacroInfo | null;
};

export type UniverseItem = { symbol: string; name: string; kind: Kind; rank: number | null; etf?: boolean; exchanges?: number };

async function get<T>(url: string): Promise<T> {
  const r = await fetch(url);
  const body = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error((body as { error?: string }).error ?? `Erreur ${r.status}`);
  return body as T;
}

const list = (items: { symbol: string; kind: Kind }[]) => encodeURIComponent(items.map((i) => `${i.symbol}:${i.kind}`).join(","));

/** The server accepts 20 assets per request: larger portfolios are split into parallel batches. */
const BATCH = 20;
async function batched<I, T>(items: I[], call: (chunk: I[]) => Promise<T[]>): Promise<T[]> {
  const chunks: I[][] = [];
  for (let i = 0; i < items.length; i += BATCH) chunks.push(items.slice(i, i + BATCH));
  return (await Promise.all(chunks.map(call))).flat();
}

export const api = {
  radar: (items: WatchItem[], interval: Interval) => batched(items, (c) => get<RadarRow[]>(`/api/radar?symbols=${list(c)}&interval=${interval}`)),
  candles: (symbol: string, kind: Kind, interval: Interval) => get<Snapshot>(`/api/candles?symbol=${encodeURIComponent(symbol)}&kind=${kind}&interval=${interval}`),
  quotes: (items: { symbol: string; kind: Kind }[]) => batched(items, (c) => get<Quote[]>(`/api/tickers?symbols=${list(c)}`)),
  search: (q: string, limit = 20) => get<UniverseItem[]>(`/api/search?q=${encodeURIComponent(q)}&limit=${limit}`),
  universe: (kind: Kind, q: string, offset: number, limit: number) =>
    get<{ total: number; offset: number; items: UniverseItem[] }>(`/api/universe?kind=${kind}&q=${encodeURIComponent(q)}&offset=${offset}&limit=${limit}`),
  guard: (symbol: string, kind: Kind) => get<GuardReport>(`/api/guard?symbol=${encodeURIComponent(symbol)}&kind=${kind}`),
  zones: (symbol: string, kind: Kind) => get<ZonesReport>(`/api/zones?symbol=${encodeURIComponent(symbol)}&kind=${kind}`),
  macro: () => get<MacroInfo>("/api/macro"),
  sentiment: (symbol: string, kind: Kind) => get<Sentiment>(`/api/sentiment?symbol=${encodeURIComponent(symbol)}&kind=${kind}`),
};

export const HIGHER: Record<Interval, Interval | null> = { "1h": "4h", "4h": "1d", "1d": null };
export const STEP_MS: Record<Interval, number> = { "1h": 3_600_000, "4h": 14_400_000, "1d": 86_400_000 };
export const INTERVAL_LABEL: Record<Interval, string> = { "1h": "1 h", "4h": "4 h", "1d": "1 j" };
