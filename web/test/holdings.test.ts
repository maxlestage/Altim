import { expect, test } from "bun:test";
import fixture from "./swift-fixture.json";
import { alignedReturns, analyzePortfolio, correlation, insightText, percentile, type Holding, type MarketInput } from "../src/engine/holdings";
import type { Candle } from "../src/engine/signal";

const DAY = 86_400_000;
const T0 = Date.UTC(2025, 0, 1);
/** Fixture series re-timed as daily candles (crypto: every day; stocks: skip weekends). */
function daily(caseIndex: number, stock = false): Candle[] {
  const rows = (fixture as unknown as { candles: number[][] }[])[caseIndex]!.candles;
  const out: Candle[] = [];
  let d = 0;
  for (const [, open, high, low, close, volume] of rows) {
    if (stock) while ([0, 6].includes(new Date(T0 + d * DAY).getUTCDay())) d++;
    out.push({ time: T0 + d * DAY + (stock ? 13.5 * 3_600_000 : 0), open: open!, high: high!, low: low!, close: close!, volume: volume! });
    d++;
  }
  return out;
}

const H = (id: string, symbol: string, kind: "crypto" | "stock", quantity: number, averagePrice: number): Holding => ({ id, symbol, kind, name: symbol, quantity, averagePrice });

export const CASES: { name: string; holdings: Holding[]; cash: number; market: Record<string, MarketInput> }[] = [
  {
    name: "diversifié",
    holdings: [H("1", "BTC", "crypto", 0.05, 60), H("2", "ETH", "crypto", 2, 110), H("3", "AAPL", "stock", 10, 90)],
    cash: 500,
    market: {
      "crypto:BTC": { price: 0, daily: daily(0), daySignal: { action: "buy", score: 30 }, shortSignal: { action: "buy", score: 28 }, reliability: "high" },
      "crypto:ETH": { price: 0, daily: daily(1), daySignal: { action: "sell", score: -30 }, shortSignal: { action: "sell", score: -40 }, reliability: "high" },
      "stock:AAPL": { price: 0, daily: daily(3, true), daySignal: { action: "hold", score: 10 }, shortSignal: { action: "hold", score: 5 }, reliability: "medium" },
    },
  },
  {
    name: "concentré et en gain",
    holdings: [H("1", "SOL", "crypto", 100, 40), H("2", "NVDA", "stock", 1, 80)],
    cash: 0,
    market: {
      "crypto:SOL": { price: 0, daily: daily(2), daySignal: { action: "hold", score: 5 }, shortSignal: { action: "strongSell", score: -55 }, reliability: "high" },
      "stock:NVDA": { price: 0, daily: daily(4, true), daySignal: null, shortSignal: null, reliability: "low" },
    },
  },
  {
    name: "une seule ligne",
    holdings: [H("1", "BTC", "crypto", 1, 200)],
    cash: 10_000,
    market: { "crypto:BTC": { price: 0, daily: daily(5), daySignal: { action: "strongBuy", score: 60 }, shortSignal: { action: "buy", score: 30 }, reliability: "high" } },
  },
  { name: "vide", holdings: [], cash: 1000, market: {} },
];
// Current price = last daily close.
for (const c of CASES) for (const m of Object.values(c.market)) m.price = m.daily[m.daily.length - 1]!.close;

test("statistiques de base", () => {
  expect(percentile([1, 2, 3, 4, 5], 0.05)).toBeCloseTo(1.2, 12);
  expect(correlation([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], [2, 4, 6, 8, 10, 12, 14, 16, 18, 20])).toBeCloseTo(1, 12);
  // Stocks closed at the weekend: price carried forward, zero return.
  const r = alignedReturns([daily(0), daily(3, true).slice(0, 180)], 90);
  expect(r[0]!.length).toBe(r[1]!.length);
  expect(r[1]!.filter((x) => x === 0).length).toBeGreaterThan(5);
});

test("portefeuille diversifié : valeurs, recommandations et risques cohérents", () => {
  const c = CASES[0]!;
  const a = analyzePortfolio(c.holdings, c.cash, c.market);
  const sum = a.lines.reduce((s, l) => s + l.value, 0) + a.cash;
  expect(a.total).toBeCloseTo(sum, 9);
  expect(a.allocation.crypto + a.allocation.stock + a.allocation.cash).toBeCloseTo(100, 9);
  expect(a.lines.reduce((s, l) => s + l.weight, 0) + a.allocation.cash).toBeCloseTo(100, 9);
  const eth = a.lines.find((l) => l.symbol === "ETH")!;
  expect(eth.recommendation).toBe("sell");
  expect(eth.stop!).toBeLessThan(eth.price!);
  expect(a.risk.volatilityAnnual!).toBeGreaterThan(0);
  expect(a.risk.var95Day!).toBeGreaterThan(0);
  expect(a.insights[0]!.level).toBe("danger");
  for (const i of a.insights) expect(insightText(i)).not.toBe(i.code);
});

test("ligne trop lourde → alléger jusqu'à 30 %, données non fiables → pas de conseil", () => {
  const c = CASES[1]!;
  const a = analyzePortfolio(c.holdings, c.cash, c.market);
  const sol = a.lines.find((l) => l.symbol === "SOL")!;
  expect(sol.recommendation).toBe("protect"); // strong 4h sell has priority over overweight
  expect(sol.reasons).toContain("overweight");
  expect(a.lines.find((l) => l.symbol === "NVDA")!.recommendation).toBe("unknown");
  const heavy = analyzePortfolio(c.holdings, c.cash, { ...c.market, "crypto:SOL": { ...c.market["crypto:SOL"]!, shortSignal: { action: "hold", score: 0 } } });
  const sol2 = heavy.lines.find((l) => l.symbol === "SOL")!;
  expect(sol2.recommendation).toBe("lighten");
  expect((sol2.value - sol2.trimValue) / heavy.total).toBeCloseTo(0.3, 9);
});

test("portefeuille vide", () => {
  const a = analyzePortfolio([], 1000, {});
  expect(a.total).toBe(1000);
  expect(a.insights.map((i) => i.code)).toEqual(["empty"]);
});

