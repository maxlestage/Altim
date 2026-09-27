import { describe, expect, test } from "bun:test";
import fixture from "./swift-fixture.json";
import { analyze, rsi, sanitize, type Candle } from "../src/engine/signal";
import { backtest } from "../src/engine/backtest";
import { positionSize, preTradeIssues, DEFAULT_RISK } from "../src/engine/risk";

type Case = {
  candles: number[][];
  higher?: number[][];
  score: number;
  confidence: number;
  action: string;
  factors: Record<string, number>;
  warnings: string[];
  stopLoss?: number;
  takeProfit?: number;
  backtest?: {
    totalReturnPercent: number; buyAndHoldPercent: number; winRatePercent: number; maxDrawdownPercent: number;
    exposurePercent: number;
    trades: { entryTime: number; exitTime: number; entryPrice: number; exitPrice: number; returnPercent: number; exitReason: string }[];
  };
};

const cases = fixture as unknown as Case[];

const toCandles = (rows: number[][]): Candle[] =>
  rows.map(([time, open, high, low, close, volume]) => ({ time: time!, open: open!, high: high!, low: low!, close: close!, volume: volume! }));

describe("moteur de signaux : 17 scénarios de référence", () => {
  cases.forEach((c, i) => {
    test(`cas ${i} : ${c.action}${c.higher ? " + UT supérieure" : ""}`, () => {
      const s = analyze(toCandles(c.candles), { higher: c.higher ? toCandles(c.higher) : undefined })!;
      expect(s.score).toBeCloseTo(c.score, 9);
      expect(s.confidence).toBeCloseTo(c.confidence, 9);
      expect<string>(s.action).toBe(c.action);
      expect(s.warnings).toEqual(c.warnings);
      expect(s.factors.length).toBe(Object.keys(c.factors).length);
      for (const f of s.factors) expect(f.score).toBeCloseTo(c.factors[f.name]!, 9);
      if (c.stopLoss != null) {
        expect(s.stopLoss).toBeCloseTo(c.stopLoss, 6);
        expect(s.takeProfit).toBeCloseTo(c.takeProfit!, 6);
      }
    });
  });

  cases.filter((c) => c.backtest).forEach((c, i) => {
    test(`backtest ${i} identique trade par trade`, () => {
      const r = backtest(toCandles(c.candles));
      const ref = c.backtest!;
      expect(r.trades.length).toBe(ref.trades.length);
      r.trades.forEach((t, j) => {
        const e = ref.trades[j]!;
        expect(t.entryTime).toBe(e.entryTime);
        expect(t.exitTime).toBe(e.exitTime);
        expect(t.entryPrice).toBeCloseTo(e.entryPrice, 8);
        expect(t.exitPrice).toBeCloseTo(e.exitPrice, 8);
        expect(t.returnPercent).toBeCloseTo(e.returnPercent, 8);
        expect(t.exitReason).toBe(e.exitReason);
      });
      expect(r.totalReturnPercent).toBeCloseTo(ref.totalReturnPercent, 8);
      expect(r.buyAndHoldPercent).toBeCloseTo(ref.buyAndHoldPercent, 8);
      expect(r.winRatePercent).toBeCloseTo(ref.winRatePercent, 8);
      expect(r.maxDrawdownPercent).toBeCloseTo(ref.maxDrawdownPercent, 8);
      expect(r.exposurePercent).toBeCloseTo(ref.exposurePercent, 8);
    });
  });
});

test("RSI référence StockCharts", () => {
  const closes = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.1, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61, 46.28, 46.28];
  // StockCharts affiche 70,53 avec des moyennes arrondies ; la valeur exacte est 70,464.
  expect(rsi(closes)[14]!).toBeCloseTo(70.464, 3);
});

test("historique insuffisant et bougies invalides", () => {
  expect(analyze([])).toBeNull();
  const c = toCandles(cases[0]!.candles);
  const withBad = [...c.slice(0, 50), { ...c[50]!, time: c[50]!.time + 1, high: 1, low: 2 }, ...c.slice(50)];
  expect(sanitize(withBad).length).toBe(c.length);
  expect(analyze(withBad)!.score).toBeCloseTo(analyze(c)!.score, 12);
});

test("données périmées signalées", () => {
  const c = toCandles(cases[0]!.candles);
  const s = analyze(c, { intervalMs: 3_600_000, now: c[c.length - 1]!.time + 86_400_000 })!;
  expect(s.warnings.some((w) => w.includes("périmées"))).toBe(true);
});

test("gestion du risque = RiskManager.swift", () => {
  const size = positionSize(DEFAULT_RISK, 10_000, { entry: 100, stopLoss: 95, takeProfit: 110 })!;
  expect(size.quantity).toBeCloseTo(100 / 5.2, 9);
  expect(size.capped).toBe(false);
  const capped = positionSize(DEFAULT_RISK, 10_000, { entry: 100, stopLoss: 99.9, takeProfit: 101 })!;
  expect(capped.capped).toBe(true);
  expect(capped.notional).toBeCloseTo(2000, 6);
  const good = { entry: 100, stopLoss: 95, takeProfit: 110 };
  expect(preTradeIssues(DEFAULT_RISK, "buy", 1000, 10_000, good, 0)).toEqual([]);
  expect(preTradeIssues(DEFAULT_RISK, "buy", 1000, 10_000, null, 0).length).toBe(1);
  expect(preTradeIssues(DEFAULT_RISK, "buy", 5000, 10_000, good, 0).length).toBe(1);
  expect(preTradeIssues(DEFAULT_RISK, "buy", 1000, 10_000, good, -400).length).toBe(1);
  expect(preTradeIssues(DEFAULT_RISK, "buy", 1000, 10_000, { entry: 100, stopLoss: 95, takeProfit: 102 }, 0).length).toBe(1);
});
