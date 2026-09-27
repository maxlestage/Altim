import { expect, test } from "bun:test";
import type { Server } from "node:http";
import { createApp } from "../server/app";
import { FEEDS, LiveHub, liveConsensus, usMarketOpen, type Tick } from "../server/live";
import type { Asset, SourceQuote } from "../server/quotes";
import samples from "./live-samples.json";

// Messages captured on the real streams (BTC), see test/live-samples.json.
const feed = (name: string) => FEEDS.find((f) => f.name === name)!;
const parseAll = (name: string) => {
  const ctx = { chanIds: new Map<number, string>() };
  return ((samples as Record<string, unknown[]>)[name] ?? []).flatMap((m) => feed(name).parse(m, ctx));
};

test("chaque flux temps réel est décodé sur un vrai message", () => {
  expect(FEEDS.map((f) => f.name)).toEqual(["OKX", "Coinbase", "Kraken", "Bitfinex", "Bitget", "Gate.io", "Crypto.com"]);
  const expected: Record<string, [number, number]> = {
    OKX: [84595.2, (84595.2 / 84060.1 - 1) * 100],
    Coinbase: [84581.99, (84581.99 / 84146.86 - 1) * 100],
    Kraken: [84576, 0.63],
    Bitfinex: [84529, 0.418166],
    Bitget: [84608.29, (84608.29 / 84161.83 - 1) * 100],
    "Gate.io": [84596.4, 0.6384],
    "Crypto.com": [84600.03, 0.64],
  };
  for (const [name, [price, change]] of Object.entries(expected)) {
    const [p] = parseAll(name);
    expect(p?.base).toBe("BTC");
    expect(p?.price).toBe(price);
    expect(p?.change).toBeCloseTo(change, 6);
  }
});

test("flux : messages sans prix ignorés, symboles longs, abonnements", () => {
  for (const f of FEEDS) {
    expect(f.parse({ event: "subscribe" }, { chanIds: new Map() })).toEqual([]);
    expect(f.parse([1, "hb"], { chanIds: new Map() })).toEqual([]);
    expect(f.subscribe(["BTC", "PEPE"]).length).toBeGreaterThan(0);
  }
  // Bitfinex: 4+ letter symbols use the "t{BASE}:USD" form, and the channel id is remembered.
  expect(feed("Bitfinex").subscribe(["DOGE"])).toEqual([{ event: "subscribe", channel: "ticker", symbol: "tDOGE:USD" }]);
  const ctx = { chanIds: new Map<number, string>() };
  feed("Bitfinex").parse({ event: "subscribed", channel: "ticker", chanId: 7, symbol: "tDOGE:USD" }, ctx);
  expect(ctx.chanIds.get(7)).toBe("DOGE");
  // Crypto.com heartbeats must be answered, or the exchange closes the connection.
  expect(feed("Crypto.com").reply!({ id: 42, method: "public/heartbeat" })).toEqual({ id: 42, method: "public/respond-heartbeat" });
  expect(feed("Crypto.com").reply!({ method: "subscribe" })).toBeNull();
});

test("consensus temps réel : médiane, source aberrante et cotations périmées écartées", () => {
  const now = 1_000_000;
  const q = new Map([
    ["A", { price: 100, change: 1, time: now - 1000 }],
    ["B", { price: 100.4, change: 2, time: now - 500 }],
    ["C", { price: 99.8, change: null, time: now }],
    ["D", { price: 110, change: 9, time: now }], // 10 % away: ignored
    ["E", { price: 50, change: 0, time: now - 700_000 }], // stale: ignored
  ]);
  const c = liveConsensus(q, now)!;
  expect(c.price).toBe(100);
  expect(c.change).toBe(1.5);
  expect(c.agreeing).toBe(3);
  expect(c.total).toBe(4);
  expect(c.sources.sort()).toEqual(["A", "B", "C"]);
  expect(c.time).toBe(now);
  expect(liveConsensus(new Map(), now)).toBeNull();
});

test("séance américaine : ouverte 9 h 30 – 16 h New York en semaine, heure d'été comme d'hiver", () => {
  expect(usMarketOpen(Date.parse("2026-09-28T14:00:00Z"))).toBe(true); // Monday 10:00 EDT
  expect(usMarketOpen(Date.parse("2026-09-28T13:29:00Z"))).toBe(false); // 9:29 EDT
  expect(usMarketOpen(Date.parse("2026-09-28T19:59:00Z"))).toBe(true); // 15:59 EDT
  expect(usMarketOpen(Date.parse("2026-09-28T20:00:00Z"))).toBe(false); // 16:00 EDT
  expect(usMarketOpen(Date.parse("2026-09-27T15:00:00Z"))).toBe(false); // Sunday
  expect(usMarketOpen(Date.parse("2026-12-01T14:31:00Z"))).toBe(true); // Tuesday 9:31 EST
  expect(usMarketOpen(Date.parse("2026-12-01T14:29:00Z"))).toBe(false);
});

/** REST source whose prices the test sets. */
function fakeSource(name: string, kind: "crypto" | "stock", prices: Record<string, number>) {
  const calls: string[][] = [];
  return {
    calls,
    source: {
      name,
      kind,
      fetch: async (assets: Asset[]) => {
        calls.push(assets.map((a) => a.symbol));
        return new Map<string, SourceQuote>(assets.filter((a) => prices[a.symbol]).map((a) => [a.symbol, { price: prices[a.symbol]!, change: 1 }]));
      },
    },
  };
}
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

test("hub : abonnement, dernier prix, 4 envois par seconde au plus, rien d'envoyé sans changement", async () => {
  const hub = new LiveHub([], [], 60_000);
  const ticks: Tick[] = [];
  const off = hub.subscribe(["crypto:BTC"], (t) => ticks.push(t));
  for (let i = 0; i < 20; i++) hub.onQuote("crypto:BTC", "OKX", { price: 100 + i, change: 1 });
  await wait(50);
  expect(ticks.length).toBe(1);
  expect(ticks[0]!.price).toBe(119); // latest state, not the first one
  hub.onQuote("crypto:BTC", "OKX", { price: 119, change: 1 });
  await wait(300);
  expect(ticks.length).toBe(1); // same price: nothing sent
  hub.onQuote("crypto:BTC", "OKX", { price: 120, change: 1 });
  await wait(300);
  expect(ticks.length).toBe(2);
  expect(hub.snapshot(["crypto:BTC", "crypto:ETH"]).map((t) => t.price)).toEqual([120]);
  expect(hub.watched).toBe(1);
  off();
  expect(hub.watched).toBe(0);
  hub.onQuote("crypto:BTC", "OKX", { price: 121 });
  await wait(300);
  expect(ticks.length).toBe(2);
  hub.close();
});

test("hub : actions interrogées en continu, séance indiquée ; crypto sans flux WebSocket relayée par REST", async () => {
  const stock = fakeSource("Robinhood", "stock", { AAPL: 200 });
  const crypto = fakeSource("Binance", "crypto", { OBSCURE: 0.5, BTC: 100 });
  const hub = new LiveHub([], [stock.source, crypto.source], 60_000);
  const ticks: Tick[] = [];
  const off = hub.subscribe(["stock:AAPL", "crypto:OBSCURE"], (t) => ticks.push(t));
  await wait(30);
  expect(ticks.find((t) => t.symbol === "AAPL")).toMatchObject({ kind: "stock", price: 200, market: expect.stringMatching(/open|closed/), sources: ["Robinhood"] });
  expect(ticks.find((t) => t.symbol === "OBSCURE")).toMatchObject({ kind: "crypto", price: 0.5 });
  expect(stock.calls).toEqual([["AAPL"]]);
  expect(crypto.calls).toEqual([["OBSCURE"]]);
  // A crypto with a live WebSocket quote is not polled.
  hub.subscribe(["crypto:BTC"], () => {});
  await wait(30);
  hub.onQuote("crypto:BTC", "OKX", { price: 100 });
  await hub.poll();
  expect(crypto.calls.at(-1)).toEqual(["OBSCURE"]);
  off();
  hub.close();
});

test("/api/live : flux SSE non compressé, dernier prix d'abord puis chaque changement", async () => {
  const hub = new LiveHub([], [], 60_000);
  hub.subscribe(["crypto:ETH"], () => {});
  hub.onQuote("crypto:ETH", "OKX", { price: 3000, change: 2 });
  await wait(20);
  const server: Server = createApp({ live: hub }).listen(0);
  await new Promise((r) => server.once("listening", r));
  const addr = server.address();
  const base = `http://127.0.0.1:${typeof addr === "object" && addr ? addr.port : 0}`;
  try {
    expect((await fetch(`${base}/api/live?symbols=../x:crypto`)).status).toBe(400);
    const ctrl = new AbortController();
    const r = await fetch(`${base}/api/live?symbols=ETH:crypto,SOL:crypto`, { signal: ctrl.signal, headers: { "Accept-Encoding": "gzip" } });
    expect(r.headers.get("content-type")).toContain("text/event-stream");
    expect(r.headers.get("content-encoding")).toBeNull();
    const reader = r.body!.getReader();
    const events: Tick[] = [];
    let buf = "";
    const read = async () => {
      while (true) {
        const { value, done } = await reader.read();
        if (done) return;
        buf += new TextDecoder().decode(value);
        for (const m of buf.matchAll(/^data: (.*)$/gm)) events.push(JSON.parse(m[1]!));
        buf = buf.slice(buf.lastIndexOf("\n\n") + 2);
        if (events.length >= 2) return;
      }
    };
    const done = read();
    await wait(100);
    hub.onQuote("crypto:ETH", "OKX", { price: 3010, change: 2.3 });
    await Promise.race([done, wait(2000)]);
    expect(events.map((e) => e.price)).toEqual([3000, 3010]);
    expect(events[1]).toMatchObject({ symbol: "ETH", kind: "crypto", agreeing: 1, total: 1 });
    expect(hub.watched).toBe(2);
    ctrl.abort();
    await wait(100);
    // The browser left: SOL is no longer watched (ETH keeps the test's own subscription).
    expect(hub.watched).toBe(1);
  } finally {
    server.close();
    hub.close();
  }
});

test("/api/live : 8 flux au plus par adresse, la place se libère à la fermeture", async () => {
  const hub = new LiveHub([], [], 60_000);
  const server: Server = createApp({ live: hub }).listen(0);
  await new Promise((r) => server.once("listening", r));
  const addr = server.address();
  const base = `http://127.0.0.1:${typeof addr === "object" && addr ? addr.port : 0}`;
  const ctrls: AbortController[] = [];
  try {
    for (let i = 0; i < 8; i++) {
      const c = new AbortController();
      ctrls.push(c);
      expect((await fetch(`${base}/api/live?symbols=ETH:crypto`, { signal: c.signal })).status).toBe(200);
    }
    const refused = await fetch(`${base}/api/live?symbols=ETH:crypto`);
    expect(refused.status).toBe(429);
    ctrls.pop()!.abort();
    await wait(150);
    const c = new AbortController();
    ctrls.push(c);
    expect((await fetch(`${base}/api/live?symbols=ETH:crypto`, { signal: c.signal })).status).toBe(200);
  } finally {
    ctrls.forEach((c) => c.abort());
    server.close();
    hub.close();
  }
});
