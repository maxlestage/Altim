/**
 * Golden outputs of the TypeScript market data layer (web/server/{market,quotes,stocks-extra,live}.ts), compared
 * by backend/tests/parity_market.rs. Imported at the end of golden.ts (bun parity/golden.ts).
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { inputs, find, NOW, write } from "./golden";
import type { Candle } from "../../web/src/engine/signal";
import { aggregate, aggregateSession, blend, consensus, crossCheck, deviations, nyOpen, parse, parseStock } from "../../web/server/market";
import { combine, parseQuotes, type Asset, type SourceQuote } from "../../web/server/quotes";
import { parseExtra } from "../../web/server/stocks-extra";
import { FEEDS, liveConsensus, usMarketOpen } from "../../web/server/live";

const TEST = join(import.meta.dir, "../../web/test");
const samples = JSON.parse(readFileSync(join(TEST, "samples.json"), "utf8"));
const extra = JSON.parse(readFileSync(join(TEST, "stocks-extra-samples.json"), "utf8"));
const live = JSON.parse(readFileSync(join(TEST, "live-samples.json"), "utf8"));
const H = 3_600_000;

/** Output or thrown message, like the sources lists show it. */
const attempt = (f: () => unknown) => {
  try {
    const r = f();
    return r instanceof Map ? { map: [...r.entries()] } : { ok: r ?? null };
  } catch (e) {
    return { error: (e as Error).message };
  }
};
/** Deterministic perturbation of a series (another exchange's view of the same market). */
const perturb = (c: Candle[], k: number, drop = 0): Candle[] =>
  c.filter((_, i) => !drop || i % drop !== 0).map((x, i) => {
    // Plain arithmetic (no Math.sin): the Rust test rebuilds the same series bit for bit.
    const f = 1 + (k * (((i * 7) % 11) - 5)) / 5;
    return { ...x, open: x.open * f, high: x.high * (f + Math.abs(k)), low: x.low * (f - Math.abs(k)), close: x.close * (1 + (k * (((i * 3) % 7) - 3)) / 3), volume: x.volume * (1 + (i % 3)) };
  });

// ---------- aggregate ----------
{
  const cases = [];
  for (const i of inputs.filter((x) => x.interval === "1h")) {
    cases.push({ args: { symbol: i.symbol, interval: "1h", from: H, to: 4 * H }, output: aggregate(i.candles, H, 4 * H) });
    cases.push({ args: { symbol: i.symbol, interval: "1h", from: H, to: 24 * H }, output: aggregate(i.candles, H, 24 * H) });
  }
  for (const i of inputs.filter((x) => x.interval === "4h" && x.kind === "crypto")) {
    cases.push({ args: { symbol: i.symbol, interval: "4h", from: 4 * H, to: 24 * H }, output: aggregate(i.candles, 4 * H, 24 * H) });
  }
  write("market-aggregate", cases);
}

// ---------- aggregateSession ----------
{
  const cases = [];
  for (const i of inputs.filter((x) => x.interval === "1h" && x.kind === "stock")) {
    for (const per of [4, 2, 7]) cases.push({ args: { symbol: i.symbol, per }, output: aggregateSession(i.candles, per) });
  }
  // Hourly sessions around the daylight saving changes (March and November 2025, 2026).
  const synth: Candle[] = [];
  for (const day of ["2025-03-06", "2025-03-07", "2025-03-10", "2025-03-11", "2025-10-31", "2025-11-03", "2025-11-04", "2026-03-06", "2026-03-09", "2026-10-30", "2026-11-02"]) {
    const [y, m, d] = day.split("-").map(Number);
    const open = nyOpen(y!, m!, d!);
    for (let k = 0; k < 7; k++) synth.push({ time: open + k * H, open: 100 + k, high: 101 + k, low: 99 + k, close: 100.5 + k, volume: 10 + k });
  }
  // Late-evening candles (UTC next day, same New York day).
  synth.push({ time: Date.UTC(2026, 10, 3, 3, 0), open: 1, high: 2, low: 0.5, close: 1.5, volume: 1 });
  synth.push({ time: Date.UTC(2026, 10, 3, 5, 0), open: 1, high: 2, low: 0.5, close: 1.5, volume: 1 });
  cases.push({ args: { symbol: "synth", per: 4 }, output: aggregateSession(synth, 4) });
  write("market-session", cases);
}

// ---------- deviations / blend ----------
{
  const dev = [], bl = [];
  for (const i of inputs.filter((x) => x.interval !== "long")) {
    const a = i.candles.slice(-150), b = perturb(a, 0.0005), c = perturb(a, -0.0003, 7), d = perturb(a, 0.04).slice(50), e = a.slice(-10).map((x) => ({ ...x, time: x.time + 1 }));
    dev.push({ args: { symbol: i.symbol, interval: i.interval, set: "abcde" }, output: deviations([a, b, c, d, e]) });
    dev.push({ args: { symbol: i.symbol, interval: i.interval, set: "ab" }, output: deviations([a, b]) });
    dev.push({ args: { symbol: i.symbol, interval: i.interval, set: "a" }, output: deviations([a]) });
    bl.push({ args: { symbol: i.symbol, interval: i.interval, set: "a|bc" }, output: blend(a, [b, c]) });
    bl.push({ args: { symbol: i.symbol, interval: i.interval, set: "c|abd" }, output: blend(c, [a, b, d]) });
  }
  write("market-deviations", dev);
  write("market-blend", bl);
}

// ---------- consensus (fake sources built from the real candles) ----------
{
  const cases = [];
  for (const i of inputs.filter((x) => x.interval !== "long" && x.kind === "crypto")) {
    const iv = i.interval as "1h" | "4h" | "1d";
    const a = i.candles.slice(-120);
    const series: Record<string, Candle[] | string> = {
      A: a, B: perturb(a, 0.0005), C: perturb(a, -0.0003, 7), Faux: perturb(a, 0.04), Panne: "HTTP 451",
      Court: a.slice(-30), Lente: a.slice(0, -5), Vide: [], Futur: [...a, { ...a.at(-1)!, time: Date.now() + 10 * H }],
    };
    const src = (name: string) => ({ name, fetch: async () => { const s = series[name]!; if (typeof s === "string") throw new Error(s); return s; } });
    for (const [names, target, tol] of [
      [["Panne", "Faux", "A", "B", "C"], 3, 0.5], [["A", "B", "C", "Faux"], Infinity, 0.5], [["Court", "A", "B"], Infinity, 0.5],
      [["Lente", "A", "B"], Infinity, 0.5], [["Vide", "Panne"], Infinity, 0.5], [["Faux", "A"], Infinity, 0.5], [["A", "Faux", "B", "C", "Futur"], 2, 1],
      [["Faux", "Court", "Panne", "A", "B", "C"], 4, 0.05],
    ] as [string[], number, number][]) {
      const out = await consensus(i.symbol, iv, names.map(src), target, tol).catch((e: Error) => ({ error: e.message }));
      cases.push({ args: { symbol: i.symbol, interval: iv, names, target: Number.isFinite(target) ? target : null, tol }, output: out });
    }
  }
  write("market-consensus", cases);
}

// ---------- crossCheck ----------
{
  const cases = [];
  const quotes = [
    { name: "Yahoo Finance", ok: true, price: 100.1 }, { name: "Nasdaq", ok: true, price: 101 }, { name: "Cboe", ok: true, price: 105 },
    { name: "TradingView", ok: false, error: "non coté" }, { name: "Zacks", ok: false }, { name: "Webull", ok: true, price: 98.5 }, { name: "Fidelity", ok: true, price: 0 },
  ];
  for (const [last, known, tol] of [[100, ["Yahoo Finance"], 2], [100, [], 3], [97, ["Nasdaq", "Cboe"], 2], [103.2, ["Yahoo Finance"], 3]] as [number, string[], number][]) {
    cases.push({ args: { last, quotes, known, tol }, output: crossCheck(last, quotes, known, tol) });
  }
  write("market-crosscheck", cases);
}

// ---------- nyOpen ----------
{
  const cases = [];
  for (let t = Date.UTC(2023, 0, 1); t <= Date.UTC(2027, 11, 31); t += 86_400_000) {
    const d = new Date(t);
    const [y, m, day] = [d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate()];
    cases.push({ args: [y, m, day], output: nyOpen(y, m, day) });
  }
  for (const a of [[2026, 13, 1], [2026, 3, 0], [2026, 2, 30], [2026, 11, 32], [99, 6, 1], [1970, 1, 1], [2000, 2, 29], [2038, 3, 14], [1999, 10, 31]]) {
    cases.push({ args: a, output: nyOpen(a[0]!, a[1]!, a[2]!) });
  }
  cases.push({ args: [NaN, 1, 1], output: attempt(() => nyOpen(NaN, 1, 1)) });
  write("market-nyopen", cases);
}

// ---------- parsers (real samples + malformed responses) ----------
{
  const s = await import("./server-samples.json");
  const cases = [];
  const bad: unknown[] = [{}, null, "s", 1, [null], [[]], [{}], { code: 3 }];
  const okShapes: Record<string, unknown> = {
    okx: { code: "0" }, kucoin: { code: "200000" }, kraken: { error: [] }, cryptocom: { code: 0 }, bitget: { code: "00000" }, htx: { status: "ok" },
    whitebit: { success: true }, coinex: { code: 0 }, xt: { rc: 0 }, woox: { success: true }, bingx: { code: 0 }, lbank: { result: "true" },
  };
  const wrong = (base: unknown) => [base, { ...(base as object), data: {}, result: {}, rows: {} }, { ...(base as object), data: [null], result: [null], rows: [null] },
    { ...(base as object), data: [[]], result: [[]], rows: [{}] }, { ...(base as object), result: { XXBTZUSD: {} }, data: { ohlc: {} } }, { ...(base as object), result: { XXBTZUSD: [null] }, data: { ohlc: [null] } }];
  for (const [k, f] of Object.entries(parse)) {
    const ins = [...bad, ...(okShapes[k] ? wrong(okShapes[k]) : []), ...((s as any).parse[k] ?? [])];
    if (k === "bitstamp") ins.push({ data: { ohlc: {} } }, { data: { ohlc: [null] } }, { data: null });
    if (k === "cryptocom") ins.push({ code: 0, result: { data: {} } }, { code: 0, result: { data: [null] } });
    if (k === "kraken") ins.push({ error: ["EQuery:Unknown asset pair"] }, { error: [], result: { last: 1 } }, { error: [], result: "ab" }, { error: "x" });
    if (k === "hitbtc") ins.push([{ timestamp: "garbage", open: "1" }]);
    for (const input of ins) cases.push({ args: { module: "market", parser: k, input }, output: attempt(() => (f as (d: unknown) => unknown)(input)) });
  }
  const stockBad: unknown[] = [{}, null, "s", [null], { chart: {} }, { chart: { result: [{}] } }, { chart: { error: { description: "No data found, symbol may be delisted" } } },
    { chart: { result: [{ timestamp: {} }] } }, { chart: { result: [{ timestamp: [1, 2], indicators: { quote: [{ open: [1, null], high: [2, 3], low: [0.5, 1], close: [1.5, 2] }] } }] } },
    { data: { tradesTable: { rows: [{ date: "9/25/2026", open: "$1,001.5", high: "$1,010", low: "990", close: "1,005", volume: "N/A" }, { date: "bad" }] } } },
    { data: { tradesTable: { rows: [null] } } }, { data: { tradesTable: { rows: {} } } }, { historicals: [null] }, { historicals: {} },
    { historicals: [{ begins_at: "2026-09-25T00:00:00Z", session: "pre" }, { begins_at: "2026-09-25T00:00:00Z", session: "reg", interpolated: true }, { begins_at: "2026-09-25T00:00:00Z", session: "reg", open_price: "1" }] },
    { data: [null] }, { data: {} }, { data: [{ t: "2026-09-25", o: 1 }, { date: "2026-09-24", open: "2" }] }, [{ data: [null] }], [{ data: [5] }], [{ data: {} }], [{ data: ["1790308800,1,2,3,4,5,,7", "x,1"] }],
  ];
  for (const [k, f] of Object.entries(parseStock)) {
    for (const input of [...stockBad, ...((s as any).stock[k] ?? [])]) cases.push({ args: { module: "stock", parser: k, input }, output: attempt(() => (f as (d: unknown) => unknown)(input)) });
  }
  write("market-parsers", cases);

  // Quotes (maps as entry lists).
  const q = [];
  const asset = (symbol: string, kind: "crypto" | "stock", gecko?: string): Asset => ({ symbol, name: symbol, kind, ...(gecko ? { gecko } : {}) });
  const qbad: unknown[] = [{}, null, "s", 1, [null], [[]], [{}], { code: "0" }, { code: "0", data: {} }, { code: "0", data: [null] }, { code: "1" }, { error: [] }, { error: ["EGeneral:Too many requests"] },
    { error: [], result: {} }, { data: {} }, { data: { primaryData: {} } }, { details: {} }, { results: [null, {}] }, { results: {} }, { data: [null] }, { data: [{ d: [] }] }, { chart: { result: [{}] } },
    { chart: { result: [{ indicators: { quote: [{}] } }] } }, { chart: { result: [{ meta: {}, indicators: { quote: [{ close: [] }] } }] } }, { bitcoin: {} }, { bitcoin: { usd: "12", usd_24h_change: null } }];
  for (const [k, f] of Object.entries(parseQuotes)) {
    for (const input of [...qbad, ...((s as any).quotes[k] ?? [])]) {
      const call = () => {
        switch (k) {
          case "okx": case "kraken": return (f as any)(input, ["BTC", "SOL", "BNB", "DOGE"]);
          case "coingecko": return (f as any)(input, [asset("BTC", "crypto", "bitcoin"), asset("ETH", "crypto", "ethereum")]);
          case "webullTicker": return (f as any)(input, "BRK-B");
          default: return (f as any)(input);
        }
      };
      q.push({ args: { parser: k, input }, output: attempt(call) });
    }
  }
  write("quotes-parsers", q);

  // stocks-extra parsers on the real samples and malformed responses.
  const x = [];
  const xbad: unknown[] = [{}, null, "s", 1, [null], { TimeInfo: {} }, { TimeInfo: { Ticks: [1790208000000, 1790294400000, 5] }, Series: [{ SeriesId: "s1", DataPoints: [[1, 2, 1, 2], [3, null, 1, 2]] }] },
    { unadjusted: {} }, { unadjusted: [null] }, { unadjusted: [{ x: "2026-09-25T00:00:00Z", open: "1", high: "2", low: "0.5", close: "1.5" }, { x: "2026-09-26", open: 0 }] },
    { date: [1790343000] }, { date: {} }, { date: [1790343000], open: [1], high: [2], low: [1], close: [2] }, { Dates: ["2026-09-25T00:00:00"] }, { Elements: [{ ComponentSeries: [null] }] },
    { Dates: ["2026-09-25T00:00:00", "2026-09-26T00:00:00"], Elements: [{ ComponentSeries: [{ Type: "Open", Values: [1, 2] }, { Type: "High", Values: [2, 3] }, { Type: "Low", Values: [1, 1] }, { Type: "Close", Values: [2] }] }] },
    { data: { security: [null] } }, { data: { security: {} } }, { data: { security: [{ symbol: "AAPL:NSQ", xid: "1" }, { symbol: "AAPL:NYQ", xid: "2", isPrimary: true }, { symbol: "AAPL:LSE", xid: "3", isPrimary: true }] } },
    { Candles: [{ Candles: [null] }] }, { Candles: [{ Candles: [{ FromDate: "2026-09-25T00:00:00Z", Open: 1, High: 2, Low: 1, Close: 2 }] }] },
    { InstrumentDisplayDatas: [null] }, { InstrumentDisplayDatas: [{ InstrumentID: "12", SymbolFull: "X", InstrumentTypeID: 6 }] },
    { prices: [{ p: 0 }, { p: "3" }] }, { prices: [{ p: "2" }, { p: "3" }] }, { prices: {} }, { close: "$1,000", lastClose: "0" }, { close: "1,000.5", lastClose: "990" }];
  const texts = ["", "()", "({})", '({"QUOTES":{"A.B":{"ERROR_CODE":"0","LAST_PRICE":"$1,200.5","PCT_CHG_TODAY":"x"},"C":{"ERROR_CODE":"1","LAST_PRICE":"1"},"D/E":{"ERROR_CODE":"0","LAST_PRICE":"0"}}});',
    '"symbol":"AAPL","x":1,"last": "12.5","previousClose":"10"', '"symbol" : "brk.b" "last":13 "previousClose":"0"', "<html>\"last\":\"5\"</html>"];
  for (const [k, f] of Object.entries(parseExtra)) {
    const ins = k === "fidelity" || k === "publicCom" ? [...texts, extra[k]] : [...xbad, extra[k === "ftXid" ? "ftSearch" : k]];
    for (const input of ins) {
      for (const sym of k === "ftXid" || k === "publicCom" ? ["AAPL", "MSFT", "BRK-B"] : [null]) {
        x.push({ args: { parser: k, input, symbol: sym }, output: attempt(() => (f as any)(input, sym)) });
      }
    }
  }
  write("extra-parsers", x);
}

// ---------- combine ----------
{
  const cases = [];
  const assets: Asset[] = [
    { symbol: "BTC", name: "Bitcoin", kind: "crypto", gecko: "bitcoin" }, { symbol: "ETH", name: "Ethereum", kind: "crypto" },
    { symbol: "AAPL", name: "Apple", kind: "stock" }, { symbol: "BRK-B", name: "Berkshire", kind: "stock" }, { symbol: "NONE", name: "Rien", kind: "stock" },
  ];
  type R = { name: string; kind: "crypto" | "stock"; quotes?: [string, SourceQuote][]; error?: string };
  const sets: R[][] = [
    [
      { name: "A", kind: "crypto", quotes: [["BTC", { price: 100, change: 1 }], ["ETH", { price: 3000, change: -2 }]] },
      { name: "B", kind: "crypto", quotes: [["BTC", { price: 100.1, change: 2 }], ["ETH", { price: 3001 }]] },
      { name: "C", kind: "crypto", quotes: [["BTC", { price: 99.95 }], ["ETH", { price: 3100, change: 5 }]] },
      { name: "Faux", kind: "crypto", quotes: [["BTC", { price: 110, change: 9 }]] },
      { name: "Panne", kind: "crypto", error: "HTTP 451" },
      { name: "Vide", kind: "crypto", quotes: [] },
      { name: "Y", kind: "stock", quotes: [["AAPL", { price: 341.07, change: 1.53 }], ["BRK-B", { price: 505.4 }]] },
      { name: "N", kind: "stock", quotes: [["AAPL", { price: 341.04, change: NaN }], ["BRK-B", { price: 512, change: 0.2 }]] },
      { name: "R", kind: "stock", quotes: [["AAPL", { price: 346.2, change: 1.2 }]] },
      { name: "T", kind: "stock", error: "The operation timed out." },
      { name: "Z", kind: "stock" },
    ],
    [{ name: "A", kind: "crypto", error: "x" }],
    [{ name: "A", kind: "stock", quotes: [["AAPL", { price: 10 }], ["BRK-B", { price: 11, change: 3 }]] }, { name: "B", kind: "stock", quotes: [["AAPL", { price: 10.2, change: 1 }]] }],
  ];
  for (const set of sets) {
    const results = set.map((r) => ({ name: r.name, kind: r.kind, ...(r.quotes ? { quotes: new Map(r.quotes) } : {}), ...(r.error ? { error: r.error } : {}) }));
    cases.push({ args: { assets, results: set }, output: combine(assets, results) });
  }
  write("quotes-combine", cases);
}

// ---------- live: feeds, consensus, US session ----------
{
  const feeds = [];
  for (const f of FEEDS) {
    const ctx = { chanIds: new Map<number, string>() };
    const msgs = [...(live[f.name] ?? []), { event: "subscribe" }, [1, "hb"], null, "pong", { arg: { channel: "tickers" }, data: [{ instId: "ETH-USDT", last: "0", open24h: "1" }, { instId: "SOL-USDT", last: "150.5", open24h: "0" }] },
      { type: "ticker", product_id: "PEPE-USD", price: "0.0000123", open_24h: "" }, { channel: "ticker", data: [{ symbol: "DOGE/USD", last: 0.2, change_pct: "x" }] },
      { event: "subscribed", channel: "ticker", chanId: 7, symbol: "tDOGE:USD" }, [7, [1, 2, 3, 4, 5, -0.01, 0.21, 8]], [7, [1, 2, 3, 4, 5, null, 0.21, 8]],
      { arg: { channel: "ticker" }, data: [{ instId: "PEPEUSDT", lastPr: "0.00001", open24h: "0.000011" }] },
      { channel: "spot.tickers", event: "update", result: { currency_pair: "ETH_USDT", last: "3000.1", change_percentage: "-1.2" } }, { channel: "spot.tickers", event: "subscribe", result: { status: "success" } },
      { method: "subscribe", result: { channel: "ticker", instrument_name: "ETH_USDT", data: [{ a: "3000", c: "-0.012" }, { a: "0", c: "1" }] } }];
    feeds.push({ args: { feed: f.name }, output: { parsed: msgs.map((m) => f.parse(m, ctx)), chanIds: [...ctx.chanIds.entries()], reply: msgs.map((m) => f.reply?.(m) ?? null) } });
    const subs = (m: unknown[]) => JSON.parse(JSON.stringify(m, (k, v) => (k === "time" || k === "id" ? 0 : v)));
    feeds.push({ args: { feed: f.name, subscribe: ["BTC", "PEPE", "ETH"] }, output: { subscribe: subs(f.subscribe(["BTC", "PEPE", "ETH"])), unsubscribe: subs(f.unsubscribe(["BTC", "PEPE"])), ping: f.ping ? { every: f.ping.every, message: subs([f.ping.message()]) } : null, url: f.url } });
  }
  write("live-feeds", feeds);

  const cons = [];
  const now = 1_790_500_000_000;
  const sets: [string, { price: number; change: number | null; time: number }][][] = [
    [["A", { price: 100, change: 1, time: now - 1000 }], ["B", { price: 100.4, change: 2, time: now - 500 }], ["C", { price: 99.8, change: null, time: now }], ["D", { price: 110, change: 9, time: now }], ["E", { price: 50, change: 0, time: now - 700_000 }]],
    [],
    [["A", { price: 0, change: 1, time: now }]],
    [["A", { price: 10, change: NaN, time: now - 119_000 }], ["B", { price: 10.05, change: 3, time: now - 121_000 }]],
    [["A", { price: 1, change: 1, time: now }], ["B", { price: 1.012, change: 2, time: now }], ["C", { price: 0.99, change: 3, time: now - 3_000_000 }], ["D", { price: 1.02, change: 4, time: now }]],
  ];
  for (const set of sets) {
    for (const [maxAge, tol] of [[600_000, 1], [120_000, 1], [3_600_000, 1.5]]) {
      cons.push({ args: { quotes: set, now, maxAge, tol }, output: liveConsensus(new Map(set), now, maxAge, tol) });
    }
  }
  write("live-consensus", cons);

  const open = [];
  for (let t = Date.UTC(2025, 0, 1, 0, 7); t < Date.UTC(2027, 0, 1); t += 7 * H + 13 * 60_000) open.push({ args: t, output: usMarketOpen(t) });
  for (const d of ["2026-03-06T14:30:00Z", "2026-03-09T13:30:00Z", "2026-03-09T13:29:59Z", "2026-11-02T14:29:00Z", "2026-11-02T14:30:00Z", "2026-11-02T20:59:59Z", "2026-11-02T21:00:00Z"]) {
    open.push({ args: Date.parse(d), output: usMarketOpen(Date.parse(d)) });
  }
  write("live-market", open);
}

void find;
void NOW;
