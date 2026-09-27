/**
 * Additional stock and ETF providers (no key): daily candles from Dow Jones (WSJ / MarketWatch), AlphaQuery, Finviz,
 * the Financial Times and eToro; live quotes from Fidelity, StockCharts, TipRanks and Public.com.
 * Formats checked on real responses (September 2026), see test/stocks-extra.test.ts.
 */
import type { Candle } from "../src/engine/signal";

const UA = {
  "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
  Accept: "application/json, text/html, */*",
  "Accept-Language": "en-US,en;q=0.9",
};

async function fetchText(url: string, init: RequestInit = {}): Promise<string> {
  const res = await fetch(url, { ...init, headers: { ...UA, ...(init.headers ?? {}) }, signal: AbortSignal.timeout(10_000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.text();
}
const fetchJSON = async (url: string, init: RequestInit = {}): Promise<any> => JSON.parse(await fetchText(url, init));

// Formatters are costly to build: created once (thousands of candles per history).
const NY_TIME = new Intl.DateTimeFormat("en-US", { timeZone: "America/New_York", hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
const NY_DATE = new Intl.DateTimeFormat("en-CA", { timeZone: "America/New_York" });
const openCache = new Map<string, number>();

/** 9:30 New York of a calendar date (same convention as the other stock sources). */
function open(y: number, m: number, d: number): number {
  const key = `${y}-${m}-${d}`;
  const hit = openCache.get(key);
  if (hit !== undefined) return hit;
  const fmt = NY_TIME;
  for (const h of [13, 14]) {
    const t = Date.UTC(y, m - 1, d, h, 30);
    if (fmt.format(t) === "09:30") {
      openCache.set(key, t);
      return t;
    }
  }
  return Date.UTC(y, m - 1, d, 13, 30);
}
/** "2026-09-25…" → 9:30 New York that day. */
const fromIso = (s: string) => {
  const [y, m, d] = s.slice(0, 10).split("-").map(Number);
  return open(y!, m!, d!);
};
/** Timestamp (ms) → its calendar date in New York → 9:30 that day. */
const fromNy = (ms: number) => fromIso(NY_DATE.format(ms));
const n = (v: unknown) => {
  const x = typeof v === "string" ? Number(v.replace(/[$,]/g, "")) : Number(v);
  return Number.isFinite(x) ? x : null;
};
const valid = (c: Candle) => c.close > 0 && c.open > 0 && c.high >= c.low;
const dotted = (s: string) => s.replace(/-/g, ".");
/** Class shares written BRK/B (Fidelity, StockCharts). */
const slashed = (s: string) => s.replace(/-/g, "/");

export type Quote = { price: number; change?: number };

export const parseExtra = {
  /** WSJ / MarketWatch timeseries: ticks (ms, midnight UTC of the session) + [open, high, low, last] per tick. */
  wsj: (d: any): Candle[] => {
    const ticks: number[] = d?.TimeInfo?.Ticks ?? [];
    const px: (number | null)[][] = d?.Series?.find((s: any) => s.SeriesId === "s1")?.DataPoints ?? [];
    return ticks.flatMap((t, i) => {
      const p = px[i];
      if (!p || p.some((v) => v == null)) return [];
      const c = { time: fromIso(new Date(t).toISOString()), open: p[0]!, high: p[1]!, low: p[2]!, close: p[3]!, volume: 0 };
      return valid(c) ? [c] : [];
    });
  },
  /** AlphaQuery: {"unadjusted": [{"x": "2026-09-25T00:00:00Z", open, high, low, close, volume}]} (unadjusted, like the others). */
  alphaquery: (d: any): Candle[] =>
    ((d?.unadjusted ?? []) as any[]).map((r) => ({ time: fromIso(String(r.x)), open: Number(r.open), high: Number(r.high), low: Number(r.low), close: Number(r.close), volume: Number(r.volume) || 0 })).filter(valid),
  /** Finviz chart API: parallel arrays; `date` in seconds during the New York session. */
  finviz: (d: any): Candle[] => {
    const dates: number[] = d?.date ?? [];
    return dates.map((t, i) => ({ time: fromNy(t * 1000), open: Number(d.open[i]), high: Number(d.high[i]), low: Number(d.low[i]), close: Number(d.close[i]), volume: Number(d.volume?.[i]) || 0 })).filter(valid);
  },
  /** Financial Times chart API: Dates + Open/High/Low/Close series. */
  ft: (d: any): Candle[] => {
    const dates: string[] = d?.Dates ?? [];
    const series = Object.fromEntries(((d?.Elements?.[0]?.ComponentSeries ?? []) as any[]).map((c) => [c.Type, c.Values as number[]]));
    return dates.map((s, i) => ({ time: fromIso(s), open: series.Open?.[i]!, high: series.High?.[i]!, low: series.Low?.[i]!, close: series.Close?.[i]!, volume: 0 })).filter((c) => c.close != null && valid(c));
  },
  /** FT search: the primary US listing of this exact symbol (NASDAQ, NYSE, NYSE Arca, Cboe BZX, NYSE American). */
  ftXid: (d: any, symbol: string): string | null => {
    const us = /^(NSQ|NYQ|PCQ|BTQ|ASQ|NMQ)$/;
    const list = ((d?.data?.security ?? []) as any[]).filter((s) => {
      const [sym, ex] = String(s.symbol).split(":");
      return sym === dotted(symbol) && us.test(ex ?? "");
    });
    return (list.find((s) => s.isPrimary) ?? list[0])?.xid ?? null;
  },
  /** eToro daily candles (eToro's own prices, sessions dated at midnight UTC). */
  etoro: (d: any): Candle[] =>
    ((d?.Candles?.[0]?.Candles ?? []) as any[]).map((r) => ({ time: fromIso(String(r.FromDate)), open: Number(r.Open), high: Number(r.High), low: Number(r.Low), close: Number(r.Close), volume: Number(r.Volume) || 0 })).filter(valid),
  /** eToro instruments: stocks (type 5) and ETFs (type 6) by symbol (class shares written BRK.B). */
  etoroIds: (d: any): Map<string, number> =>
    new Map(((d?.InstrumentDisplayDatas ?? []) as any[]).filter((x) => x.InstrumentTypeID === 5 || x.InstrumentTypeID === 6).map((x) => [String(x.SymbolFull), Number(x.InstrumentID)])),
  /** Fidelity (JSONP-like, wrapped in parentheses): LAST_PRICE and PCT_CHG_TODAY per symbol. */
  fidelity: (text: string): Map<string, Quote> => {
    const d = JSON.parse(text.trim().replace(/^\(/, "").replace(/\)\s*;?$/, ""));
    return new Map(Object.entries(d?.QUOTES ?? {}).flatMap(([sym, q]: [string, any]) => {
      const price = n(q?.LAST_PRICE);
      return q?.ERROR_CODE === "0" && price ? [[sym.replace(/[./]/g, "-"), { price, change: n(q.PCT_CHG_TODAY) ?? undefined }] as [string, Quote]] : [];
    }));
  },
  /** StockCharts summary: last close and previous close. */
  stockcharts: (d: any): Quote => {
    const price = n(d?.close), prev = n(d?.lastClose);
    if (!price) throw new Error("StockCharts : symbole inconnu");
    return { price, change: prev ? (price / prev - 1) * 100 : undefined };
  },
  /** TipRanks: daily closes, the last one is the current price. */
  tipranks: (d: any): Quote => {
    const p = ((d?.prices ?? []) as any[]).map((x) => n(x.p)).filter((x): x is number => !!x);
    if (!p.length) throw new Error("TipRanks : aucun cours");
    const price = p[p.length - 1]!, prev = p[p.length - 2];
    return { price, change: prev ? (price / prev - 1) * 100 : undefined };
  },
  /** Public.com stock page: the quote is embedded in the page data ("last" and "previousClose"). */
  publicCom: (html: string, symbol: string): Quote => {
    const i = html.search(new RegExp(`"symbol"\\s*:\\s*"${dotted(symbol).replace(".", "\\.")}"`, "i"));
    const scope = i >= 0 ? html.slice(i, i + 4000) : html;
    const price = n(scope.match(/"last"\s*:\s*"?([\d.]+)/)?.[1]);
    const prev = n(scope.match(/"previousClose"\s*:\s*"?([\d.]+)/)?.[1]);
    if (!price) throw new Error("Public.com : cours introuvable");
    return { price, change: prev ? (price / prev - 1) * 100 : undefined };
  },
};

// ---------- Identifier lookups (kept once found) ----------

const memo = new Map<string, Promise<unknown>>();
function once<T>(key: string, fn: () => Promise<T>): Promise<T> {
  if (!memo.has(key)) memo.set(key, fn().catch((e) => { memo.delete(key); throw e; }));
  return memo.get(key) as Promise<T>;
}

/**
 * Entitlement token of the MarketWatch charts. It is the one every visitor's browser sends, but it is kept out of the
 * code: set WSJ_TOKEN in the environment (Heroku config var). Without it, the source is simply skipped.
 */
const wsjToken = () => process.env.WSJ_TOKEN ?? "";
function wsjUrl(key: string, timeFrame: string): string {
  const WSJ_TOKEN = wsjToken();
  const q = {
    Step: "P1D", TimeFrame: timeFrame, EntitlementToken: WSJ_TOKEN, IncludeMockTick: false, FilterNullSlots: true, FilterClosedPoints: true,
    IncludeClosedSlots: false, IncludeOfficialClose: true, InjectOpen: false, ShowPreMarket: false, ShowAfterHours: false, UseExtendedTimeFrame: true,
    WantPriorClose: false, IncludeCurrentQuotes: false, ResetTodaysAfterHoursPercentChange: false,
    // No volume series: the public entitlement refuses it ("Series kind 'Volume' is not known").
    Series: [{ Key: key, Dialect: "Charting", Kind: "Ticker", SeriesId: "s1", DataTypes: ["Open", "High", "Low", "Last"] }],
  };
  return `https://api.wsj.net/api/michelangelo/timeseries/history?json=${encodeURIComponent(JSON.stringify(q))}&ckey=${WSJ_TOKEN.slice(0, 10)}`;
}
const wsjHeaders = () => ({ "Dylan2010.EntitlementToken": wsjToken() });

/** Dow Jones keys depend on the exchange: tried in turn, the first one that answers is kept. */
function wsjKey(symbol: string): Promise<string> {
  return once(`wsj:${symbol}`, async () => {
    const s = dotted(symbol);
    for (const key of [`STOCK/US/XNAS/${s}`, `STOCK/US/XNYS/${s}`, `FUND/US/ARCX/${s}`, `FUND/US/XNAS/${s}`, `STOCK/US/XASE/${s}`, `FUND/US/BATS/${s}`, `STOCK/US/ARCX/${s}`]) {
      try {
        if (parseExtra.wsj(await fetchJSON(wsjUrl(key, "P5D"), { headers: wsjHeaders() })).length) return key;
      } catch {}
    }
    throw new Error("non coté");
  });
}

const ftXid = (symbol: string) =>
  once(`ft:${symbol}`, async () => {
    const xid = parseExtra.ftXid(await fetchJSON(`https://markets.ft.com/data/searchapi/searchsecurities?query=${encodeURIComponent(dotted(symbol))}`), symbol);
    if (!xid) throw new Error("non coté");
    return xid;
  });

/** eToro's instrument list (≈ 12 MB): loaded once a day. */
let etoroList: { at: number; ids: Promise<Map<string, number>> } | null = null;
function etoroId(symbol: string): Promise<number | null> {
  if (!etoroList || Date.now() - etoroList.at > 86_400_000) {
    const ids = fetchJSON("https://api.etorostatic.com/sapi/instrumentsmetadata/V1.1/instruments").then(parseExtra.etoroIds);
    etoroList = { at: Date.now(), ids };
    ids.catch(() => (etoroList = null));
  }
  return etoroList.ids.then((m) => m.get(dotted(symbol)) ?? null);
}

// ---------- Sources ----------

type CandleSource = { name: string; supports: (i: string) => boolean; fetch: (symbol: string) => Promise<Candle[]> };
const daily = (i: string) => i === "1d";

export const EXTRA_CANDLE_SOURCES: CandleSource[] = [
  {
    name: "WSJ / MarketWatch", supports: (i) => daily(i) && !!wsjToken(),
    fetch: async (s) => parseExtra.wsj(await fetchJSON(wsjUrl(await wsjKey(s), "P5Y"), { headers: wsjHeaders() })),
  },
  {
    name: "AlphaQuery", supports: daily,
    fetch: async (s) => parseExtra.alphaquery(await fetchJSON(`https://www.alphaquery.com/data/stock-price-chart?ticker=${encodeURIComponent(s.replace(/-/g, "."))}`)),
  },
  {
    name: "Finviz", supports: daily,
    fetch: async (s) => parseExtra.finviz(await fetchJSON(`https://finviz.com/api/quote.ashx?instrument=stock&ticker=${encodeURIComponent(s)}&timeframe=d`)),
  },
  {
    name: "Financial Times", supports: daily,
    fetch: async (s) => {
      const body = { days: 1100, dataNormalized: false, dataPeriod: "Day", dataInterval: 1, realtime: false, yFormat: "0.###", timeServiceFormat: "JSON", returnDateType: "ISO8601", elements: [{ Type: "price", Symbol: await ftXid(s), OverlayIndicators: [], Params: {} }] };
      return parseExtra.ft(await fetchJSON("https://markets.ft.com/data/chartapi/series", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) }));
    },
  },
  {
    name: "eToro", supports: daily,
    fetch: async (s) => {
      const id = await etoroId(s);
      if (!id) throw new Error("non coté");
      return parseExtra.etoro(await fetchJSON(`https://candle.etoro.com/candles/asc.json/OneDay/1000/${id}`));
    },
  },
];

type QuoteFetch = { name: string; fetch: (symbols: string[]) => Promise<Map<string, Quote>> };
async function each(symbols: string[], one: (s: string) => Promise<Quote>): Promise<Map<string, Quote>> {
  const out = new Map<string, Quote>();
  await Promise.all(symbols.map(async (s) => {
    try {
      out.set(s, await one(s));
    } catch {}
  }));
  return out;
}

export const EXTRA_QUOTE_SOURCES: QuoteFetch[] = [
  {
    name: "Fidelity",
    fetch: async (symbols) => symbols.length
      ? parseExtra.fidelity(await fetchText(`https://fastquote.fidelity.com/service/quote/json?productid=embeddedquotes&symbols=${symbols.map((s) => encodeURIComponent(slashed(s))).join(",")}`))
      : new Map(),
  },
  { name: "StockCharts", fetch: (symbols) => each(symbols, async (s) => parseExtra.stockcharts(await fetchJSON(`https://stockcharts.com/j-sum/sum?cmd=symsum&symbol=${encodeURIComponent(slashed(s))}`))) },
  { name: "TipRanks", fetch: (symbols) => each(symbols, async (s) => parseExtra.tipranks(await fetchJSON(`https://www.tipranks.com/api/stocks/getData/?name=${encodeURIComponent(dotted(s))}`))) },
  { name: "Public.com", fetch: (symbols) => each(symbols, async (s) => parseExtra.publicCom(await fetchText(`https://public.com/stocks/${encodeURIComponent(dotted(s).toLowerCase())}`), s)) },
];
