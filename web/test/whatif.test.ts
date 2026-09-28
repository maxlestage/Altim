import { describe, expect, test } from "bun:test";
import { analyzePortfolio, type Holding, type MarketInput } from "../src/engine/holdings";
import { factorBeta, MIN_BETA_DAYS, whatIf } from "../src/engine/portfolio-risk";
import type { Candle } from "../src/engine/signal";

const DAY = 86_400_000;
const T0 = Date.UTC(2026, 0, 5); // a Monday
const series = (closes: number[], times: number[]): Candle[] => closes.map((c, i) => ({ time: times[i]!, open: c, high: c, low: c, close: c, volume: 1 }));
function returns(n: number, seed: number): number[] {
  let x = seed;
  return Array.from({ length: n }, () => {
    x = (x * 16807) % 2147483647;
    return (x / 2147483647 - 0.5) * 0.04;
  });
}
const path = (r: number[]) => r.reduce<number[]>((a, x) => [...a, a[a.length - 1]! * (1 + x)], [100]);
const everyDay = (n: number) => Array.from({ length: n }, (_, i) => T0 + i * DAY);
const weekdays = (n: number) => everyDay(Math.ceil(n * 1.5)).filter((t) => ![0, 6].includes(new Date(t).getUTCDay())).slice(0, n);

describe("factor beta", () => {
  test("measured only on the days both have a close: a crypto's weekends do not dilute its beta to QQQ", () => {
    const days = weekdays(121);
    const q = returns(120, 11);
    const factor = series(path(q), days);
    // The crypto moves 1.5 × QQQ on weekdays and has extra weekend candles with big moves.
    const cryptoTimes = everyDay(Math.ceil(121 * 1.5));
    const byDay = new Map(days.map((t, i) => [t, path(q.map((x) => 1.5 * x))[i]!]));
    let last = 100;
    const crypto = series(cryptoTimes.map((t) => (byDay.has(t) ? (last = byDay.get(t)!) : last * 1.03)), cryptoTimes);
    const b = factorBeta(crypto, factor);
    expect(b.estimated).toBe(true);
    expect(b.days).toBe(90);
    expect(b.beta).toBeCloseTo(1.5, 6);
    expect(b.correlation).toBeCloseTo(1, 6);
  });

  test("under 30 shared days: not estimated", () => {
    const t = everyDay(20);
    const b = factorBeta(series(path(returns(19, 3)), t), series(path(returns(19, 5)), t));
    expect(b.estimated).toBe(false);
    expect(b.days).toBe(19);
    expect(MIN_BETA_DAYS).toBe(30);
  });
});

describe("what if", () => {
  const H = (id: string, symbol: string, kind: "crypto" | "stock", quantity: number): Holding => ({ id, symbol, kind, name: symbol, quantity, averagePrice: 100 });
  const M = (price: number): MarketInput => ({ price, daily: [], daySignal: null, shortSignal: null, reliability: "high" });
  const a = analyzePortfolio(
    [H("1", "NVDA", "stock", 30), H("2", "NVDA", "stock", 10), H("3", "QQQ", "stock", 20), H("4", "DOGE", "crypto", 20)],
    2_000,
    { "stock:NVDA": M(100), "stock:QQQ": M(100), "crypto:DOGE": M(100) },
  );

  test("each line moves by its beta; the factor itself is beta 1; a line without beta is not covered, never guessed", () => {
    const r = whatIf(a, "qqq", -10, { "stock:NVDA": { beta: 2, days: 90, estimated: true, correlation: 0.8 }, "crypto:DOGE": { beta: 1, days: 12, estimated: false, correlation: null } });
    expect(r.base).toBe(10_000);
    const nvda = r.lines.find((l) => l.symbol === "NVDA")!;
    expect(nvda).toMatchObject({ value: 4_000, movePercent: -20, loss: 800, beta: 2 });
    expect(r.lines.find((l) => l.symbol === "QQQ")).toMatchObject({ reference: true, loss: 200 });
    expect(r.lines.find((l) => l.symbol === "DOGE")).toMatchObject({ loss: null, movePercent: null });
    expect(r.loss).toBe(1_000);
    expect(r.lossPercent).toBe(10);
    expect(r.uncovered).toEqual(["DOGE"]);
    expect(r.uncoveredValue).toBe(2_000);
    expect(r.worst!.symbol).toBe("NVDA");
    expect([r.minDays, r.maxDays]).toEqual([90, 90]);
  });

  test("an amount is spread on the current weights (cash included); a move never goes below −100 %", () => {
    const r = whatIf(a, "qqq", -50, { "stock:NVDA": { beta: 3, days: 60, estimated: true, correlation: 0.9 } }, 1_000);
    expect(r.scaled).toBe(true);
    expect(r.cash).toBe(200);
    expect(r.lines.find((l) => l.symbol === "NVDA")).toMatchObject({ value: 400, movePercent: -100, loss: 400 });
    expect(r.loss).toBe(500);
    expect(r.lossPercent).toBe(50);
  });

  test("a negative beta gains in the fall; the worst line is the biggest loss only", () => {
    const r = whatIf(a, "spy", -10, { "stock:NVDA": { beta: -0.5, days: 90, estimated: true, correlation: -0.3 }, "stock:QQQ": { beta: 1.2, days: 90, estimated: true, correlation: 0.95 } });
    expect(r.lines.find((l) => l.symbol === "NVDA")!.loss).toBe(-200);
    expect(r.worst!.symbol).toBe("QQQ");
  });
});
