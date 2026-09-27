import { expect, test } from "bun:test";
import { parseExtra, EXTRA_CANDLE_SOURCES, EXTRA_QUOTE_SOURCES } from "../server/stocks-extra";
import { STOCK_SOURCES, nyOpen } from "../server/market";
import { QUOTE_SOURCES } from "../server/quotes";
import s from "./stocks-extra-samples.json";

// Real responses (AAPL, 24–25/09/2026), see stocks-extra-samples.json.
const d24 = nyOpen(2026, 9, 24), d25 = nyOpen(2026, 9, 25);

test("bougies journalières des nouvelles sources : séance de New York, du plus ancien au plus récent", () => {
  for (const [name, candles] of [
    ["WSJ", parseExtra.wsj(s.wsj)],
    ["AlphaQuery", parseExtra.alphaquery(s.alphaquery)],
    ["Finviz", parseExtra.finviz(s.finviz)],
    ["FT", parseExtra.ft(s.ft)],
    ["eToro", parseExtra.etoro(s.etoro)],
  ] as const) {
    expect(candles.map((c) => c.time), name).toEqual([d24, d25]);
    expect(candles[1]!.high, name).toBeGreaterThanOrEqual(candles[1]!.low);
  }
  expect(parseExtra.wsj(s.wsj)[1]).toMatchObject({ open: 336.04, high: 341.67, low: 334.53, close: 341.07 });
  expect(parseExtra.alphaquery(s.alphaquery)[1]!.close).toBe(341.07);
  expect(parseExtra.finviz(s.finviz)[1]!.close).toBeCloseTo(341.07, 2);
  expect(parseExtra.ft(s.ft)[1]!.close).toBe(341.07);
  // eToro quotes its own prices: close to the market, not identical.
  expect(Math.abs(parseExtra.etoro(s.etoro)[1]!.close / 341.07 - 1)).toBeLessThan(0.005);
});

test("identifiants : listing américain principal au FT, actions et ETF chez eToro", () => {
  expect(parseExtra.ftXid(s.ftSearch, "AAPL")).toBe("36276");
  expect(parseExtra.ftXid(s.ftSearch, "MSFT")).toBeNull();
  const ids = parseExtra.etoroIds({ InstrumentDisplayDatas: [
    { InstrumentID: 1, SymbolFull: "EURUSD", InstrumentTypeID: 1 },
    { InstrumentID: 1001, SymbolFull: "AAPL", InstrumentTypeID: 5 },
    { InstrumentID: 3000, SymbolFull: "SPY", InstrumentTypeID: 6 },
    { InstrumentID: 1118, SymbolFull: "BRK.B", InstrumentTypeID: 5 },
  ] });
  expect([...ids.keys()]).toEqual(["AAPL", "SPY", "BRK.B"]);
});

test("cours : Fidelity (plusieurs symboles, BRK/B), StockCharts, TipRanks, Public.com", () => {
  const f = parseExtra.fidelity(s.fidelity);
  expect(f.get("AAPL")).toEqual({ price: 341.07, change: 1.53 });
  expect(f.get("BRK-B")?.price).toBe(505.48);
  expect(f.has("XXXX")).toBe(false);
  expect(parseExtra.stockcharts(s.stockcharts).price).toBe(341.07);
  expect(parseExtra.stockcharts(s.stockcharts).change).toBeCloseTo((341.07 / 335.92 - 1) * 100, 6);
  expect(parseExtra.tipranks(s.tipranks)).toEqual({ price: 341.07, change: (341.07 / 335.92 - 1) * 100 });
  expect(parseExtra.publicCom(s.publicCom, "AAPL").price).toBe(341.07);
  expect(() => parseExtra.stockcharts({})).toThrow();
  expect(() => parseExtra.tipranks({ prices: [] })).toThrow();
  expect(() => parseExtra.publicCom("<html></html>", "AAPL")).toThrow();
});

test("17 fournisseurs indépendants pour les actions", () => {
  expect(EXTRA_CANDLE_SOURCES.every((x) => STOCK_SOURCES.some((y) => y.name === x.name))).toBe(true);
  expect(EXTRA_QUOTE_SOURCES.every((x) => QUOTE_SOURCES.some((y) => y.name === x.name && y.kind === "stock"))).toBe(true);
  const providers = new Set([...STOCK_SOURCES.map((x) => x.name), ...QUOTE_SOURCES.filter((q) => q.kind === "stock").map((q) => q.name)]);
  expect(providers.size).toBe(17);
});
