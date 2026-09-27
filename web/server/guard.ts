/**
 * Data for the market guard: crowd positioning (OKX perpetuals), sentiment (Fear & Greed, StockTwits),
 * news (Google News, per asset) and market-wide fear (VIX). Each input is optional: a missing one only
 * removes its factors, never blocks the guard.
 */
import type { Candle } from "../src/engine/signal";
import type { Kind } from "../src/engine/reliability";
import { guard, newsTone, type Evidence, type GuardResult, type NewsItem, type Positioning, type SentimentInput } from "../src/engine/guard";
import type { MacroReport } from "../src/engine/macro";
import { cached } from "./cache";

const UA = { "User-Agent": "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 Altim/1.0" };

async function getJSON(url: string): Promise<any> {
  const res = await fetch(url, { headers: { ...UA, Accept: "application/json" }, signal: AbortSignal.timeout(8000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

async function getText(url: string): Promise<string> {
  const res = await fetch(url, { headers: UA, signal: AbortSignal.timeout(8000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.text();
}

const optional = <T>(p: Promise<T>): Promise<T | null> => p.catch(() => null);

// ---------- Parsers (formats checked on real responses, see test/guard-data.test.ts) ----------

const decode = (s: string) =>
  s.replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, "$1")
    .replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&quot;/g, '"').replace(/&#39;|&apos;/g, "'")
    .trim();

export const parseGuard = {
  /** OKX current funding rate: {"code":"0","data":[{"fundingRate":"-0.0000101"}]}. */
  funding: (d: any): number | null => {
    const v = Number(d?.data?.[0]?.fundingRate);
    return d?.code === "0" && Number.isFinite(v) ? v : null;
  },
  /** OKX rubik series [[ts, value, …]], most recent first → values oldest first. */
  rubik: (d: any, column = 1): number[] =>
    d?.code === "0" ? ((d.data ?? []) as string[][]).map((r) => Number(r[column])).filter(Number.isFinite).reverse() : [],
  /** alternative.me Fear & Greed history, most recent first → oldest first. */
  fearGreed: (d: any): number[] => ((d?.data ?? []) as { value: string }[]).map((x) => Number(x.value)).filter(Number.isFinite).reverse(),
  stocktwits: (d: any): { bullish: number | null; sample: number } => {
    const tags = ((d?.messages ?? []) as any[]).map((m) => m?.entities?.sentiment?.basic).filter(Boolean);
    return { bullish: tags.length ? (tags.filter((t) => t === "Bullish").length / tags.length) * 100 : null, sample: tags.length };
  },
  /** RSS 2.0 (Google News and others): title, date and source of each item. */
  rss: (xml: string): NewsItem[] =>
    [...xml.matchAll(/<item>([\s\S]*?)<\/item>/g)].flatMap(([, item]) => {
      const title = decode(item!.match(/<title>([\s\S]*?)<\/title>/)?.[1] ?? "");
      const time = Date.parse(item!.match(/<pubDate>([\s\S]*?)<\/pubDate>/)?.[1] ?? "");
      const source = decode(item!.match(/<source[^>]*>([\s\S]*?)<\/source>/)?.[1] ?? "");
      return title && Number.isFinite(time) ? [{ title, time, source: source || undefined }] : [];
    }),
  /** Cboe VIX history: {"data": [{"date", "close": "17.24"}]} → closes, oldest first. */
  vix: (d: any): number[] => ((d?.data ?? []) as { close: string }[]).slice(-60).map((x) => Number(x.close)).filter(Number.isFinite),
};

// ---------- Fetchers ----------

export async function positioning(base: string): Promise<Positioning | null> {
  return cached(`pos:${base}`, 300_000, async () => {
    const [funding, ls, oi] = await Promise.all([
      optional(getJSON(`https://www.okx.com/api/v5/public/funding-rate?instId=${base}-USDT-SWAP`).then(parseGuard.funding)),
      optional(getJSON(`https://www.okx.com/api/v5/rubik/stat/contracts/long-short-account-ratio?ccy=${base}&period=1H`).then((d) => parseGuard.rubik(d))),
      optional(getJSON(`https://www.okx.com/api/v5/rubik/stat/contracts/open-interest-volume?ccy=${base}&period=1H`).then((d) => parseGuard.rubik(d))),
    ]);
    if (funding === null && !ls?.length && !oi?.length) return null;
    return { fundingRate: funding, longShortRatio: ls ?? [], openInterest: oi ?? [] };
  }).catch(() => null);
}

export async function sentimentInput(symbol: string, kind: Kind): Promise<SentimentInput> {
  const [fg, st] = await Promise.all([
    kind === "crypto"
      ? optional(cached("fng30", 1_800_000, async () => parseGuard.fearGreed(await getJSON("https://api.alternative.me/fng/?limit=30"))))
      : Promise.resolve(null),
    optional(cached(`st2:${kind}:${symbol}`, 300_000, async () =>
      parseGuard.stocktwits(await getJSON(`https://api.stocktwits.com/api/2/streams/symbol/${encodeURIComponent(kind === "crypto" ? `${symbol}.X` : symbol)}.json`)))),
  ]);
  return { fearGreed: fg ?? [], socialBullish: st?.bullish ?? null, socialSample: st?.sample ?? 0 };
}

export async function news(symbol: string, kind: Kind, name: string): Promise<NewsItem[]> {
  const q = kind === "crypto" ? `"${name}" crypto` : `${symbol.replace(/-/g, ".")} stock "${name.replace(/,? (Inc|Corp|Corporation|Ltd|plc)\.?$/i, "")}"`;
  return cached(`news:${kind}:${symbol}`, 600_000, async () =>
    parseGuard.rss(await getText(`https://news.google.com/rss/search?q=${encodeURIComponent(`${q} when:7d`)}&hl=en-US&gl=US&ceid=US:en`)),
  ).catch(() => []);
}

export async function vix(): Promise<number[]> {
  return cached("vix", 3_600_000, async () => parseGuard.vix(await getJSON("https://cdn.cboe.com/api/global/delayed_quotes/charts/historical/_VIX.json")))
    .catch(() => []);
}

export interface GuardReport extends GuardResult {
  symbol: string;
  kind: Kind;
  asOf: number;
  price: number | null;
  inputs: {
    fundingRate: number | null;
    longShortRatio: number | null;
    openInterestUsd: number | null;
    fearGreed: number | null;
    socialBullish: number | null;
    socialSample: number;
    news24h: number;
    newsTone: { negative: number; positive: number };
    headlines: { title: string; time: number; source?: string }[];
    vix: number | null;
  };
  /** Macro / geopolitical context and what its stress announced on this asset. */
  macro: (MacroReport & { evidence: Evidence | null }) | null;
}

/** Full guard for one asset; candles come from the multi-source consensus (injected, already cached). */
export async function guardReport(
  symbol: string,
  kind: Kind,
  name: string,
  candles: (interval: "1h" | "4h" | "1d") => Promise<Candle[]>,
  now = Date.now(),
  macroContext: () => Promise<{ report: MacroReport; evidence: Evidence | null } | null> = async () => null,
): Promise<GuardReport> {
  const [daily, h4, h1, pos, sent, items, v, mac] = await Promise.all([
    candles("1d"),
    candles("4h"),
    candles("1h").catch(() => [] as Candle[]),
    kind === "crypto" ? positioning(symbol) : Promise.resolve(null),
    sentimentInput(symbol, kind),
    news(symbol, kind, name),
    kind === "stock" ? vix() : Promise.resolve([] as number[]),
    macroContext().catch(() => null),
  ]);
  const result = guard({ kind, daily, h4, h1, positioning: pos, sentiment: sent, news: items, vix: v, macro: mac, now });
  const day = items.filter((n) => n.time >= now - 86_400_000 && n.time <= now);
  const last = <T>(a: T[] | undefined) => (a?.length ? a[a.length - 1]! : null);
  return {
    ...result,
    symbol,
    kind,
    asOf: now,
    price: last(h1)?.close ?? last(h4)?.close ?? last(daily)?.close ?? null,
    inputs: {
      fundingRate: pos?.fundingRate ?? null,
      longShortRatio: last(pos?.longShortRatio),
      openInterestUsd: last(pos?.openInterest),
      fearGreed: last(sent.fearGreed),
      socialBullish: sent.socialBullish ?? null,
      socialSample: sent.socialSample ?? 0,
      news24h: day.length,
      newsTone: newsTone(day),
      headlines: [...day].sort((a, b) => b.time - a.time).slice(0, 5),
      vix: last(v),
    },
    macro: mac ? { ...mac.report, evidence: mac.evidence } : null,
  };
}
