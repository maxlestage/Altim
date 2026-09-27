/**
 * Full asset universe: every crypto listed against USD/USDT on the exchanges Altim reads,
 * and every stock / ETF listed in the United States (official Nasdaq Trader directory).
 * Lists are fetched from the sources themselves and cached, so nothing is hard-coded.
 */
import type { Kind } from "../src/engine/reliability";
import { cached } from "./cache";

/** Compact entry: symbol, name, rank (market cap order, 0 = unknown), flag (crypto: number of exchanges; stock: 1 = ETF). */
export type UniverseEntry = [symbol: string, name: string, rank: number, flag: number];

const UA = { "User-Agent": "Mozilla/5.0 Altim/1.0", Accept: "application/json, text/plain" };
const DAY = 86_400_000;

async function getJSON(url: string): Promise<any> {
  const res = await fetch(url, { headers: UA, signal: AbortSignal.timeout(15_000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

async function getText(url: string): Promise<string> {
  const res = await fetch(url, { headers: UA, signal: AbortSignal.timeout(15_000) });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.text();
}

const CRYPTO_SYMBOL = /^[A-Z0-9]{1,12}$/;
/** Kraken legacy codes. */
const KRAKEN_ALIAS: Record<string, string> = { XBT: "BTC", XDG: "DOGE" };

// ---------- Parsers (formats checked against real responses; see test/universe.test.ts) ----------

export const parseUniverse = {
  okx: (d: any): string[] =>
    (d?.data ?? []).filter((x: any) => x.quoteCcy === "USDT" && x.state === "live").map((x: any) => String(x.baseCcy)),
  coinbase: (d: any): string[] =>
    (Array.isArray(d) ? d : [])
      .filter((x: any) => ["USD", "USDT"].includes(x.quote_currency) && x.status === "online" && !x.trading_disabled)
      .map((x: any) => String(x.base_currency)),
  kraken: (d: any): string[] =>
    Object.values(d?.result ?? {})
      .map((x: any) => String(x.wsname ?? ""))
      .filter((w) => w.endsWith("/USD") || w.endsWith("/USDT"))
      .map((w) => w.split("/")[0]!)
      .map((b) => KRAKEN_ALIAS[b] ?? b),
  kucoin: (d: any): string[] =>
    (d?.data ?? []).filter((x: any) => x.quoteCurrency === "USDT" && x.enableTrading).map((x: any) => String(x.baseCurrency)),
  gate: (d: any): string[] =>
    (Array.isArray(d) ? d : []).filter((x: any) => x.quote === "USDT" && x.trade_status === "tradable").map((x: any) => String(x.base)),
  /** Coinbase / KuCoin currency lists: full names, used when CoinGecko is unavailable. */
  coinbaseNames: (d: any): [string, string][] =>
    (Array.isArray(d) ? d : []).map((x: any) => [String(x.id).toUpperCase(), String(x.name ?? "")]),
  kucoinNames: (d: any): [string, string][] =>
    (d?.data ?? []).map((x: any) => [String(x.currency).toUpperCase(), String(x.fullName ?? "")]),
  /** CoinGecko markets page: names and market cap rank. */
  gecko: (d: any): { symbol: string; name: string; rank: number }[] =>
    (Array.isArray(d) ? d : [])
      .filter((x: any) => x.market_cap_rank)
      .map((x: any) => ({ symbol: String(x.symbol).toUpperCase(), name: String(x.name), rank: Number(x.market_cap_rank) })),
  /** nasdaqlisted.txt / otherlisted.txt (pipe separated, last line = file date). */
  nasdaqDirectory: (text: string): { symbol: string; name: string; etf: boolean }[] => {
    const lines = text.trim().split(/\r?\n/);
    const header = lines[0]!.split("|");
    const col = (name: string) => header.indexOf(name);
    const sym = col("Symbol") >= 0 ? col("Symbol") : col("ACT Symbol");
    const [nameCol, etfCol, testCol] = [col("Security Name"), col("ETF"), col("Test Issue")];
    const out: { symbol: string; name: string; etf: boolean }[] = [];
    for (const line of lines.slice(1)) {
      const f = line.split("|");
      if (f.length < header.length || f[testCol] === "Y") continue;
      const raw = f[sym]!.trim();
      const name = f[nameCol]!.trim();
      // Preferred shares ($), warrants, rights, units and notes are not stocks you "own" in a portfolio.
      if (!raw || raw.includes("$") || /\b(warrants?|rights?|units?|notes? due|subordinated|debentures?|preferred)\b/i.test(name)) continue;
      const symbol = raw.replace(/\./g, "-"); // Yahoo format: BRK.B → BRK-B
      if (!/^[A-Z][A-Z0-9\-]{0,9}$/.test(symbol)) continue;
      out.push({ symbol, name: cleanStockName(name), etf: f[etfCol] === "Y" });
    }
    return out;
  },
  /** Nasdaq screener: market capitalisation, used to rank the stocks. */
  screener: (d: any): Map<string, number> =>
    new Map(
      (d?.data?.rows ?? [])
        .map((r: any) => [String(r.symbol).trim().replace(/[./]/g, "-"), Number(String(r.marketCap ?? "").replace(/,/g, "")) || 0] as const)
        .filter(([, cap]: readonly [string, number]) => cap > 0),
    ),
};

/** "Apple Inc. - Common Stock" → "Apple Inc."; "Alphabet Inc. - Class A Common Stock" → "Alphabet Inc. Class A". */
export function cleanStockName(name: string): string {
  name = name.replace(/\s+/g, " ");
  const cls = name.match(/\bClass [A-Z]\b/)?.[0];
  let n = name
    .replace(/\s*-?\s*\b(Class [A-Z] )?(Common Stock|Ordinary Shares?|Common Shares?|American Depositary Shares?|Depositary Shares?|Shares of Beneficial Interest|Capital Stock)\b.*$/i, "")
    .replace(/[\s,-]+$/, "")
    .trim();
  if (cls && !n.includes(cls)) n += ` ${cls}`;
  return n || name.trim();
}

// ---------- Building ----------

/** Tokens that replicate a stock or an ETF are not cryptos: the stock itself is offered instead. */
const TOKENIZED_STOCK = /\b(xStocks?|tokeni[sz]ed)\b/i;
/** Leveraged tokens (BTC3L, ETH5S…) are derivatives, not coins you hold. */
const LEVERAGED = /^[A-Z0-9]{2,}[2-9][LS]$/;
const STABLE_OR_FIAT = new Set(["USDT", "USD", "USDC", "DAI", "FDUSD", "TUSD", "BUSD", "USDE", "PYUSD", "USDP", "EUR", "GBP", "EURC", "EURT"]);

export function buildCrypto(
  exchanges: string[][],
  gecko: { symbol: string; name: string; rank: number }[],
  names: [string, string][] = [],
): UniverseEntry[] {
  const count = new Map<string, number>();
  for (const list of exchanges) {
    for (const b of new Set(list.map((s) => s.toUpperCase()))) {
      if (CRYPTO_SYMBOL.test(b) && !STABLE_OR_FIAT.has(b) && !LEVERAGED.test(b)) {
        count.set(b, (count.get(b) ?? 0) + 1);
      }
    }
  }
  // Same ticker for several coins: the best ranked one wins (it is the one the exchanges list).
  const info = new Map<string, { name: string; rank: number }>();
  for (const g of gecko) if (!info.has(g.symbol)) info.set(g.symbol, { name: g.name, rank: g.rank });
  for (const [sym, name] of names) {
    const clean = name.trim();
    if (clean && clean.toUpperCase() !== sym && !info.has(sym)) info.set(sym, { name: clean, rank: 0 });
  }
  return [...count.entries()]
    .map(([s, n]): UniverseEntry => [s, info.get(s)?.name ?? s, info.get(s)?.rank ?? 0, n])
    // Tokenized stocks (NVDAX "xStock", SPYON "Ondo Tokenized"): the real stock is in the stock list.
    .filter((e) => !TOKENIZED_STOCK.test(e[1]))
    .sort(byRank);
}

/**
 * Largest US ETFs by assets under management (the Nasdaq screener gives no size for ETFs):
 * they are listed right after the ranked stocks instead of alphabetically.
 */
export const POPULAR_ETFS = [
  "VOO", "IVV", "SPY", "VTI", "QQQ", "VUG", "VEA", "IEFA", "VTV", "BND", "AGG", "IWF", "GLD", "IEMG", "VXUS", "VGT", "IJH", "VWO",
  "VIG", "IJR", "SPLG", "XLK", "IWM", "SCHD", "VO", "RSP", "ITOT", "IBIT", "BNDX", "VB", "EFA", "IWD", "SCHX", "VYM", "TLT", "XLF",
  "SMH", "IAU", "SCHG", "QUAL", "IVW", "MUB", "VCIT", "SCHF", "VT", "XLV", "VNQ", "DIA", "IWR", "XLE", "ARKK", "SOXX", "SLV", "FBTC",
];

export function buildStocks(directories: { symbol: string; name: string; etf: boolean }[][], caps: Map<string, number>): UniverseEntry[] {
  const seen = new Map<string, { name: string; etf: boolean }>();
  for (const dir of directories) for (const s of dir) if (!seen.has(s.symbol)) seen.set(s.symbol, s);
  const ranked = [...caps.entries()].filter(([s]) => seen.has(s)).sort((a, b) => b[1] - a[1]);
  const rank = new Map(ranked.map(([s], i) => [s, i + 1]));
  const popular = (s: string) => {
    const i = POPULAR_ETFS.indexOf(s);
    return i < 0 ? Infinity : i;
  };
  return [...seen.entries()]
    .map(([s, v]): UniverseEntry => [s, v.name, rank.get(s) ?? 0, v.etf ? 1 : 0])
    .sort((a, b) => {
      if (a[2] || b[2]) return byRank(a, b);
      return popular(a[0]) - popular(b[0]) || a[0].localeCompare(b[0]);
    });
}

/** Ranked first (by rank), then crypto listed on more exchanges, then alphabetical. */
function byRank(a: UniverseEntry, b: UniverseEntry): number {
  if (a[2] && b[2]) return a[2] - b[2];
  if (a[2] || b[2]) return a[2] ? -1 : 1;
  return b[3] - a[3] || a[0].localeCompare(b[0]);
}

const settled = async <T>(p: Promise<T>, fallback: T): Promise<T> => p.catch(() => fallback);

/** CoinGecko limits bursts: pages are fetched one after the other, with one retry. */
let lastGecko: { symbol: string; name: string; rank: number }[] = [];
async function geckoRanks(): Promise<{ symbol: string; name: string; rank: number }[]> {
  const out: { symbol: string; name: string; rank: number }[] = [];
  for (const page of [1, 2, 3, 4]) {
    const url = `https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=${page}`;
    let rows = await settled(getJSON(url).then(parseUniverse.gecko), null);
    if (!rows) {
      await new Promise((r) => setTimeout(r, 2500));
      rows = await settled(getJSON(url).then(parseUniverse.gecko), null);
    }
    if (!rows) break;
    out.push(...rows);
  }
  // A throttled refresh keeps the previous ranking rather than losing it.
  if (out.length >= lastGecko.length) lastGecko = out;
  return lastGecko;
}

export function cryptoUniverse(): Promise<UniverseEntry[]> {
  return cached("universe:crypto", DAY / 4, async () => {
    const [okx, coinbase, kraken, kucoin, gate, cbNames, kcNames, gecko] = await Promise.all([
      settled(getJSON("https://www.okx.com/api/v5/public/instruments?instType=SPOT").then(parseUniverse.okx), []),
      settled(getJSON("https://api.exchange.coinbase.com/products").then(parseUniverse.coinbase), []),
      settled(getJSON("https://api.kraken.com/0/public/AssetPairs").then(parseUniverse.kraken), []),
      settled(getJSON("https://api.kucoin.com/api/v2/symbols").then(parseUniverse.kucoin), []),
      settled(getJSON("https://api.gateio.ws/api/v4/spot/currency_pairs").then(parseUniverse.gate), []),
      settled(getJSON("https://api.exchange.coinbase.com/currencies").then(parseUniverse.coinbaseNames), []),
      settled(getJSON("https://api.kucoin.com/api/v3/currencies").then(parseUniverse.kucoinNames), []),
      geckoRanks(),
    ]);
    const list = buildCrypto([okx, coinbase, kraken, kucoin, gate], gecko, [...cbNames, ...kcNames]);
    if (list.length < 50) throw new Error("Liste des cryptos indisponible");
    return list;
  });
}

export function stockUniverse(): Promise<UniverseEntry[]> {
  return cached("universe:stock", DAY / 2, async () => {
    const [nasdaq, other, caps] = await Promise.all([
      settled(getText("https://www.nasdaqtrader.com/dynamic/SymDir/nasdaqlisted.txt").then(parseUniverse.nasdaqDirectory), []),
      settled(getText("https://www.nasdaqtrader.com/dynamic/SymDir/otherlisted.txt").then(parseUniverse.nasdaqDirectory), []),
      settled(getJSON("https://api.nasdaq.com/api/screener/stocks?tableonly=true&download=true").then(parseUniverse.screener), new Map<string, number>()),
    ]);
    const list = buildStocks([nasdaq, other], caps);
    if (list.length < 1000) throw new Error("Liste des actions indisponible");
    return list;
  });
}

export const universe = (kind: Kind) => (kind === "crypto" ? cryptoUniverse() : stockUniverse());

const norm = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toUpperCase();

/** Search in a universe: exact symbol, then symbol prefix, then name (word start, then anywhere); ranked assets first. */
export function searchUniverse(list: UniverseEntry[], query: string, limit = 50): UniverseEntry[] {
  const q = norm(query.trim());
  if (!q) return list.slice(0, limit);
  const scored: { e: UniverseEntry; s: number; i: number }[] = [];
  list.forEach((e, i) => {
    const sym = e[0];
    const name = norm(e[1]);
    const s = sym === q ? 0 : sym.startsWith(q) ? 1 : new RegExp(`\\b${q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`).test(name) ? 2 : name.includes(q) ? 3 : -1;
    if (s >= 0) scored.push({ e, s, i });
  });
  return scored.sort((a, b) => a.s - b.s || a.i - b.i).slice(0, limit).map((x) => x.e);
}

/** Cryptos and stocks searched together: exact symbol, then symbol prefix, then the largest (same order on iOS). */
export function searchAll(crypto: UniverseEntry[], stock: UniverseEntry[], query: string, limit = 20): { e: UniverseEntry; kind: Kind }[] {
  const hits = [
    ...searchUniverse(crypto, query, limit).map((e, i) => ({ e, i, kind: "crypto" as Kind })),
    ...searchUniverse(stock, query, limit).map((e, i) => ({ e, i, kind: "stock" as Kind })),
  ];
  const Q = norm(query.trim());
  const score = (e: UniverseEntry) => (e[0] === Q ? 0 : e[0].startsWith(Q) ? 1 : 2);
  return hits
    .sort((a, b) => score(a.e) - score(b.e) || (a.e[2] || 1e9) - (b.e[2] || 1e9) || a.i - b.i)
    .slice(0, limit)
    .map(({ e, kind }) => ({ e, kind }));
}
