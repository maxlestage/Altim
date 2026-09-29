/** Client for the Altim APIs (multi-source consensus computed on the server). */
import type { Candle } from "../engine/signal";
import type { Kind, QualityReport, Reliability } from "../engine/reliability";
import type { Interval, WatchItem } from "./store";
import { decisionUrl, parseDecision, type MarketRegime, type PersonalInput, type ScoreWeights } from "./decision";
import { calendarUrl, type CalendarReport } from "./calendar";
import { strategiesUrl, type StrategiesReport } from "./strategies";
import { botViewsUrl, type BotViews } from "./model-bot";

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

export type MacroInfo = import("../engine/macro").MacroReport & { evidence?: import("../engine/guard").Evidence | null; regime?: MarketRegime | null };
export type ZonesReport = {
  symbol: string; kind: Kind; price: number | null; asOf: number;
  zones: (import("../engine/fibonacci").FibZone & { macroNote: string | null })[];
  macro: MacroInfo | null;
};

export type SelectionCandidate = {
  rank: number; symbol: string; name: string; sector: string; marketCap: number; price: number;
  scores: Record<import("../engine/screener").Criterion, number>;
  why: Record<import("../engine/screener").Criterion, string>;
  action: string; zoneStatus: string;
  plan: { entry: number; limit: number | null; stop: number; target: number; atrPct: number } | null;
  track: { trades: number; winRate: number; avgReturn: number } | null;
  checks: { label: string; ok: boolean; detail: string }[];
};
export type SelectionReport = {
  market: "stock" | "crypto"; horizon: import("../engine/screener").Horizon; asOf: number; scanned: number;
  rankBy: import("../engine/screener").Criterion; rankRule: import("../engine/screener").RankRule;
  holdText: string; evidence: string; marketClosed: boolean;
  criteria: Record<import("../engine/screener").Criterion, string>;
  roles: Record<import("../engine/screener").Criterion, string>;
  buy: SelectionCandidate[];
  watch: (SelectionCandidate & { reason: string })[];
  setAside: { symbol: string; name: string; reason: string }[];
  validation: import("../engine/screener").Validation | null;
};

/** "Can I buy now?" (/api/alerts): the rule of the notifications of the apps. */
export type BuyAlert = {
  symbol: string; kind: Kind; name: string; price: number | null; buy?: boolean; strong?: boolean;
  reasons?: string[]; blockers?: string[]; cautions?: string[]; key?: string; title?: string; body?: string; error?: string;
};

/** "Point du jour" (/api/brief). */
export interface BriefReport {
  asOf: number;
  headline: string;
  market: { level: "calm" | "tense" | "high"; label: string; score: number; themes: string[] } | null;
  buyable: { symbol: string; kind: Kind; strong: boolean; title: string }[];
  movers: { symbol: string; kind: Kind; price: number; change: number }[];
  news: import("../engine/news").NewsItem[];
}

/** News section (/api/news). */
export type NewsReport = {
  asOf: number;
  items: import("../engine/news").NewsItem[];
  top: string[];
  digest: { total: number; themes: { theme: string; label: string; count: number }[]; tone: { negative: number; positive: number; neutral: number } };
  sources: { name: string; ok: boolean; count: number; error?: string }[];
  /** The day's important events (at most 5); absent from an older server. */
  summary?: import("./news-summary").StorySummary[];
};

export type UniverseItem = { symbol: string; name: string; kind: Kind; rank: number | null; etf?: boolean; exchanges?: number };

async function get<T>(url: string): Promise<T> {
  const r = await fetch(url);
  // Session expired (private access): back to the login page, then to the same screen.
  if (r.status === 401 && typeof window !== "undefined") {
    window.location.assign(`/login?next=${encodeURIComponent(window.location.pathname + window.location.search)}`);
    throw new Error("Session expirée");
  }
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
  history: (items: { symbol: string; kind: Kind }[], days: 30 | 90 | 365 | 730) =>
    get<{ asOf: number; days: number; series: { symbol: string; kind: Kind; closes: [number, number][]; error?: string }[] }>(
      `/api/history?days=${days}${items.length ? `&symbols=${list(items.slice(0, 20))}` : ""}`,
    ),
  brief: (items: { symbol: string; kind: Kind }[]) => get<BriefReport>(`/api/brief?symbols=${list(items.slice(0, 20))}`),
  news: (items: { symbol: string; kind: Kind }[]) => get<NewsReport>(`/api/news${items.length ? `?symbols=${list(items.slice(0, 20))}` : ""}`),
  alerts: (items: { symbol: string; kind: Kind }[]) => batched(items, (c) => get<BuyAlert[]>(`/api/alerts?symbols=${list(c)}`)),
  selection: (horizon: import("../engine/screener").Horizon, kind: Kind = "stock") => get<SelectionReport | { pending: true }>(`/api/selection?horizon=${horizon}&kind=${kind}`),
  /** Decision for one asset; `personal` (average cost, portfolio weights) only for a held asset, never stored by the server. */
  decision: (symbol: string, kind: Kind, personal?: PersonalInput | null, scoreWeights?: ScoreWeights | null) =>
    get<unknown>(decisionUrl(symbol, kind, personal, scoreWeights)).then(parseDecision),
  /** Agenda: economy, central banks, earnings, dividends, splits, IPOs; `symbols` limits the company events. */
  calendar: (days: number, symbols: string[] | null, top = false) => get<CalendarReport>(calendarUrl(days, symbols, top)),
  /** Strategy comparator of one asset (fixed textbook parameters, daily history). */
  strategies: (symbol: string, kind: Kind) => get<StrategiesReport>(strategiesUrl(symbol, kind)),
  sentiment: (symbol: string, kind: Kind) => get<Sentiment>(`/api/sentiment?symbol=${encodeURIComponent(symbol)}&kind=${kind}`),
  opportunities: (kind: Kind) => get<import("../engine/opportunities").OpportunityReport | { pending: true }>(`/api/opportunities?kind=${kind}`),
  /** Sector of each stock (Nasdaq screener, SEC SIC code, ETF flag); 50 at most. */
  sectors: (symbols: string[]) => get<import("../engine/sectors").SectorsReport>(`/api/sectors?symbols=${encodeURIComponent(symbols.slice(0, 50).join(","))}`),
  /** Signal validated on a fixed basket (heavy: 202 { pending } until the first computation is ready). */
  validation: () => get<import("./model-validation").ValidationReport | { pending: true }>("/api/validation"),
  /** « Bot Altim » trained and tested walk-forward on the same basket (heavy: 202 { pending } until ready). */
  bot: () => get<import("./model-bot").BotReport | { pending: true }>("/api/bot"),
  /** Today's view of the bot for these assets (cached report only). */
  botViews: (items: { symbol: string; kind: Kind }[]) => get<BotViews>(botViewsUrl(items)),
  anomalies: (symbol: string, kind: Kind) => get<import("../engine/opportunities").AnomalyReport>(`/api/anomalies?symbol=${encodeURIComponent(symbol)}&kind=${kind}`),
};

export const HIGHER: Record<Interval, Interval | null> = { "1h": "4h", "4h": "1d", "1d": null };
export const STEP_MS: Record<Interval, number> = { "1h": 3_600_000, "4h": 14_400_000, "1d": 86_400_000 };
export const INTERVAL_LABEL: Record<Interval, string> = { "1h": "1 h", "4h": "4 h", "1d": "1 j" };
