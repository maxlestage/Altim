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

async function get<T>(url: string): Promise<T> {
  const r = await fetch(url);
  const body = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error((body as { error?: string }).error ?? `Erreur ${r.status}`);
  return body as T;
}

const list = (items: { symbol: string; kind: Kind }[]) => encodeURIComponent(items.map((i) => `${i.symbol}:${i.kind}`).join(","));

export const api = {
  radar: (items: WatchItem[], interval: Interval) => get<RadarRow[]>(`/api/radar?symbols=${list(items)}&interval=${interval}`),
  candles: (symbol: string, kind: Kind, interval: Interval) => get<Snapshot>(`/api/candles?symbol=${encodeURIComponent(symbol)}&kind=${kind}&interval=${interval}`),
  quotes: (items: { symbol: string; kind: Kind }[]) => get<Quote[]>(`/api/tickers?symbols=${list(items)}`),
  search: (q: string) => get<{ symbol: string; name: string; kind: Kind }[]>(`/api/search?q=${encodeURIComponent(q)}`),
  sentiment: (symbol: string, kind: Kind) => get<Sentiment>(`/api/sentiment?symbol=${encodeURIComponent(symbol)}&kind=${kind}`),
};

export const HIGHER: Record<Interval, Interval | null> = { "1h": "4h", "4h": "1d", "1d": null };
export const STEP_MS: Record<Interval, number> = { "1h": 3_600_000, "4h": 14_400_000, "1d": 86_400_000 };
export const INTERVAL_LABEL: Record<Interval, string> = { "1h": "1 h", "4h": "4 h", "1d": "1 j" };
