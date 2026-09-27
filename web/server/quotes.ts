/**
 * Price consensus for the site's market panel: each price is fetched from several independent
 * sources, compared with the median, and a divergent source is discarded.
 */
import { EXTRA_QUOTE_SOURCES } from "./stocks-extra";

export type Kind = "crypto" | "stock";
export type Asset = { symbol: string; name: string; kind: Kind; gecko?: string };
export type SourceQuote = { price: number; change?: number };
export type QuoteSourceStatus = { name: string; ok: boolean; price?: number; error?: string };
export type ConsensusQuote = {
  symbol: string;
  name: string;
  kind: Kind;
  price: number;
  change: number | null;
  agreeing: number;
  total: number;
  sources: QuoteSourceStatus[];
};

export const ASSETS: Asset[] = [
  { symbol: "BTC", name: "Bitcoin", kind: "crypto", gecko: "bitcoin" },
  { symbol: "ETH", name: "Ethereum", kind: "crypto", gecko: "ethereum" },
  { symbol: "SOL", name: "Solana", kind: "crypto", gecko: "solana" },
  { symbol: "BNB", name: "BNB", kind: "crypto", gecko: "binancecoin" },
  { symbol: "XRP", name: "XRP", kind: "crypto", gecko: "ripple" },
  { symbol: "ADA", name: "Cardano", kind: "crypto", gecko: "cardano" },
  { symbol: "AAPL", name: "Apple", kind: "stock" },
  { symbol: "NVDA", name: "NVIDIA", kind: "stock" },
];

/** CoinGecko identifiers for the common cryptos (other cryptos go through the exchanges). */
export const GECKO: Record<string, string> = {
  BTC: "bitcoin", ETH: "ethereum", SOL: "solana", BNB: "binancecoin", XRP: "ripple", ADA: "cardano", DOGE: "dogecoin",
  AVAX: "avalanche-2", DOT: "polkadot", LINK: "chainlink", LTC: "litecoin", TRX: "tron", TON: "the-open-network",
  SHIB: "shiba-inu", ATOM: "cosmos", UNI: "uniswap", NEAR: "near", APT: "aptos", ARB: "arbitrum", OP: "optimism",
  SUI: "sui", PEPE: "pepe", BCH: "bitcoin-cash", XLM: "stellar", ETC: "ethereum-classic", FIL: "filecoin",
};

export function makeAsset(symbol: string, kind: Kind, name?: string): Asset {
  const known = ASSETS.find((a) => a.symbol === symbol && a.kind === kind);
  return known ?? { symbol, kind, name: name ?? symbol, gecko: kind === "crypto" ? GECKO[symbol] : undefined };
}

const UA = { "User-Agent": "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 Altim/1.0", Accept: "application/json" };
async function getJSON(url: string): Promise<any> {
  const res = await fetch(url, { headers: UA, signal: AbortSignal.timeout(8000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}
const n = (v: unknown) => {
  const x = typeof v === "string" ? Number(v.replace(/[$,%+]/g, "")) : Number(v);
  return Number.isFinite(x) && x !== 0 ? x : undefined;
};
/** Runs one request per asset, ignoring individual failures (asset not listed on that source). */
async function each(assets: Asset[], f: (a: Asset) => Promise<SourceQuote | undefined>) {
  const out = new Map<string, SourceQuote>();
  await Promise.all(assets.map(async (a) => {
    try {
      const q = await f(a);
      if (q?.price) out.set(a.symbol, q);
    } catch {}
  }));
  return out;
}

type QuoteSource = { name: string; kind: Kind; fetch: (assets: Asset[]) => Promise<Map<string, SourceQuote>> };

// Parsers (formats checked against real responses, see test/quotes.test.ts).
export const parseQuotes = {
  binance: (rows: any[]) =>
    new Map(rows.map((r) => [String(r.symbol).replace(/USDT$/, ""), { price: n(r.lastPrice)!, change: n(r.priceChangePercent) }])),
  okx: (d: any, bases: string[]) => {
    if (d.code !== "0") throw new Error("OKX");
    const m = new Map<string, SourceQuote>();
    for (const r of d.data) {
      const base = String(r.instId).replace(/-USDT$/, "");
      if (r.instId.endsWith("-USDT") && bases.includes(base)) {
        m.set(base, { price: n(r.last)!, change: (n(r.last)! / n(r.open24h)! - 1) * 100 });
      }
    }
    return m;
  },
  kraken: (d: any, bases: string[]) => {
    if (d.error?.length) throw new Error(d.error.join());
    const m = new Map<string, SourceQuote>();
    for (const base of bases) {
      const alt = base === "BTC" ? "XBT" : base;
      const row = d.result[`${alt}USD`] ?? d.result[`X${alt}ZUSD`];
      if (row) m.set(base, { price: n(row.c[0])! }); // Kraken's "o" is the UTC-day open, not 24h: no change
    }
    return m;
  },
  bitfinex: (rows: any[]) => new Map(rows.map((r) => [String(r[0]).slice(1).replace(/:?USD$/, ""), { price: n(r[7])!, change: r[6] * 100 }])),
  coingecko: (d: any, assets: Asset[]) =>
    new Map(assets.filter((a) => d[a.gecko!]).map((a) => [a.symbol, { price: n(d[a.gecko!].usd)!, change: n(d[a.gecko!].usd_24h_change) }])),
  nasdaq: (d: any): SourceQuote => ({ price: n(d.data.primaryData.lastSalePrice)!, change: n(d.data.primaryData.percentageChange) }),
  cboe: (d: any): SourceQuote => ({ price: n(d.details.current_price)!, change: n(d.details.price_change_percent) }),
  /** Robinhood batch quotes (class shares written BRK.B); unknown symbols come back as null. */
  robinhood: (d: any): Map<string, SourceQuote> =>
    new Map(((d?.results ?? []) as any[]).filter(Boolean).map((r) => {
      const price = n(r.last_trade_price)!;
      const prev = n(r.adjusted_previous_close) ?? n(r.previous_close);
      return [String(r.symbol).replace(/\./g, "-"), { price, change: prev ? (price / prev - 1) * 100 : undefined }];
    })),
  /** TradingView scanner: one row per listing; the main US listing is kept. */
  tradingview: (d: any): Map<string, SourceQuote> => {
    const m = new Map<string, SourceQuote>();
    for (const r of (d?.data ?? []) as { s: string; d: [string, string, number, number] }[]) {
      const sym = String(r.d[0]).replace(/\./g, "-");
      if (!m.has(sym) && r.d[2] > 0) m.set(sym, { price: r.d[2], change: r.d[3] });
    }
    return m;
  },
  /** Webull real-time quote: regular-session price and change ratio. */
  webull: (d: any): SourceQuote => ({ price: n(d.close)!, change: d.changeRatio != null ? Number(d.changeRatio) * 100 : undefined }),
  /** Zacks quote feed: {"AAPL": {"last": "341.07", "percent_net_change": "1.53…"}}. */
  zacks: (d: any): Map<string, SourceQuote> =>
    new Map(Object.entries(d ?? {}).flatMap(([sym, x]: [string, any]) =>
      n(x?.last) ? [[sym.replace(/\./g, "-"), { price: n(x.last)!, change: n(x.percent_net_change) }] as [string, SourceQuote]] : [])),
  /** Webull search: the US listing whose symbol is exactly the one asked (class shares written "BRK B"). */
  webullTicker: (d: any, symbol: string): number | null => {
    const hit = ((d?.data ?? []) as any[]).find((x) => x.regionCode === "US" && String(x.disSymbol).replace(/[ .]/g, "-") === symbol);
    return hit ? Number(hit.tickerId) : null;
  },
  yahoo: (d: any): SourceQuote => {
    const r = d.chart.result[0];
    const closes: number[] = (r.indicators.quote[0].close as (number | null)[]).filter((x): x is number => x != null);
    const price = n(r.meta.regularMarketPrice)!;
    const prev = closes.length > 1 ? closes[closes.length - 2]! : undefined;
    return { price, change: prev ? (price / prev - 1) * 100 : undefined };
  },
};

const cryptos = (a: Asset[]) => a.filter((x) => x.kind === "crypto");

/** Webull identifies securities by a numeric id: looked up once per symbol, then kept. */
const webullIds = new Map<string, Promise<number | null>>();
export function webullTickerId(symbol: string): Promise<number | null> {
  if (!webullIds.has(symbol)) {
    const keyword = symbol.replace(/-/g, " ");
    const p = getJSON(`https://quotes-gw.webullfintech.com/api/search/pc/tickers?keyword=${encodeURIComponent(keyword)}&pageIndex=1&pageSize=10`)
      .then((d) => parseQuotes.webullTicker(d, symbol))
      .catch(() => {
        webullIds.delete(symbol); // network error: retried next time
        return null;
      });
    webullIds.set(symbol, p);
  }
  return webullIds.get(symbol)!;
}
const stocks = (a: Asset[]) => a.filter((x) => x.kind === "stock");
const dotted = (a: Asset[]) => stocks(a).map((x) => x.symbol.replace(/-/g, "."));

export const QUOTE_SOURCES: QuoteSource[] = [
  {
    name: "Binance", kind: "crypto",
    fetch: async (a) => parseQuotes.binance(await getJSON(`https://api.binance.com/api/v3/ticker/24hr?symbols=${encodeURIComponent(JSON.stringify(cryptos(a).map((x) => `${x.symbol}USDT`)))}`)),
  },
  { name: "OKX", kind: "crypto", fetch: async (a) => parseQuotes.okx(await getJSON("https://www.okx.com/api/v5/market/tickers?instType=SPOT"), cryptos(a).map((x) => x.symbol)) },
  {
    name: "Coinbase", kind: "crypto",
    fetch: (a) => each(cryptos(a), async (x) => {
      const d = await getJSON(`https://api.exchange.coinbase.com/products/${x.symbol}-USD/stats`);
      return { price: n(d.last)!, change: (n(d.last)! / n(d.open)! - 1) * 100 };
    }),
  },
  {
    name: "Kraken", kind: "crypto",
    fetch: async (a) => {
      const bases = cryptos(a).map((x) => x.symbol);
      const pairs = bases.map((b) => `${b === "BTC" ? "XBT" : b}USD`).join(",");
      return parseQuotes.kraken(await getJSON(`https://api.kraken.com/0/public/Ticker?pair=${pairs}`), bases);
    },
  },
  {
    name: "KuCoin", kind: "crypto",
    fetch: (a) => each(cryptos(a), async (x) => {
      const d = await getJSON(`https://api.kucoin.com/api/v1/market/stats?symbol=${x.symbol}-USDT`);
      return d.data?.last ? { price: n(d.data.last)!, change: Number(d.data.changeRate) * 100 } : undefined;
    }),
  },
  {
    name: "Gate.io", kind: "crypto",
    fetch: (a) => each(cryptos(a), async (x) => {
      const [r] = await getJSON(`https://api.gateio.ws/api/v4/spot/tickers?currency_pair=${x.symbol}_USDT`);
      return { price: n(r.last)!, change: Number(r.change_percentage) };
    }),
  },
  {
    name: "Bitfinex", kind: "crypto",
    fetch: async (a) => parseQuotes.bitfinex(await getJSON(`https://api-pub.bitfinex.com/v2/tickers?symbols=${cryptos(a).map((x) => (x.symbol.length > 3 ? `t${x.symbol}:USD` : `t${x.symbol}USD`)).join(",")}`)),
  },
  {
    name: "CoinGecko", kind: "crypto",
    fetch: async (a) => {
      const known = cryptos(a).filter((x) => x.gecko);
      if (!known.length) return new Map();
      const ids = known.map((x) => x.gecko).join(",");
      return parseQuotes.coingecko(await getJSON(`https://api.coingecko.com/api/v3/simple/price?ids=${ids}&vs_currencies=usd&include_24hr_change=true`), known);
    },
  },
  {
    name: "Yahoo Finance", kind: "stock",
    fetch: (a) => each(a.filter((x) => x.kind === "stock"), async (x) => parseQuotes.yahoo(await getJSON(`https://query1.finance.yahoo.com/v8/finance/chart/${x.symbol}?interval=1d&range=5d`))),
  },
  {
    name: "Nasdaq", kind: "stock",
    fetch: (a) => each(a.filter((x) => x.kind === "stock"), async (x) => {
      // Class shares: BRK-B (Yahoo) = BRK.B (Nasdaq). ETFs live in another asset class.
      const sym = x.symbol.replace(/-/g, ".");
      try {
        return parseQuotes.nasdaq(await getJSON(`https://api.nasdaq.com/api/quote/${sym}/info?assetclass=stocks`));
      } catch {
        return parseQuotes.nasdaq(await getJSON(`https://api.nasdaq.com/api/quote/${sym}/info?assetclass=etf`));
      }
    }),
  },
  {
    name: "Robinhood", kind: "stock",
    fetch: async (a) => {
      if (!stocks(a).length) return new Map();
      return parseQuotes.robinhood(await getJSON(`https://api.robinhood.com/quotes/?symbols=${dotted(a).map(encodeURIComponent).join(",")}`));
    },
  },
  {
    name: "Webull", kind: "stock",
    fetch: (a) => each(stocks(a), async (x) => {
      const id = await webullTickerId(x.symbol);
      if (!id) throw new Error("non coté");
      return parseQuotes.webull(await getJSON(`https://quotes-gw.webullfintech.com/api/stock/tickerRealTime/getQuote?tickerId=${id}&includeSecu=1`));
    }),
  },
  {
    name: "Zacks", kind: "stock",
    fetch: async (a) => {
      if (!stocks(a).length) return new Map();
      return parseQuotes.zacks(await getJSON(`https://quote-feed.zacks.com/index?t=${dotted(a).map(encodeURIComponent).join(",")}`));
    },
  },
  {
    name: "TradingView", kind: "stock",
    fetch: async (a) => {
      if (!stocks(a).length) return new Map();
      const res = await fetch("https://scanner.tradingview.com/america/scan", {
        method: "POST",
        headers: { ...UA, "Content-Type": "application/json" },
        body: JSON.stringify({
          filter: [
            { left: "name", operation: "in_range", right: dotted(a) },
            { left: "exchange", operation: "in_range", right: ["NASDAQ", "NYSE", "AMEX", "CBOE"] },
          ],
          columns: ["name", "exchange", "close", "change"],
          range: [0, 100],
        }),
        signal: AbortSignal.timeout(8000),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return parseQuotes.tradingview(await res.json());
    },
  },
  {
    name: "Cboe", kind: "stock",
    fetch: (a) => each(a.filter((x) => x.kind === "stock"), async (x) => parseQuotes.cboe(await getJSON(`https://www.cboe.com/education/tools/trade-optimizer/symbol-info/?symbol=${x.symbol.replace(/-/g, ".")}`))),
  },
];

// Live quotes from Fidelity, StockCharts, TipRanks and Public.com (stocks-extra.ts).
QUOTE_SOURCES.push(...EXTRA_QUOTE_SOURCES.map((s): QuoteSource => ({
  name: s.name, kind: "stock",
  fetch: async (a) => (await s.fetch(stocks(a).map((x) => x.symbol))) as Map<string, SourceQuote>,
})));

const median = (v: number[]) => {
  const s = [...v].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2]! : (s[s.length / 2 - 1]! + s[s.length / 2]!) / 2;
};

/** Consensus for each asset: median of agreeing sources (0.5 % crypto, 1.5 % stocks). */
export function combine(assets: Asset[], results: { name: string; kind: Kind; quotes?: Map<string, SourceQuote>; error?: string }[]): ConsensusQuote[] {
  return assets.flatMap((asset) => {
    const relevant = results.filter((r) => r.kind === asset.kind);
    const found = relevant.flatMap((r) => (r.quotes?.get(asset.symbol) ? [{ name: r.name, q: r.quotes.get(asset.symbol)! }] : []));
    if (!found.length) return [];
    const ref = median(found.map((f) => f.q.price));
    const tol = asset.kind === "crypto" ? 0.5 : 1.5;
    const agree = found.filter((f) => Math.abs(f.q.price / ref - 1) * 100 <= tol);
    const changes = agree.map((f) => f.q.change).filter((c): c is number => c !== undefined && Number.isFinite(c));
    return [{
      symbol: asset.symbol,
      name: asset.name,
      kind: asset.kind,
      price: median(agree.map((f) => f.q.price)),
      change: changes.length ? median(changes) : null,
      agreeing: agree.length,
      total: relevant.length,
      sources: relevant.map((r) => {
        const q = r.quotes?.get(asset.symbol);
        return q ? { name: r.name, ok: agree.some((f) => f.name === r.name), price: q.price } : { name: r.name, ok: false, error: r.error ?? "non coté" };
      }),
    }];
  });
}

export async function consensusQuotes(assets = ASSETS, sources = QUOTE_SOURCES): Promise<ConsensusQuote[]> {
  const results = await Promise.all(sources.map(async (s) => {
    try {
      return { name: s.name, kind: s.kind, quotes: await s.fetch(assets) };
    } catch (e) {
      return { name: s.name, kind: s.kind, error: e instanceof Error ? e.message : String(e) };
    }
  }));
  return combine(assets, results);
}
