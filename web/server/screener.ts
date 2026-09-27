/**
 * Stock selection over the largest US companies (Nasdaq screener: market cap, sector).
 * Stage 1: every stock is scored on its daily candles (one source, fast) and ranked by relative strength.
 * Stage 2: the best candidates are checked with the full multi-source consensus, the market guard and their trend;
 * a candidate that fails a check is set aside or put on watch, with the reason.
 */
import type { Candle } from "../src/engine/signal";
import { backtest, trackRecord } from "../src/engine/backtest";
import { alignSeries, explain, factorsAt, isPegged, pick, roles, scoreUniverse, span, validate, CRITERIA, RANKED_CRITERION, SPECS, type CandleInterval, type Criterion, type Horizon, type Market, type RawFactors, type RankRule, type Validation } from "../src/engine/screener";
import { cached } from "./cache";
import { parse, parseStock } from "./market";
import { cryptoUniverse } from "./universe";
import { parseExtra } from "./stocks-extra";

const UA = { "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15", Accept: "application/json" };
async function getJSON(url: string): Promise<any> {
  const r = await fetch(url, { headers: UA, signal: AbortSignal.timeout(12_000) });
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

export interface Listed { symbol: string; name: string; sector: string; marketCap: number }

/** Company names without the listing suffix ("Apple Inc. Common Stock" → "Apple Inc."). */
const cleanName = (s: string) => s.replace(/\s+(Common Stock|Ordinary Shares|American Depositary Shares?|Class [A-C] (Common|Ordinary|Capital) Stock|Capital Stock|Common Shares).*$/i, "").trim();

/** Largest US-listed companies, one share class per company (GOOGL kept, GOOG dropped). */
export function parseListed(d: any, size = 150): Listed[] {
  const rows = ((d?.data?.rows ?? []) as any[])
    .map((r) => ({ symbol: String(r.symbol).trim().replace(/[./]/g, "-"), name: cleanName(String(r.name ?? "")), sector: String(r.sector || "Autre"), marketCap: Number(String(r.marketCap ?? "").replace(/,/g, "")) || 0 }))
    .filter((r) => r.marketCap > 0 && /^[A-Z][A-Z0-9-]{0,6}$/.test(r.symbol))
    .sort((a, b) => b.marketCap - a.marketCap);
  const seen = new Set<string>();
  const out: Listed[] = [];
  for (const r of rows) {
    const company = r.name.replace(/\b(Class|Series) [A-C]\b/gi, "").toLowerCase().replace(/[^a-z0-9]/g, "");
    if (seen.has(company)) continue;
    seen.add(company);
    out.push(r);
    if (out.length >= size) break;
  }
  return out;
}

export const SECTOR_FR: Record<string, string> = {
  Technology: "Technologie", Finance: "Finance", "Health Care": "Santé", "Consumer Discretionary": "Consommation",
  Industrials: "Industrie", Energy: "Énergie", Telecommunications: "Télécoms", "Consumer Staples": "Biens de base",
  "Basic Materials": "Matériaux", "Real Estate": "Immobilier", Utilities: "Services publics", Miscellaneous: "Divers", Autre: "Autre",
};

export const listed = () => cached("screener:listed", 12 * 3_600_000, async () => parseListed(await getJSON("https://api.nasdaq.com/api/screener/stocks?tableonly=true&download=true")));

/** Daily candles for selection: Finviz (10 years in one call), Yahoo as fallback; ≈ 3 years kept. */
export function dailyFor(symbol: string): Promise<Candle[]> {
  return cached(`screener:daily:${symbol}`, 3 * 3_600_000, async () => {
    try {
      const c = parseExtra.finviz(await getJSON(`https://finviz.com/api/quote.ashx?instrument=stock&ticker=${encodeURIComponent(symbol)}&timeframe=d`));
      if (c.length > 300) return c.slice(-2600);
    } catch {}
    return parseStock.yahoo(await getJSON(`https://query1.finance.yahoo.com/v8/finance/chart/${encodeURIComponent(symbol)}?interval=1d&range=10y`)).slice(-2600);
  });
}

// ---------- Cryptos ----------

/** Pegged or wrapped tokens: they follow a currency, gold or another coin, there is nothing to choose there. */
const STABLE = new Set("USDT USDC DAI FDUSD TUSD USDE USDS PYUSD USDD BUSD FRAX USD1 RLUSD EURC USDP GUSD LUSD SUSD USDX USDG USD0 BFUSD USDTB EUROC AEUR XUSD USDQ USDF GHO CRVUSD DOLA MIM USDM USDB USDA USDAI FXUSD EUSD USTB BUIDL OUSG USYC USDZ AUSD".split(" "));
const WRAPPED = /^(W(BTC|ETH|BNB|SOL|STETH|EETH|TRX|AVAX)|STETH|WSTETH|WEETH|CBBTC|CBETH|RETH|METH|EZETH|RSETH|SOLVBTC|LBTC|BTCB|TBTC|PAXG|XAUT|JITOSOL|MSOL|BNSOL|SUSDE|SAVAX|STSOL|OSETH|SWETH|ETHX|CMETH|LSETH|FBTC|UNIBTC|PUMPBTC|ENZOBTC|CLBTC|BBTC)$/;

export const cryptoListed = () =>
  cached("screener:crypto:listed", 6 * 3_600_000, async () =>
    (await cryptoUniverse())
      .filter((e) => e[2] > 0 && !STABLE.has(e[0]) && !WRAPPED.test(e[0]))
      .sort((a, b) => a[2] - b[2])
      .map((e): Listed => ({ symbol: e[0], name: e[1], sector: "Crypto", marketCap: 0 })));

/** Daily crypto candles (≈ 1 000 days): Gate, then MEXC, then Kraken. */
export function cryptoDailyFor(base: string): Promise<Candle[]> {
  return cached(`screener:cdaily:${base}`, 3 * 3_600_000, async () => {
    const sources = [
      async () => parse.gate(await getJSON(`https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=${base}_USDT&interval=1d&limit=1000`)),
      async () => parse.binance(await getJSON(`https://api.mexc.com/api/v3/klines?symbol=${base}USDT&interval=1d&limit=1000`)),
      async () => parse.kraken(await getJSON(`https://api.kraken.com/0/public/OHLC?pair=${base === "BTC" ? "XBT" : base}USD&interval=1440`)),
    ];
    for (const src of sources) {
      try {
        // The current day is still forming: only closed days.
        const c = (await src()).filter((x) => x.close > 0 && x.time + 86_400_000 <= Date.now()).sort((a, b) => a.time - b.time);
        if (c.length > 300) return c;
      } catch {}
    }
    throw new Error("historique indisponible");
  });
}

// ---------- Intraday candles ----------

const INTERVAL_MS: Record<Exclude<CandleInterval, "1d">, number> = { "5m": 300_000, "15m": 900_000, "30m": 1_800_000 };
/** Refresh: a 30-minute selection must follow the market, a 6-month one can wait. */
export const TTL: Record<CandleInterval, number> = { "5m": 4 * 60_000, "15m": 8 * 60_000, "30m": 12 * 60_000, "1d": 30 * 60_000 };
/** Only closed candles (the current one is still moving). */
const closed = (c: Candle[], ms: number) => c.filter((x) => x.close > 0 && x.time + ms <= Date.now()).sort((a, b) => a.time - b.time);

/** Stock intraday candles (Yahoo): recent window for the ranking, 60 days for the replay. */
export function stockIntraday(symbol: string, interval: Exclude<CandleInterval, "1d">, replay = false): Promise<Candle[]> {
  const range = replay ? "60d" : interval === "5m" ? "5d" : "1mo";
  return cached(`screener:si:${symbol}:${interval}:${range}`, replay ? 6 * 3_600_000 : TTL[interval], async () =>
    closed(parseStock.yahoo(await getJSON(`https://query1.finance.yahoo.com/v8/finance/chart/${encodeURIComponent(symbol)}?interval=${interval}&range=${range}&includePrePost=false`)), INTERVAL_MS[interval]));
}

/** Crypto intraday candles (Gate, 1 000 candles: 3.5 days in 5 min, 10 days in 15 min, 3 weeks in 30 min). */
export function cryptoIntraday(base: string, interval: Exclude<CandleInterval, "1d">): Promise<Candle[]> {
  return cached(`screener:ci:${base}:${interval}`, TTL[interval], async () =>
    closed(parse.gate(await getJSON(`https://api.gateio.ws/api/v4/spot/candlesticks?currency_pair=${base}_USDT&interval=${interval}&limit=1000`)), INTERVAL_MS[interval]));
}

function candlesFor(market: Market, symbol: string, interval: CandleInterval, replay = false): Promise<Candle[]> {
  if (interval === "1d") return market === "crypto" ? cryptoDailyFor(symbol) : dailyFor(symbol);
  return market === "crypto" ? cryptoIntraday(symbol, interval) : stockIntraday(symbol, interval, replay);
}

async function mapLimit<T, R>(items: T[], limit: number, fn: (t: T) => Promise<R>): Promise<R[]> {
  const out = new Array<R>(items.length);
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (next < items.length) {
      const i = next++;
      out[i] = await fn(items[i]!);
    }
  }));
  return out;
}

export interface Candidate {
  rank: number;
  symbol: string;
  name: string;
  sector: string;
  marketCap: number;
  price: number;
  scores: Record<Criterion, number>;
  why: Record<Criterion, string>;
  action: string;
  zoneStatus: string;
  /** Buy now, or a limit order at the top of the buy zone when the price is a little above it. */
  plan: { entry: number; limit: number | null; stop: number; target: number; atrPct: number } | null;
  track: { trades: number; winRate: number; avgReturn: number } | null;
  checks: { label: string; ok: boolean; detail: string }[];
}

export interface ScreenResult {
  market: Market;
  horizon: Horizon;
  asOf: number;
  scanned: number;
  criteria: Record<Criterion, string>;
  roles: Record<Criterion, string>;
  /** Which criterion ranks, and how (for the page). */
  rankBy: Criterion;
  rankRule: RankRule;
  holdText: string;
  evidence: string;
  /** Stocks: the US session is closed, intraday rankings come from the last session. */
  marketClosed: boolean;
  buy: Candidate[];
  watch: (Candidate & { reason: string })[];
  setAside: { symbol: string; name: string; reason: string }[];
  validation: Validation | null;
}

type Verify = (symbol: string, interval: CandleInterval) => Promise<{ reliability: "high" | "medium" | "low"; shock: string; reversalDown: boolean; daily: Candle[] }>;

/** Stage 1 on the whole universe (ranking), stage 2 on the best candidates (checks), then the final lists. */
export async function screen(h: Horizon, verify: Verify, market: Market = "stock", topN = 10, marketClosed = false): Promise<ScreenResult> {
  const crypto = market === "crypto";
  const spec = SPECS[market][h];
  const universe = (await (crypto ? cryptoListed() : listed())).slice(0, crypto ? 120 : 150);
  const candles = await mapLimit(universe, 10, (s) => candlesFor(market, s.symbol, spec.interval).catch(() => [] as Candle[]));
  // Cryptos that barely move are pegged tokens the lists missed: nothing to choose there.
  const pegged = new Set<string>();
  const factors: (RawFactors | null)[] = candles.map((c, i) => {
    const f = c.length > 220 ? factorsAt(c, c.length - 1, spec) : null;
    if (f && crypto && isPegged(f.atrPct, spec.interval)) {
      pegged.add(universe[i]!.symbol);
      return null;
    }
    return f;
  });
  const scored = scoreUniverse(factors, spec);
  const rows = universe.map((s, i) => ({ s, f: factors[i], sc: scored[i], c: candles[i]! })).filter((x) => x.f && x.sc);
  // A few more than needed: some will fail the checks.
  const finalists = pick(rows, (x) => x.sc!.total, (x) => (crypto ? undefined : x.s.sector), topN + 6);

  const checked = await mapLimit(finalists, 4, async ({ s, f, sc, c }) => {
    let v: Awaited<ReturnType<Verify>> | null = null;
    try {
      v = await verify(s.symbol, spec.interval);
    } catch {}
    const bt = trackRecord(backtest(v?.daily.length ? v.daily : c.slice(-500)));
    const checks: Candidate["checks"] = [
      { label: "Données recoupées", ok: !!v && v.reliability !== "low", detail: v ? `fiabilité ${v.reliability === "high" ? "élevée" : v.reliability === "medium" ? "moyenne" : "faible"} sur les sources indépendantes` : "sources indisponibles" },
      { label: "Garde-fou marché", ok: !!v && v.shock !== "shock" && !v.reversalDown, detail: !v ? "indisponible" : v.shock === "shock" ? "marché en choc sur ce titre" : v.reversalDown ? "risque de retournement à la baisse élevé" : v.shock === "agitated" ? "agité : taille divisée par deux" : "conditions normales" },
      { label: "Tendance de fond", ok: f!.trend >= 40, detail: f!.trend >= 70 ? "haussière" : f!.trend >= 40 ? "mitigée" : "baissière" },
    ];
    const atrAbs = f!.atrPct != null ? (f!.atrPct / 100) * f!.price : null;
    const entry = f!.price;
    const zoneTop = f!.zoneStatus === "above" && f!.zoneDistance != null && f!.zoneDistance <= 10 ? entry * (1 - f!.zoneDistance / 100) : null;
    // Stop and target from the price actually paid: the limit when there is one.
    const base = zoneTop ?? entry;
    const stop = atrAbs ? base - spec.stopAtr * atrAbs : null;
    return {
      cand: {
        rank: 0, symbol: s.symbol, name: s.name, sector: crypto ? "Crypto" : SECTOR_FR[s.sector] ?? s.sector, marketCap: s.marketCap, price: f!.price,
        scores: sc!.scores, why: explain(f!, sc!, spec, market), action: f!.action, zoneStatus: f!.zoneStatus,
        plan: stop && f!.atrPct != null ? { entry, limit: zoneTop, stop, target: base + 2 * (base - stop), atrPct: f!.atrPct } : null,
        track: bt.trades ? bt : null, checks,
      } satisfies Candidate,
    };
  });

  const buy: Candidate[] = [], watch: ScreenResult["watch"] = [], setAside: ScreenResult["setAside"] = [];
  for (const { cand } of checked) {
    const data = cand.checks[0]!, guard = cand.checks[1]!, trend = cand.checks[2]!;
    // A short-term rebound ("reversal") buys what has just fallen: a bearish trend is expected there, not a warning.
    const trendMatters = spec.rank !== "reversal";
    if (!data.ok) setAside.push({ symbol: cand.symbol, name: cand.name, reason: `${data.label} : ${data.detail}` });
    else if (!guard.ok || (trendMatters && !trend.ok)) watch.push({ ...cand, reason: !guard.ok ? `${guard.label} : ${guard.detail}` : `${trend.label} ${trend.detail}` });
    else if (buy.length < topN) buy.push({ ...cand, rank: buy.length + 1 });
  }

  const validation = await cached(`screener:validation:${market}:${h}:${topN}`, spec.interval === "1d" ? 12 * 3_600_000 : 2 * 3_600_000, async () => {
    const hist = spec.interval === "1d" || crypto ? candles : await mapLimit(universe, 10, (s) => candlesFor(market, s.symbol, spec.interval, true).catch(() => [] as Candle[]));
    const longest = Math.max(...hist.map((c) => c.length));
    // Daily: stocks with ≈ 6 years (replay from 2020), cryptos with ≈ 2.5 years. Intraday: the full window.
    const minLen = spec.interval === "1d" ? (crypto ? 900 : 1500) : Math.floor(longest * 0.8);
    const usable = universe.map((s, i) => ({ s, c: hist[i]! })).filter((x) => x.c.length > minLen && !pegged.has(x.s.symbol));
    const btc = crypto ? usable.findIndex((x) => x.s.symbol === "BTC") : -1;
    return validate(alignSeries(usable.map((x) => x.c), spec.interval !== "1d"), spec, h, topN, crypto ? undefined : usable.map((x) => x.s.sector), btc >= 0 ? btc : undefined);
  }).catch(() => null);

  return {
    market, horizon: h, asOf: Date.now(), scanned: rows.length, criteria: CRITERIA, roles: roles(spec, market),
    rankBy: RANKED_CRITERION[spec.rank], rankRule: spec.rank, holdText: span(spec.hold, spec.interval, market), evidence: spec.evidence,
    marketClosed: !crypto && spec.interval !== "1d" && marketClosed, buy, watch, setAside, validation,
  };
}
