/**
 * Data audit: compares what Altim shows with independent references it does not use for the same figure.
 *   bun scripts/audit.ts [server]   (default http://localhost:3000: `cd backend && cargo run --release`, without login)
 * - Live prices: crypto vs CoinPaprika (not an Altim source), stocks vs Yahoo Finance (1 source out of ~12).
 * - Daily closes over a year (history, charts, simulator): crypto vs Coinbase daily candles (Altim's daily history
 *   comes from Bitstamp, Binance, Gate, MEXC, Kraken), stocks vs Yahoo daily chart (Altim: Robinhood, Nasdaq, WSJ…).
 *   CoinGecko was tried first and dropped: its daily points are off by 10–14 % on some days (LINK on 7 March 2026:
 *   10,16 $ where Coinbase, Kraken, Bitstamp and Gate all closed at 8,70 $).
 * - Change shown next to each price: crypto over 24 h (recomputed from Coinbase 5-minute candles), stocks since the
 *   previous close (Yahoo); the moves of "Point du jour" since the last daily close; the simulator on BTC.
 * Prints a table and exits 1 when a figure is off by more than the tolerance.
 */
export {};

const SERVER = process.argv[2] ?? "http://localhost:3000";
const UA = { "User-Agent": "Mozilla/5.0 (Macintosh) AppleWebKit/605.1.15 Safari/605.1.15 AltimAudit/1.0" };
// Symbol, Coinbase product (null: not listed there), CoinPaprika id.
const CRYPTO: [string, string | null, string][] = [
  ["BTC", "BTC-USD", "btc-bitcoin"], ["ETH", "ETH-USD", "eth-ethereum"], ["SOL", "SOL-USD", "sol-solana"],
  ["BNB", null, "bnb-binance-coin"], ["XRP", "XRP-USD", "xrp-xrp"], ["ADA", "ADA-USD", "ada-cardano"],
  ["DOGE", "DOGE-USD", "doge-dogecoin"], ["AVAX", "AVAX-USD", "avax-avalanche"], ["LINK", "LINK-USD", "link-chainlink"],
];
const STOCKS = ["AAPL", "NVDA", "MSFT", "AMZN", "GOOGL", "META", "TSLA", "SPY"];
const DAY = 86_400_000;

async function json<T>(url: string): Promise<T> {
  for (let i = 0; i < 3; i++) {
    const r = await fetch(url, { headers: UA, signal: AbortSignal.timeout(30_000) });
    if (r.ok) return (await r.json()) as T;
    if (r.status === 429) await new Promise((res) => setTimeout(res, 15_000));
    else throw new Error(`${url} → HTTP ${r.status}`);
  }
  throw new Error(`${url} → trop de requêtes`);
}

type Row = { check: string; asset: string; altim: string; reference: string; gap: number; tolerance: number };
const rows: Row[] = [];
const fmt = (v: number) => (v >= 100 ? v.toFixed(2) : v.toPrecision(5));
const add = (check: string, asset: string, a: number, ref: number, tolerance: number, gap = Math.abs(a / ref - 1) * 100) =>
  rows.push({ check, asset, altim: fmt(a), reference: fmt(ref), gap, tolerance });

// ---------- Live prices ----------
const assets = [...CRYPTO.map(([s]) => `${s}:crypto`), ...STOCKS.map((s) => `${s}:stock`)].join(",");
const tickers = await json<{ symbol: string; kind: string; price: number; change: number | null }[]>(`${SERVER}/api/tickers?symbols=${assets}`);
const paprika = await json<{ id: string; quotes: { USD: { price: number; percent_change_24h: number } } }[]>("https://api.coinpaprika.com/v1/tickers?limit=100");
for (const [sym, , pap] of CRYPTO) {
  const t = tickers.find((x) => x.symbol === sym && x.kind === "crypto");
  const ref = paprika.find((p) => p.id === pap)?.quotes.USD;
  if (!t || !ref) { rows.push({ check: "prix", asset: sym, altim: t ? fmt(t.price) : "—", reference: ref ? fmt(ref.price) : "—", gap: NaN, tolerance: 1 }); continue; }
  // Different instants (a few seconds to minutes apart): 1 % tolerance.
  add("prix en direct", sym, t.price, ref.price, 1);
  // 24 h change: recomputed from Coinbase 5-minute candles (CoinPaprika's own 24 h figure lags by up to a point).
  const product = CRYPTO.find(([x]) => x === sym)![1];
  if (t.change != null && product) {
    // 5-minute candles (300 = 25 h): the price 24 h ago within 5 minutes.
    const hours = await json<number[][]>(`https://api.exchange.coinbase.com/products/${product}/candles?granularity=300`);
    const ago = hours.find((h) => h[0]! <= hours[0]![0]! - 24 * 3600);
    if (ago) {
      const refChange = (hours[0]![4]! / ago[4]! - 1) * 100;
      rows.push({ check: "variation 24 h (points de %)", asset: sym, altim: t.change.toFixed(2), reference: refChange.toFixed(2), gap: Math.abs(t.change - refChange), tolerance: 0.5 });
    }
  }
}
type YChart = { chart: { result: { meta: { regularMarketPrice: number; chartPreviousClose: number }; timestamp: number[]; indicators: { quote: { close: (number | null)[] }[] } }[] } };
const yahoo: Record<string, YChart["chart"]["result"][0]> = {};
for (const s of STOCKS) {
  yahoo[s] = (await json<YChart>(`https://query1.finance.yahoo.com/v8/finance/chart/${s}?range=1y&interval=1d`)).chart.result[0]!;
  const t = tickers.find((x) => x.symbol === s && x.kind === "stock");
  if (t) add("prix en direct", s, t.price, yahoo[s]!.meta.regularMarketPrice, 0.5);
}

// ---------- Daily closes over a year ----------
type Hist = { series: { symbol: string; kind: string; closes: [number, number][] }[] };
const hist = await json<Hist>(`${SERVER}/api/history?days=365&symbols=${assets}`);
const closesOf = (sym: string, kind: string) => hist.series.find((s) => s.symbol === sym && s.kind === kind)?.closes ?? [];
// One function per asset: with Bun, a Map declared inside a top-level-await loop was read back from the first
// iteration (SOL compared with Bitcoin closes), so each asset gets its own scope.
async function cryptoCloses(sym: string, product: string) {
  const ours = closesOf(sym, "crypto");
  // Coinbase daily candles [time, low, high, open, close, volume], 300 at most per request.
  const refByDay = new Map<number, number>();
  const end = Date.now();
  for (let to = end; to > end - 370 * DAY; to -= 300 * DAY) {
    const from = Math.max(end - 370 * DAY, to - 300 * DAY);
    const candles = await json<number[][]>(`https://api.exchange.coinbase.com/products/${product}/candles?granularity=86400&start=${new Date(from).toISOString()}&end=${new Date(to).toISOString()}`);
    for (const c of candles) refByDay.set(c[0]! * 1000, c[4]!);
    await new Promise((r) => setTimeout(r, 400));
  }
  const gaps = ours.map(([t, c]) => { const r = refByDay.get(Math.floor(t / DAY) * DAY); return r ? Math.abs(c / r - 1) * 100 : null; }).filter((g): g is number => g != null);
  gaps.sort((a, b) => a - b);
  rows.push({ check: `clôtures 1 an (${gaps.length} j) médiane`, asset: sym, altim: `${ours.length} clôtures`, reference: "Coinbase", gap: gaps[Math.floor(gaps.length / 2)] ?? NaN, tolerance: 0.3 });
  rows.push({ check: "clôtures 1 an pire écart", asset: sym, altim: "", reference: "", gap: gaps.at(-1) ?? NaN, tolerance: 2 });
}
for (const [sym, product] of CRYPTO) if (product) await cryptoCloses(sym, product);
for (const s of STOCKS) {
  const ours = closesOf(s, "stock");
  const y = yahoo[s]!;
  const refByDay = new Map(y.timestamp.map((t, i) => [Math.floor((t * 1000) / DAY) * DAY, y.indicators.quote[0]!.close[i]]));
  const gaps = ours.map(([t, c]) => { const r = refByDay.get(Math.floor(t / DAY) * DAY); return r ? Math.abs(c / r - 1) * 100 : null; }).filter((g): g is number => g != null);
  gaps.sort((a, b) => a - b);
  rows.push({ check: `clôtures 1 an (${gaps.length} j) médiane`, asset: s, altim: `${ours.length} clôtures`, reference: "Yahoo", gap: gaps[Math.floor(gaps.length / 2)] ?? NaN, tolerance: 0.3 });
  rows.push({ check: "clôtures 1 an pire écart", asset: s, altim: "", reference: "", gap: gaps.at(-1) ?? NaN, tolerance: 2 });
  // Change since the previous close, as shown on the radar.
  const t = tickers.find((x) => x.symbol === s && x.kind === "stock");
  const day = (await json<YChart>(`https://query1.finance.yahoo.com/v8/finance/chart/${s}?range=1d&interval=1d`)).chart.result[0]!;
  const refChange = (day.meta.regularMarketPrice / day.meta.chartPreviousClose - 1) * 100;
  if (t?.change != null) rows.push({ check: "variation affichée (points de %)", asset: s, altim: t.change.toFixed(2), reference: refChange.toFixed(2), gap: Math.abs(t.change - refChange), tolerance: 0.5 });
}

// ---------- Simulator ("Si j'avais investi") on BTC, replayed on the reference closes ----------
{
  const { simulateDca } = await import("../src/engine/dca");
  const btcRef: [number, number][] = [];
  const end = Date.now();
  for (let to = end; to > end - 370 * DAY; to -= 300 * DAY) {
    const from = Math.max(end - 370 * DAY, to - 300 * DAY);
    for (const c of await json<number[][]>(`https://api.exchange.coinbase.com/products/BTC-USD/candles?granularity=86400&start=${new Date(from).toISOString()}&end=${new Date(to).toISOString()}`)) btcRef.push([c[0]! * 1000, c[4]!]);
  }
  // Same days as Altim (its last closed day), so both replays buy on the same dates.
  const last = closesOf("BTC", "crypto").at(-1)![0];
  const ref = btcRef.filter(([t]) => t <= last);
  for (const [every, label] of [[30, "100 $/mois 1 an"], [0, "achat unique 1 an"]] as const) {
    const a = every ? simulateDca(closesOf("BTC", "crypto"), 100, every, 365)! : simulateDca(closesOf("BTC", "crypto"), 1300, 100_000, 365)!;
    const r = every ? simulateDca(ref, 100, every, 365)! : simulateDca(ref, 1300, 100_000, 365)!;
    add(`simulateur BTC ${label} (valeur)`, "BTC", a.value, r.value, 0.5);
  }
}

// ---------- "Point du jour": moves since the last daily close ----------
{
  const brief = await json<{ movers: { symbol: string; kind: string; change: number }[] }>(`${SERVER}/api/brief?symbols=BTC:crypto,ETH:crypto,SOL:crypto`);
  for (const [sym, product] of CRYPTO.slice(0, 3)) {
    const m = brief.movers.find((x) => x.symbol === sym);
    const today = await json<number[][]>(`https://api.exchange.coinbase.com/products/${product}/candles?granularity=86400&start=${new Date(Math.floor(Date.now() / DAY) * DAY - DAY).toISOString()}&end=${new Date().toISOString()}`);
    const prevClose = today.find((c) => c[0]! * 1000 === Math.floor(Date.now() / DAY) * DAY - DAY)?.[4];
    const now = tickers.find((x) => x.symbol === sym)?.price;
    if (m && prevClose && now) rows.push({ check: "point du jour : variation depuis la clôture", asset: sym, altim: m.change.toFixed(2), reference: ((now / prevClose - 1) * 100).toFixed(2), gap: Math.abs(m.change - (now / prevClose - 1) * 100), tolerance: 0.3 });
  }
}

const bad = rows.filter((r) => !(r.gap <= r.tolerance));
console.table(rows.map((r) => ({ ...r, gap: Number.isFinite(r.gap) ? `${r.gap.toFixed(3)} %` : "—", ok: r.gap <= r.tolerance ? "✔" : "✘" })));
console.log(bad.length ? `✘ ${bad.length} écart(s) au-delà de la tolérance` : "✔ Toutes les données sont dans la tolérance");
process.exit(bad.length ? 1 : 0);
