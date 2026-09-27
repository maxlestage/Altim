/**
 * Multi-source market data for the live demo (server side: no CORS, same logic as the app).
 * Sources are queried in parallel, compared candle by candle, and the first agreeing one is served.
 */
import type { Candle } from "../src/engine/signal";

export type Interval = "1h" | "4h" | "1d";
export type SourceStatus = { name: string; ok: boolean; deviation?: number; error?: string };
export type Consensus = { candles: Candle[]; source: string; sources: SourceStatus[]; agreeing: number };

const STEP: Record<Interval, number> = { "1h": 3_600_000, "4h": 14_400_000, "1d": 86_400_000 };
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
};

type Source = { name: string; fetch: (base: string, interval: Interval) => Promise<Candle[]> };

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

export async function consensus(base: string, interval: Interval, sources = SOURCES, target = 3, tolerance = 0.5): Promise<Consensus> {
  const results: { name: string; candles?: Candle[]; error?: string }[] = [];
  let cursor = 0;
  while (cursor < sources.length && results.filter((r) => r.candles).length < target) {
    const wave = sources.slice(cursor, cursor + target - results.filter((r) => r.candles).length);
    cursor += wave.length;
    results.push(
      ...(await Promise.all(
        wave.map(async (s) => {
          try {
            const candles = closedOnly(await s.fetch(base, interval), interval);
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
    sources: results.map((r) => {
      const i = ok.indexOf(r);
      return r.candles ? { name: r.name, ok: devs[i]! <= tolerance, deviation: devs[i] } : { name: r.name, ok: false, error: r.error };
    }),
  };
}
