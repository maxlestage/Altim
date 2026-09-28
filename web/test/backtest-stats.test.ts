import { describe, expect, test } from "bun:test";
import { backtest, backtestWithRisk, regimeAt, regimeSplit, tradeStats, type BacktestTrade } from "../src/engine/backtest";
import type { Candle } from "../src/engine/signal";

const DAY = 86_400_000;
const T0 = Date.UTC(2023, 0, 1);
const series = (closes: number[]): Candle[] => closes.map((c, i) => ({ time: T0 + i * DAY, open: c, high: c, low: c, close: c, volume: 1 }));

describe("regimes (only the candles up to the bar)", () => {
  const up = Array.from({ length: 300 }, (_, i) => 100 + i);
  test("steady rise: bull; fewer than 220 candles: unknown", () => {
    const c = series(up);
    expect(regimeAt(c, 299)).toBe("bull");
    expect(regimeAt(c, 218)).toBe("unknown");
    expect(regimeAt(c, 219)).toBe("bull");
  });
  test("steady fall: bear until the drawdown from the 1-year high passes 30 %, then crisis", () => {
    const down = Array.from({ length: 300 }, (_, i) => 1000 - i);
    expect(regimeAt(series(down), 250)).toBe("bear");
    const crash = [...up.slice(0, 260), ...Array.from({ length: 40 }, (_, i) => 350 - i * 5)];
    expect(regimeAt(series(crash), 299)).toBe("crisis");
  });
  test("no look-ahead: the regime of a bar does not change with the candles after it", () => {
    const crash = [...up.slice(0, 260), ...Array.from({ length: 40 }, () => 10)];
    expect(regimeAt(series(crash), 250)).toBe(regimeAt(series(up), 250));
  });
  test("flat: range", () => {
    expect(regimeAt(series(Array.from({ length: 260 }, () => 100)), 259)).toBe("range");
  });
});

describe("trade statistics", () => {
  test("expectancy = win rate × average win − loss rate × |average loss|; R = return ÷ risk", () => {
    // Wins 2 and 3.5 (avg 2.75), losses −1 and 0 (avg −0.5): 0.5 × 2.75 − 0.5 × 0.5
    const s = tradeStats([2, -1, 0, 3.5], [1, 0, 2]);
    expect(s.expectancy).toBeCloseTo(1.125, 12);
    // Only the trades with a positive risk: 2 ÷ 1 and 0 ÷ 2
    expect(s.avgR).toBeCloseTo(1, 12);
    expect(tradeStats([], [])).toEqual({ expectancy: null, avgR: null });
  });
  test("split by regime of the signal candle, small samples marked", () => {
    const c = series(Array.from({ length: 300 }, (_, i) => 100 + i));
    const t = (k: number): BacktestTrade => ({ entryTime: c[k]!.time, exitTime: c[k + 1]!.time, entryPrice: 1, exitPrice: 1, returnPercent: 0, exitReason: "Stop" });
    const split = regimeSplit(c, [t(250), t(260), t(100)], [1, -1, 5]);
    expect(split.map((g) => [g.regime, g.trades, g.lowSample])).toEqual([["bull", 2, true], ["bear", 0, true], ["range", 0, true], ["crisis", 0, true], ["unknown", 1, true]]);
    expect(split[0]!.winRate).toBe(50);
    expect(split[0]!.avgReturn).toBe(0);
  });
  test("the risks come with the unchanged backtest, one per trade", () => {
    const closes = Array.from({ length: 400 }, (_, i) => 100 + 20 * Math.sin(i / 15) + i * 0.1);
    const c = closes.map((x, i) => ({ time: T0 + i * DAY, open: x, high: x * 1.02, low: x * 0.98, close: x, volume: 1000 + (i % 7) * 100 }));
    const { result, risks } = backtestWithRisk(c);
    expect(result).toEqual(backtest(c));
    expect(risks.length).toBe(result.trades.length);
    expect(risks.every((r) => r > 0)).toBe(true);
  });
});
