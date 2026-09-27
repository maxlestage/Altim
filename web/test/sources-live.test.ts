/**
 * Real data, run only with `ALTIM_LIVE=1 bun test test/sources-live.test.ts` (daily "Santé des sources" workflow):
 * every source must still answer in the expected format, so that an API change is caught before it skews a signal.
 */
import { expect, test } from "bun:test";
import { SOURCES, STOCK_SOURCES, type Interval } from "../server/market";
import { QUOTE_SOURCES, makeAsset } from "../server/quotes";
import { FEEDS } from "../server/live";

const live = process.env.ALTIM_LIVE === "1";
const INTERVALS: Interval[] = ["1h", "4h", "1d"];
const T = 60_000;

const probe = async <T,>(label: string, run: () => Promise<T>) => {
  try {
    return { label, value: await run() };
  } catch (e) {
    return { label, error: e instanceof Error ? e.message : String(e) };
  }
};

/** HTTP 451: the exchange refuses this country (Binance from the US); that is not an API change. */
const geoBlocked = (e?: string) => !!e && /HTTP 451/.test(e);

function report(results: { label: string; error?: string }[]) {
  for (const r of results.filter((x) => geoBlocked(x.error))) console.warn(`~ ${r.label} : bloqué dans ce pays`);
  const failed = results.filter((r) => r.error && !geoBlocked(r.error));
  for (const r of failed) console.error(`✕ ${r.label} : ${r.error}`);
  console.log(`${results.length - failed.length}/${results.length} sources OK`);
  expect(failed.map((r) => r.label)).toEqual([]);
}

test.skipIf(!live)("bougies : chaque source crypto (BTC) et action (AAPL), chaque unité de temps", async () => {
  const jobs = [
    ...SOURCES.flatMap((s) => INTERVALS.filter((i) => s.supports?.(i) ?? true).map((i) => ({ s, i, base: "BTC" }))),
    ...STOCK_SOURCES.flatMap((s) => INTERVALS.filter((i) => s.supports?.(i) ?? true).map((i) => ({ s, i, base: "AAPL" }))),
  ];
  report(await Promise.all(jobs.map(({ s, i, base }) => probe(`${s.name} ${base} ${i}`, async () => {
    const c = await s.fetch(base, i);
    if (c.length < 20) throw new Error(`${c.length} bougies seulement`);
    const last = c[c.length - 1]!;
    if (!(last.close > 0) || !(last.high >= last.low)) throw new Error("bougie invalide");
    return c.length;
  }))));
}, T);

test.skipIf(!live)("cours : chaque source de cotation", async () => {
  const assets = [makeAsset("BTC", "crypto"), makeAsset("AAPL", "stock")];
  report(await Promise.all(QUOTE_SOURCES.map((s) => probe(s.name, async () => {
    const q = await s.fetch(assets.filter((a) => a.kind === s.kind));
    const sym = s.kind === "crypto" ? "BTC" : "AAPL";
    if (!(q.get(sym)?.price! > 0)) throw new Error("aucun cours");
    return q.get(sym)!.price;
  }))));
}, T);

test.skipIf(!live)("temps réel : chaque bourse WebSocket envoie un prix BTC", async () => {
  report(await Promise.all(FEEDS.map((f) => probe(f.name, () => new Promise<number>((resolve, reject) => {
    const ws = new WebSocket(f.url);
    const chanIds = new Map<number, string>();
    const timer = setTimeout(() => { ws.close(); reject(new Error("aucun prix en 15 s")); }, 15_000);
    ws.onopen = () => f.subscribe(["BTC"]).forEach((m) => ws.send(typeof m === "string" ? m : JSON.stringify(m)));
    ws.onerror = () => { clearTimeout(timer); reject(new Error("connexion impossible")); };
    ws.onmessage = (e) => {
      let m: unknown;
      try { m = JSON.parse(String(e.data)); } catch { return; }
      const p = f.parse(m, { chanIds }).find((x) => x.base === "BTC");
      if (p) { clearTimeout(timer); ws.close(); resolve(p.price); }
    };
  })))));
}, T);
