/**
 * Multi-source market data (server side: no CORS, same logic as the app).
 * Every source is queried in parallel, compared candle by candle with the median, and the first agreeing one is served.
 */
import type { Candle } from "../src/engine/signal";
import { assessQuality, reliability, type Kind, type QualityReport, type Reliability } from "../src/engine/reliability";
import { consensusQuotes, makeAsset, type QuoteSourceStatus } from "./quotes";

export type Interval = "1h" | "4h" | "1d";
export type SourceStatus = { name: string; ok: boolean; deviation?: number; error?: string };
export type Consensus = { candles: Candle[]; source: string; sources: SourceStatus[]; agreeing: number; conflict?: boolean };
export type Snapshot = Consensus & { symbol: string; kind: Kind; interval: Interval; quality: QualityReport; reliability: Reliability };

export const STEP: Record<Interval, number> = { "1h": 3_600_000, "4h": 14_400_000, "1d": 86_400_000 };
export const HIGHER: Record<Interval, Interval | null> = { "1h": "4h", "4h": "1d", "1d": null };
const num = (v: unknown) => Number(v);

async function getJSON(url: string): Promise<unknown> {
  const res = await fetch(url, { headers: { "User-Agent": "Mozilla/5.0 Altim/1.0", Accept: "application/json" }, signal: AbortSignal.timeout(8000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

/** Only closed candles, sorted chronologically. */
const closedOnly = (candles: Candle[], interval: Interval, now = Date.now()) =>
  candles.filter((c) => c.time + STEP[interval] <= now && Number.isFinite(c.close) && c.close > 0).sort((a, b) => a.time - b.time);

export function aggregate(candles: Candle[], from: number, to: number): Candle[] {
  const per = Math.round(to / from);
  const groups = new Map<number, Candle[]>();
  for (const c of [...candles].sort((a, b) => a.time - b.time)) {
    const key = Math.floor(c.time / to) * to;
    groups.set(key, [...(groups.get(key) ?? []), c]);
  }
  return [...groups.entries()]
    .filter(([, items]) => items.length === per)
    .map(([time, items]) => ({
      time,
      open: items[0]!.open,
      high: Math.max(...items.map((c) => c.high)),
      low: Math.min(...items.map((c) => c.low)),
      close: items[items.length - 1]!.close,
      volume: items.reduce((a, c) => a + c.volume, 0),
    }));
}

// Parsers (formats checked against real responses; see test/market.test.ts).
export const parse = {
  binance: (d: unknown): Candle[] =>
    (d as unknown[][]).map((r) => ({ time: num(r[0]), open: num(r[1]), high: num(r[2]), low: num(r[3]), close: num(r[4]), volume: num(r[5]) })),
  okx: (d: unknown): Candle[] => {
    const o = d as { code: string; data: string[][] };
    if (o.code !== "0") throw new Error("OKX");
    return o.data.map((r) => ({ time: num(r[0]), open: num(r[1]), high: num(r[2]), low: num(r[3]), close: num(r[4]), volume: num(r[5]) })).reverse();
  },
  coinbase: (d: unknown): Candle[] =>
    (d as number[][]).map((r) => ({ time: r[0]! * 1000, low: r[1]!, high: r[2]!, open: r[3]!, close: r[4]!, volume: r[5]! })).reverse(),
  kraken: (d: unknown): Candle[] => {
    const o = d as { error: string[]; result: Record<string, unknown> };
    if (o.error?.length) throw new Error(o.error.join());
    const rows = Object.entries(o.result).find(([k]) => k !== "last")?.[1] as unknown[][];
    return rows.map((r) => ({ time: num(r[0]) * 1000, open: num(r[1]), high: num(r[2]), low: num(r[3]), close: num(r[4]), volume: num(r[6]) }));
  },
  kucoin: (d: unknown): Candle[] => {
    const o = d as { code: string; data: string[][] };
    if (o.code !== "200000") throw new Error("KuCoin");
    return o.data.map((r) => ({ time: num(r[0]) * 1000, open: num(r[1]), close: num(r[2]), high: num(r[3]), low: num(r[4]), volume: num(r[5]) })).reverse();
  },
  gate: (d: unknown): Candle[] =>
    (d as string[][]).map((r) => ({ time: num(r[0]) * 1000, close: num(r[2]), high: num(r[3]), low: num(r[4]), open: num(r[5]), volume: num(r[6]) })),
  bitstamp: (d: unknown): Candle[] =>
    ((d as { data?: { ohlc?: Record<string, string>[] } }).data?.ohlc ?? []).map((r) => ({
      time: num(r.timestamp) * 1000, open: num(r.open), high: num(r.high), low: num(r.low), close: num(r.close), volume: num(r.volume),
    })),
  /** Gemini: [ms, open, high, low, close, volume], most recent first. */
  gemini: (d: unknown): Candle[] =>
    (d as number[][]).map((r) => ({ time: r[0]!, open: r[1]!, high: r[2]!, low: r[3]!, close: r[4]!, volume: r[5]! })).reverse(),
  /** Bitfinex: [ms, open, CLOSE, high, low, volume], most recent first. */
  bitfinex: (d: unknown): Candle[] =>
    (d as number[][]).map((r) => ({ time: r[0]!, open: r[1]!, close: r[2]!, high: r[3]!, low: r[4]!, volume: r[5]! })).reverse(),
  cryptocom: (d: unknown): Candle[] => {
    const o = d as { code: number; result?: { data?: Record<string, string | number>[] } };
    if (o.code !== 0) throw new Error("Crypto.com");
    return (o.result?.data ?? []).map((r) => ({ time: num(r.t), open: num(r.o), high: num(r.h), low: num(r.l), close: num(r.c), volume: num(r.v) }));
  },
  bitget: (d: unknown): Candle[] => {
    const o = d as { code: string; data: string[][] };
    if (o.code !== "00000") throw new Error("Bitget");
    return o.data.map((r) => ({ time: num(r[0]), open: num(r[1]), high: num(r[2]), low: num(r[3]), close: num(r[4]), volume: num(r[5]) }));
  },
  /** HTX: {id (s), open, close, low, high, amount}, most recent first. */
  htx: (d: unknown): Candle[] => {
    const o = d as { status: string; data: Record<string, number>[] };
    if (o.status !== "ok") throw new Error("HTX");
    return o.data.map((r) => ({ time: r.id! * 1000, open: r.open!, high: r.high!, low: r.low!, close: r.close!, volume: r.amount! })).reverse();
  },
};

type Source = { name: string; fetch: (base: string, interval: Interval) => Promise<Candle[]>; supports?: (i: Interval) => boolean };

export const SOURCES: Source[] = [
  { name: "Binance", fetch: async (b, i) => parse.binance(await getJSON(`https://api.binance.com/api/v3/klines?symbol=${b}USDT&interval=${i}&limit=500`)) },
  { name: "OKX", fetch: async (b, i) => parse.okx(await getJSON(`https://www.okx.com/api/v5/market/candles?instId=${b}-USDT&bar=${{ "1h": "1H", "4h": "4H", "1d": "1Dutc" }[i]}&limit=300`)) },
  {
    name: "Coinbase",
    fetch: async (b, i) => {
      const c = parse.coinbase(await getJSON(`https://api.exchange.coinbase.com/products/${b}-USD/candles?granularity=${i === "1d" ? 86400 : 3600}`));
      return i === "4h" ? aggregate(c, STEP["1h"], STEP["4h"]) : c;
    },
  },
  { name: "Kraken", fetch: async (b, i) => parse.kraken(await getJSON(`https://api.kraken.com/0/public/OHLC?pair=${b === "BTC" ? "XBT" : b}USD&interval=${STEP[i] / 60_000}`)) },
  {
    name: "KuCoin",
    fetch: async (b, i) => {
      const end = Math.floor(Date.now() / 1000);
      const type = { "1h": "1hour", "4h": "4hour", "1d": "1day" }[i];
      return parse.kucoin(await getJSON(`https://api.kucoin.com/api/v1/market/candles?type=${type}&symbol=${b}-USDT&startAt=${end - (STEP[i] / 1000) * 500}&endAt=${end}`));
    },
  },
  { name: "Gate.io", fetch: async (b, i) => parse.gate(await getJSON(`https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=${b}_USDT&interval=${i}&limit=500`)) },
  { name: "Bitstamp", fetch: async (b, i) => parse.bitstamp(await getJSON(`https://www.bitstamp.net/api/v2/ohlc/${b.toLowerCase()}usd/?step=${STEP[i] / 1000}&limit=500`)) },
  {
    name: "Gemini",
    fetch: async (b, i) => {
      // No 4 h candles at Gemini: rebuilt from 1 h.
      const c = parse.gemini(await getJSON(`https://api.gemini.com/v2/candles/${b.toLowerCase()}usd/${i === "1d" ? "1day" : "1hr"}`));
      return i === "4h" ? aggregate(c, STEP["1h"], STEP["4h"]) : c;
    },
  },
  {
    name: "Bitfinex",
    fetch: async (b, i) => {
      const pair = b.length > 3 ? `t${b}:USD` : `t${b}USD`;
      const c = parse.bitfinex(await getJSON(`https://api-pub.bitfinex.com/v2/candles/trade:${i === "1d" ? "1D" : "1h"}:${pair}/hist?limit=${i === "4h" ? 2000 : 500}`));
      return i === "4h" ? aggregate(c, STEP["1h"], STEP["4h"]) : c;
    },
  },
  { name: "Crypto.com", fetch: async (b, i) => parse.cryptocom(await getJSON(`https://api.crypto.com/exchange/v1/public/get-candlestick?instrument_name=${b}_USDT&timeframe=${{ "1h": "1h", "4h": "4h", "1d": "1D" }[i]}&count=300`)) },
  { name: "Bitget", fetch: async (b, i) => parse.bitget(await getJSON(`https://api.bitget.com/api/v2/spot/market/candles?symbol=${b}USDT&granularity=${{ "1h": "1h", "4h": "4h", "1d": "1Dutc" }[i]}&limit=500`)) },
  { name: "MEXC", fetch: async (b, i) => parse.binance(await getJSON(`https://api.mexc.com/api/v3/klines?symbol=${b}USDT&interval=${{ "1h": "60m", "4h": "4h", "1d": "1d" }[i]}&limit=500`)) },
  {
    // HTX daily candles start at 16:00 UTC (UTC+8): hourly timeframes only.
    name: "HTX",
    supports: (i) => i !== "1d",
    fetch: async (b, i) => parse.htx(await getJSON(`https://api.huobi.pro/market/history/kline?symbol=${b.toLowerCase()}usdt&period=${i === "1h" ? "60min" : "4hour"}&size=500`)),
  },
  { name: "Binance.US", fetch: async (b, i) => parse.binance(await getJSON(`https://api.binance.us/api/v3/klines?symbol=${b}USDT&interval=${i}&limit=500`)) },
];

const median = (v: number[]) => {
  const s = [...v].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2]! : (s[s.length / 2 - 1]! + s[s.length / 2]!) / 2;
};

/** Median deviation (%) of each series from the per-timestamp median, over 20 shared candles. */
export function deviations(series: Candle[][]): number[] {
  const maps = series.map((s) => new Map(s.map((c) => [c.time, c.close])));
  return maps.map((m, i) => {
    const shared = [...m.keys()].filter((t) => maps.some((o, j) => j !== i && o.has(t))).sort((a, b) => a - b).slice(-20);
    if (!shared.length) return Infinity;
    return median(shared.map((t) => { const ref = median(maps.filter((o) => o.has(t)).map((o) => o.get(t)!)); return (Math.abs(m.get(t)! / ref - 1) * 100); }));
  });
}

export async function consensus(
  base: string,
  interval: Interval,
  sources = SOURCES,
  target = Infinity,
  tolerance = 0.5,
  closed: (c: Candle[], i: Interval) => Candle[] = closedOnly,
): Promise<Consensus> {
  sources = sources.filter((s) => !s.supports || s.supports(interval));
  const results: { name: string; candles?: Candle[]; error?: string }[] = [];
  let cursor = 0;
  while (cursor < sources.length && results.filter((r) => r.candles).length < target) {
    const wave = sources.slice(cursor, cursor + target - results.filter((r) => r.candles).length);
    cursor += wave.length;
    results.push(
      ...(await Promise.all(
        wave.map(async (s) => {
          try {
            const candles = closed(await s.fetch(base, interval), interval);
            if (!candles.length) throw new Error("vide");
            return { name: s.name, candles };
          } catch (e) {
            return { name: s.name, error: e instanceof Error ? e.message : String(e) };
          }
        }),
      )),
    );
  }
  const ok = results.filter((r) => r.candles);
  if (!ok.length) throw new Error("Toutes les sources ont échoué");
  const devs = ok.length > 1 ? deviations(ok.map((r) => r.candles!)) : [0];
  const agreeing = ok.map((r, i) => ({ r, d: devs[i]! })).filter((x) => x.d <= tolerance);
  const primary = agreeing.find((x) => x.r.candles!.length >= 60) ?? agreeing[0] ?? { r: ok[0]!, d: devs[0]! };
  return {
    candles: primary.r.candles!,
    source: primary.r.name,
    agreeing: agreeing.length,
    // Fewer than half of the responding sources agree: impossible to know which ones are right.
    conflict: ok.length > 1 && agreeing.length * 2 < ok.length,
    sources: results.map((r) => {
      const i = ok.indexOf(r);
      return r.candles ? { name: r.name, ok: devs[i]! <= tolerance, deviation: devs[i] } : { name: r.name, ok: false, error: r.error };
    }),
  };
}

// ---------- Stocks: Yahoo Finance (all timeframes) + Nasdaq (daily) ----------

/** Opening of the New York session (9:30 local) for a date, US daylight saving time handled. */
export function nyOpen(year: number, month: number, day: number): number {
  const fmt = new Intl.DateTimeFormat("en-US", { timeZone: "America/New_York", hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
  for (const utcHour of [13, 14]) {
    const t = Date.UTC(year, month - 1, day, utcHour, 30);
    if (fmt.format(t) === "09:30") return t;
  }
  return Date.UTC(year, month - 1, day, 13, 30);
}

export const parseStock = {
  yahoo: (d: any): Candle[] => {
    const r = d?.chart?.result?.[0];
    if (!r) throw new Error(d?.chart?.error?.description ?? "Yahoo : aucune donnée");
    const q = r.indicators?.quote?.[0] ?? {};
    const ts: number[] = r.timestamp ?? [];
    return ts.flatMap((t, i) => {
      const [o, h, l, c] = [q.open?.[i], q.high?.[i], q.low?.[i], q.close?.[i]];
      if ([o, h, l, c].some((v) => v == null)) return [];
      return [{ time: t * 1000, open: o, high: h, low: l, close: c, volume: q.volume?.[i] ?? 0 }];
    });
  },
  nasdaq: (d: any): Candle[] => {
    const rows: any[] = d?.data?.tradesTable?.rows ?? [];
    const num = (v: string) => Number(String(v).replace(/[$,]/g, ""));
    return rows
      .map((r) => {
        const [m, day, y] = String(r.date).split("/").map(Number);
        return { time: nyOpen(y!, m!, day!), open: num(r.open), high: num(r.high), low: num(r.low), close: num(r.close), volume: num(r.volume) || 0 };
      })
      .reverse();
  },
  /** Robinhood daily history: regular session only, dated at midnight UTC. */
  robinhood: (d: any): Candle[] =>
    ((d?.historicals ?? []) as any[])
      .filter((r) => r.session === "reg" && !r.interpolated)
      .map((r) => {
        const [y, m, day] = String(r.begins_at).slice(0, 10).split("-").map(Number);
        return { time: nyOpen(y!, m!, day!), open: Number(r.open_price), high: Number(r.high_price), low: Number(r.low_price), close: Number(r.close_price), volume: Number(r.volume) || 0 };
      }),
  /** Cboe daily history (since 2004): the last 800 sessions are enough. */
  cboe: (d: any): Candle[] =>
    ((d?.data ?? []) as any[]).slice(-800).map((r) => {
      const [y, m, day] = String(r.date).split("-").map(Number);
      return { time: nyOpen(y!, m!, day!), open: Number(r.open), high: Number(r.high), low: Number(r.low), close: Number(r.close), volume: Number(r.volume) || 0 };
    }),
};

/** 1h → 4h for stocks: 4-bar blocks inside each session (New York day). */
export function aggregateSession(candles: Candle[], per = 4): Candle[] {
  const day = (t: number) => new Intl.DateTimeFormat("en-CA", { timeZone: "America/New_York" }).format(t);
  const out: Candle[] = [];
  let bucket: Candle[] = [];
  const flush = () => {
    if (!bucket.length) return;
    out.push({
      time: bucket[0]!.time, open: bucket[0]!.open, high: Math.max(...bucket.map((c) => c.high)),
      low: Math.min(...bucket.map((c) => c.low)), close: bucket[bucket.length - 1]!.close,
      volume: bucket.reduce((a, c) => a + c.volume, 0),
    });
    bucket = [];
  };
  for (const c of candles) {
    if (bucket.length && (bucket.length === per || day(bucket[0]!.time) !== day(c.time))) flush();
    bucket.push(c);
  }
  flush();
  return out;
}

const isoDate = (t: number) => new Date(t).toISOString().slice(0, 10);

export const STOCK_SOURCES: Source[] = [
  {
    name: "Yahoo Finance",
    fetch: async (symbol, i) => {
      const [interval, range] = i === "1d" ? ["1d", "2y"] : i === "4h" ? ["1h", "2y"] : ["1h", "6mo"];
      const c = parseStock.yahoo(await getJSON(`https://query1.finance.yahoo.com/v8/finance/chart/${encodeURIComponent(symbol)}?interval=${interval}&range=${range}&includePrePost=false`));
      return i === "4h" ? aggregateSession(c) : c;
    },
  },
  {
    name: "Nasdaq",
    supports: (i) => i === "1d",
    fetch: async (symbol) => {
      const from = isoDate(Date.now() - 800 * 86_400_000);
      for (const cls of ["stocks", "etf"]) {
        const c = parseStock.nasdaq(await getJSON(`https://api.nasdaq.com/api/quote/${encodeURIComponent(symbol.replace(/-/g, "."))}/historical?assetclass=${cls}&fromdate=${from}&todate=${isoDate(Date.now())}&limit=9999`));
        if (c.length) return c;
      }
      throw new Error("symbole inconnu");
    },
  },
  {
    name: "Robinhood",
    // Robinhood hourly bars start on the hour (Yahoo's at :30): daily only, to compare the same candles.
    supports: (i) => i === "1d",
    fetch: async (symbol) =>
      parseStock.robinhood(await getJSON(`https://api.robinhood.com/marketdata/historicals/${encodeURIComponent(symbol.replace(/-/g, "."))}/?interval=day&span=5year&bounds=regular`)),
  },
  {
    name: "Cboe",
    supports: (i) => i === "1d",
    fetch: async (symbol) => parseStock.cboe(await getJSON(`https://cdn.cboe.com/api/global/delayed_quotes/charts/historical/${encodeURIComponent(symbol.replace(/-/g, "."))}.json`)),
  },
];

/** Stock candle closed: the daily session lasts 6h30, not 24h. */
const stockClosed = (candles: Candle[], interval: Interval, now = Date.now()) => {
  const duration = interval === "1d" ? 6.5 * 3_600_000 : STEP[interval];
  return candles.filter((c) => c.time + duration <= now && Number.isFinite(c.close) && c.close > 0).sort((a, b) => a.time - b.time);
};

/**
 * Intraday stocks: only Yahoo publishes hourly candles aligned on the session, so its last close is
 * cross-checked with the live price of the other providers (Nasdaq, Cboe, Robinhood, TradingView).
 * Tolerance: 2 % on 1 h, 3 % on 4 h (the price moves between the last closed candle and now).
 */
export function crossCheck(last: number, quotes: QuoteSourceStatus[], known: string[], tolerance: number): SourceStatus[] {
  return quotes
    .filter((q) => !known.includes(q.name))
    .map((q) => {
      if (!q.price) return { name: `${q.name} (cours)`, ok: false, error: q.error ?? "non coté" };
      const deviation = Math.abs(q.price / last - 1) * 100;
      return { name: `${q.name} (cours)`, ok: deviation <= tolerance, deviation };
    });
}

/** Validated snapshot: multi-source candles + quality + reliability score. */
export async function snapshot(symbol: string, kind: Kind, interval: Interval): Promise<Snapshot> {
  const intraStock = kind === "stock" && interval !== "1d";
  // Live prices are fetched at the same time as the candles (not after).
  const [c, quotes] = await Promise.all([
    kind === "crypto"
      ? consensus(symbol, interval)
      : consensus(symbol, interval, STOCK_SOURCES, Infinity, 1, (candles, i) => stockClosed(candles, i)),
    intraStock ? consensusQuotes([makeAsset(symbol, "stock")]).catch(() => []) : Promise.resolve([]),
  ]);
  if (intraStock && c.candles.length) {
    const extra = crossCheck(c.candles[c.candles.length - 1]!.close, quotes[0]?.sources ?? [], c.sources.map((s) => s.name), interval === "1h" ? 2 : 3);
    c.sources = [...c.sources, ...extra];
  }
  const quality = assessQuality(c.candles, STEP[interval], kind);
  // Distinct providers that agree (a provider's candles and its live price count once).
  const independent = new Set(c.sources.filter((s) => s.ok).map((s) => s.name.replace(/ \(cours\)$/, ""))).size;
  return { ...c, agreeing: c.sources.filter((s) => s.ok).length, symbol, kind, interval, quality, reliability: reliability(quality.score, independent, !!c.conflict) };
}
