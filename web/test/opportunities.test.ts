import { expect, test } from "bun:test";
import { DEFAULT_FILTERS, compactUsd, countByCategory, filterItems, longShare, passes, type OppItem } from "../src/engine/opportunities";

const item = (symbol: string, over: Partial<OppItem> = {}): OppItem => ({
  symbol, name: symbol, sector: "Technologie", marketCap: 1e11, rank: null, price: 100, time: 0, change1d: 1, rsi14: 50,
  volumeRatio: 1, volatility: 2, liquidity: 5e8, distanceAtr: 0, hits: [{ category: "breakout", reason: "r", strength: 2 }], ...over,
});

test("filtres numériques : une valeur inconnue échoue à un filtre posé", () => {
  const a = item("A");
  expect(passes(a, DEFAULT_FILTERS)).toBe(true);
  expect(passes(a, { ...DEFAULT_FILTERS, minCap: 2e11 })).toBe(false);
  expect(passes(item("B", { marketCap: null }), { ...DEFAULT_FILTERS, minCap: 1e10 })).toBe(false);
  expect(passes(item("C", { rank: 12 }), { ...DEFAULT_FILTERS, maxRank: 20 })).toBe(true);
  expect(passes(item("C", { rank: 45 }), { ...DEFAULT_FILTERS, maxRank: 20 })).toBe(false);
  expect(passes(a, { ...DEFAULT_FILTERS, maxVolatility: 1.5 })).toBe(false);
  expect(passes(a, { ...DEFAULT_FILTERS, minLiquidity: 1e9 })).toBe(false);
});

test("catégories : seules les raisons choisies restent, les plus de raisons d'abord", () => {
  const items = [
    item("ONE"),
    item("TWO", { hits: [{ category: "volume", reason: "v", strength: 5 }, { category: "breakout", reason: "b", strength: 3 }] }),
    item("OVS", { hits: [{ category: "oversold", reason: "o", strength: 9 }] }),
  ];
  expect(filterItems(items, DEFAULT_FILTERS).map((i) => i.symbol)).toEqual(["TWO", "OVS", "ONE"]);
  const only = filterItems(items, { ...DEFAULT_FILTERS, categories: ["breakout"] });
  expect(only.map((i) => i.symbol)).toEqual(["TWO", "ONE"]);
  expect(only[0]!.hits.map((h) => h.category)).toEqual(["breakout"]);
  expect(filterItems(items, { ...DEFAULT_FILTERS, categories: [] })).toEqual([]);
  expect(countByCategory(items, DEFAULT_FILTERS)).toEqual({ setup: 0, reversal: 0, breakout: 2, volume: 1, oversold: 1, fundamentals: 0 });
  expect(countByCategory(items, { ...DEFAULT_FILTERS, maxVolatility: 1 }).breakout).toBe(0);
});

test("liquidations et montants", () => {
  const l = { longUsd: 14_389_008, shortUsd: 5_835_591, longCount: 1486, shortCount: 764, largest: null, from: 0, to: 0, hours: 24, complete: true, scope: "" };
  expect(Math.round(longShare(l)!)).toBe(71);
  expect(longShare({ ...l, longUsd: 0, shortUsd: 0 })).toBeNull();
  expect(compactUsd(14_389_008)).toBe("14,4 M$");
  expect(compactUsd(3_060_890_723)).toBe("3,1 Md$");
  expect(compactUsd(843_414)).toBe("843 k$");
  expect(compactUsd(420)).toBe("420 $");
});
