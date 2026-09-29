import { expect, test } from "bun:test";
import { ETF_BLOCK_LABEL, sectorExposure, sectorInsightText, UNKNOWN_LABEL, type SectorItem } from "../src/engine/sectors";

const item = (symbol: string, sector: string | null, classification: SectorItem["classification"], reason: string | null = null): SectorItem => ({
  symbol, sector, classification, source: classification ? "test" : null, reason, etf: classification === "etf", sec: null, nasdaq: null,
});
const SECTORS: Record<string, SectorItem> = {
  AAPL: item("AAPL", "Technologie", "nasdaq"),
  NVDA: item("NVDA", "Technologie", "nasdaq"),
  JPM: item("JPM", "Finance", "nasdaq"),
  "BRK-B": item("BRK-B", "Finance et immobilier", "sec"),
  SPY: item("SPY", "ETF / fonds indiciel (plusieurs secteurs)", "etf"),
  ABCD: item("ABCD", null, null, "Secteur non couvert : aucun dépôt à la SEC"),
};

test("blocks: sectors, ETF, crypto, cash and unknown, in % of the portfolio, heaviest first", () => {
  const e = sectorExposure(
    [
      { symbol: "AAPL", kind: "stock", value: 3000 },
      { symbol: "NVDA", kind: "stock", value: 1000 },
      { symbol: "JPM", kind: "stock", value: 1000 },
      { symbol: "SPY", kind: "stock", value: 1000 },
      { symbol: "ABCD", kind: "stock", value: 500 },
      { symbol: "BTC", kind: "crypto", value: 2000 },
      { symbol: "ETH", kind: "crypto", value: 500 },
    ],
    1000,
    SECTORS,
  );
  expect(e.total).toBe(10000);
  expect(e.stockValue).toBe(6500);
  expect(e.blocks.map((b) => [b.label, b.weight])).toEqual([
    ["Technologie", 40],
    ["Crypto", 25],
    [ETF_BLOCK_LABEL, 10],
    ["Finance", 10],
    ["Liquidités", 10],
    [UNKNOWN_LABEL, 5],
  ]);
  const tech = e.blocks[0]!;
  expect(tech.stockWeight).toBeCloseTo((4000 / 6500) * 100, 9);
  expect(tech.symbols).toEqual(["AAPL", "NVDA"]);
  expect(e.blocks.find((b) => b.kind === "crypto")!.stockWeight).toBeNull();
  // Weights add up to 100 %.
  expect(e.blocks.reduce((a, b) => a + b.weight, 0)).toBeCloseTo(100, 9);
  // Effective sectors over Technologie 4000 / Finance 1000: 1 / (0.8² + 0.2²).
  expect(e.effectiveSectors).toBeCloseTo(1 / 0.68, 9);
  expect(e.classifiedShare).toBeCloseTo((5000 / 6500) * 100, 9);
  expect(e.bySource).toEqual({ nasdaq: 3, sec: 0, etf: 1 });
  expect(e.unknown).toEqual([{ symbol: "ABCD", reason: "Secteur non couvert : aucun dépôt à la SEC" }]);
  const codes = e.insights.map((i) => i.code);
  expect(codes).toEqual(["sector_heavy", "sector_effective", "sector_etf", "sector_unknown"]);
  expect(sectorInsightText(e.insights[0]!)).toBe("Technologie pèse 40 % de votre patrimoine : une mauvaise passe de ce secteur pourrait toucher plusieurs lignes à la fois.");
});

test("concentration of the stock part only (from two stock lines), SEC and Nasdaq never merged", () => {
  const e = sectorExposure(
    [
      { symbol: "AAPL", kind: "stock", value: 2000 },
      { symbol: "JPM", kind: "stock", value: 1000 },
      { symbol: "BRK-B", kind: "stock", value: 500 },
      { symbol: "BTC", kind: "crypto", value: 6500 },
    ],
    0,
    SECTORS,
  );
  expect(e.blocks.map((b) => b.label)).toEqual(["Crypto", "Technologie", "Finance", "Finance et immobilier (SIC)"]);
  expect(e.insights[0]).toEqual({ level: "warning", code: "sector_heavy_stocks", values: { sector: "Technologie", share: (2000 / 3500) * 100 } });
  expect(e.effectiveSectors).toBeCloseTo(1 / ((2 / 3.5) ** 2 + (1 / 3.5) ** 2 + (0.5 / 3.5) ** 2), 9);
  // One stock line only: 100 % of the stock part is not flagged (the line concentration is, elsewhere).
  const one = sectorExposure([{ symbol: "AAPL", kind: "stock", value: 1000 }, { symbol: "BTC", kind: "crypto", value: 9000 }], 0, SECTORS);
  expect(one.insights.map((i) => i.code)).toEqual([]);
});

test("server unreachable: every stock is unknown with the reason, nothing invented", () => {
  const e = sectorExposure([{ symbol: "AAPL", kind: "stock", value: 1000 }, { symbol: "BTC", kind: "crypto", value: 1000 }], 0, null, "HTTP 502");
  expect(e.blocks.map((b) => [b.kind, b.weight])).toEqual([["crypto", 50], ["unknown", 50]]);
  expect(e.effectiveSectors).toBeNull();
  expect(e.classifiedShare).toBe(0);
  expect(e.unknown).toEqual([{ symbol: "AAPL", reason: "HTTP 502" }]);
  // Empty portfolio: no block, no division by zero.
  const empty = sectorExposure([], 0, {});
  expect([empty.total, empty.blocks.length, empty.effectiveSectors]).toEqual([0, 0, null]);
});
