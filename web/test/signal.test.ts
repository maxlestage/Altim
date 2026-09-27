import { describe, expect, test } from "bun:test";
import fixture from "./swift-fixture.json";
import { analyze, rsi, type Candle } from "../src/engine/signal";

type Case = { candles: number[][]; score: number; confidence: number; action: string; factors: Record<string, number> };

describe("moteur TypeScript = moteur Swift", () => {
  (fixture as Case[]).forEach((c, i) => {
    test(`cas ${i} : score, confiance, action et facteurs identiques`, () => {
      const candles: Candle[] = c.candles.map(([time, open, high, low, close, volume]) => ({
        time: time!, open: open!, high: high!, low: low!, close: close!, volume: volume!,
      }));
      const s = analyze(candles)!;
      expect(s.score).toBeCloseTo(c.score, 9);
      expect(s.confidence).toBeCloseTo(c.confidence, 9);
      expect<string>(s.action).toBe(c.action);
      for (const f of s.factors) expect(f.score).toBeCloseTo(c.factors[f.name]!, 9);
      expect(s.factors.length).toBe(Object.keys(c.factors).length);
    });
  });
});

test("RSI référence StockCharts", () => {
  const closes = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.1, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61, 46.28, 46.28];
  // StockCharts affiche 70,53 avec des moyennes arrondies ; la valeur exacte est 70,464.
  expect(rsi(closes)[14]!).toBeCloseTo(70.464, 3);
});

test("historique insuffisant", () => {
  expect(analyze([])).toBeNull();
});
