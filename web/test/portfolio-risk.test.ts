import { describe, expect, test } from "bun:test";
import { analyzePortfolio, type Holding, type MarketInput } from "../src/engine/holdings";
import {
  checkLimits, correlatedClusters, dailyChange, dangerousPositions, DAILY_LOSS_REACHED, estimateBeta, previousClose, STRESS_SCENARIOS, stressTest,
} from "../src/engine/portfolio-risk";
import { DEFAULT_RISK } from "../src/engine/risk";
import type { Candle } from "../src/engine/signal";

const DAY = 86_400_000;
const T0 = Date.UTC(2026, 0, 1);
/** Daily candles from closes (high/low ± 1 %). */
const series = (closes: number[], start = T0): Candle[] =>
  closes.map((c, i) => ({ time: start + i * DAY, open: c, high: c * 1.01, low: c * 0.99, close: c, volume: 1 }));
/** Deterministic pseudo-random daily returns. */
function returns(n: number, seed: number): number[] {
  let x = seed;
  return Array.from({ length: n }, () => {
    x = (x * 16807) % 2147483647;
    return (x / 2147483647 - 0.5) * 0.06;
  });
}
const path = (r: number[], start = 100) => r.reduce<number[]>((a, x) => [...a, a[a.length - 1]! * (1 + x)], [start]);
const H = (id: string, symbol: string, kind: "crypto" | "stock", quantity: number, averagePrice: number, stop?: number): Holding => ({ id, symbol, kind, name: symbol, quantity, averagePrice, stop });
const M = (price: number, daily: Candle[]): MarketInput => ({ price, daily, daySignal: { action: "hold", score: 0 }, shortSignal: { action: "hold", score: 0 }, reliability: "high" });

describe("beta", () => {
  const bench = returns(120, 7);
  test("an asset moving twice as much as its benchmark has a beta of 2, measured on the shared days", () => {
    const b = estimateBeta(series(path(bench.map((x) => 2 * x))), series(path(bench)));
    expect(b.estimated).toBe(true);
    expect(b.days).toBe(90);
    expect(b.beta).toBeCloseTo(2, 6);
  });
  test("too short a history falls back to 1, said as not estimated", () => {
    // A recent listing: 20 days shared with the benchmark.
    const b = estimateBeta(series(path(bench.slice(0, 20)), T0 + 100 * DAY), series(path(bench)));
    expect(b).toEqual({ beta: 1, days: 0, estimated: false });
  });
});

describe("stress scenarios", () => {
  const holdings = [H("1", "ETH", "crypto", 1, 1000), H("2", "AAPL", "stock", 10, 100)];
  const a = analyzePortfolio(holdings, 1000, { "crypto:ETH": M(1000, []), "stock:AAPL": M(100, []) });
  const r = stressTest(a, { "crypto:ETH": { beta: 1.5, days: 90, estimated: true } });
  test("each line moves by beta × its benchmark's shock; cash does not move", () => {
    const all10 = r.find((x) => x.scenario.key === "all10")!;
    // ETH 1000 × 1.5 × 10 % + AAPL 1000 × 1 × 10 % (no beta: 1)
    expect(all10.loss).toBeCloseTo(250, 9);
    expect(all10.lossPercent).toBeCloseTo((250 / 3000) * 100, 9);
    expect(all10.investedLossPercent).toBeCloseTo((250 / 2000) * 100, 9);
    expect(all10.worst?.symbol).toBe("ETH");
  });
  test("mixed scenarios: crypto only, then stocks −10 % with crypto −30 %", () => {
    expect(r.find((x) => x.scenario.key === "crypto20")!.loss).toBeCloseTo(300, 9);
    expect(r.find((x) => x.scenario.key === "stock10crypto30")!.loss).toBeCloseTo(450 + 100, 9);
    expect(r.map((x) => x.scenario.key)).toEqual(STRESS_SCENARIOS.map((s) => s.key));
  });
  test("a line never loses more than its value", () => {
    const big = stressTest(a, { "crypto:ETH": { beta: 5, days: 90, estimated: true } }).find((x) => x.scenario.key === "all30")!;
    expect(big.loss).toBeCloseTo(1000 + 300, 9);
  });
});

describe("limits", () => {
  const bench = returns(120, 11);
  const eth = series(path(bench));
  const sol = series(path(bench.map((x, i) => x * 1.2 + (i % 2 ? 0.001 : -0.001))));
  const aapl = series(path(returns(120, 99)));
  test("a correlated cluster is found and weighed", () => {
    const c = correlatedClusters([{ symbol: "ETH", weight: 30, daily: eth }, { symbol: "SOL", weight: 25, daily: sol }, { symbol: "AAPL", weight: 20, daily: aapl }]);
    expect(c.length).toBe(1);
    expect(c[0]!.symbols.sort()).toEqual(["ETH", "SOL"]);
    expect(c[0]!.weight).toBe(55);
    expect(c[0]!.averageCorrelation).toBeGreaterThan(0.7);
  });
  test("weight per line, crypto cap, cluster and daily loss against the settings", () => {
    const now = T0 + 120 * DAY + 3_600_000;
    const holdings = [H("1", "ETH", "crypto", 1, eth.at(-1)!.close), H("2", "SOL", "crypto", 1, sol.at(-1)!.close), H("3", "AAPL", "stock", 1, aapl.at(-1)!.close)];
    // Prices 10 % under the previous close (the last candle is today's): a big daily loss.
    const prev = { eth: eth.at(-2)!.close, sol: sol.at(-2)!.close, aapl: aapl.at(-2)!.close };
    const a = analyzePortfolio(holdings, 0, { "crypto:ETH": M(prev.eth * 0.9, eth), "crypto:SOL": M(prev.sol * 0.9, sol), "stock:AAPL": M(prev.aapl * 0.9, aapl) });
    const daily = { "crypto:ETH": eth, "crypto:SOL": sol, "stock:AAPL": aapl };
    const d = dailyChange(a, daily, now)!;
    expect(d.percent).toBeCloseTo(-10, 6);
    expect(d.covered).toBe(3);
    const clusters = correlatedClusters(a.lines.map((l) => ({ symbol: l.symbol, weight: l.weight, daily: daily[`${l.kind}:${l.symbol}` as keyof typeof daily] })));
    const checks = checkLimits(a, { ...DEFAULT_RISK, maxPositionPercent: 20, maxCryptoPercent: 30 }, { clusters, daily: d });
    const by = Object.fromEntries(checks.map((c) => [c.code, c]));
    expect(by.max_weight!.level).toBe("danger");
    expect(by.crypto_cap!.level).toBe("warning");
    expect(by.daily_loss!.level).toBe("danger");
    expect(by.daily_loss!.detail).toContain(DAILY_LOSS_REACHED);
    if (a.allocation.crypto > 40 && clusters.some((c) => c.weight > 40)) expect(by.cluster!.level).toBe("warning");
    const calm = checkLimits(a, { ...DEFAULT_RISK, maxPositionPercent: 100, maxCryptoPercent: 100, dailyLossLimitPercent: 50, riskPerTradePercent: 100 }, { clusters: [], daily: d });
    expect(calm.every((c) => c.level === "ok")).toBe(true);
    expect(checkLimits(a, DEFAULT_RISK, { clusters: [], daily: null }).find((c) => c.code === "daily_loss")!.level).toBe("na");
  });
  test("previous close: yesterday's candle when today's exists, else the last one", () => {
    const c = series([10, 11, 12]);
    expect(previousClose(c, T0 + 2 * DAY + 5_000)).toBe(11);
    expect(previousClose(c, T0 + 3 * DAY + 5_000)).toBe(12);
    expect(previousClose([], T0)).toBeNull();
  });
});

describe("positions that became dangerous", () => {
  const closes = Array.from({ length: 30 }, (_, i) => 100 + (i % 2));
  const c = series(closes);
  test("stop broken, stop within one ATR, loss beyond the risk per idea", () => {
    const holdings = [H("a", "BTC", "crypto", 1, 100, 101.5), H("b", "ETH", "crypto", 1, 100, 99.5), H("c", "AAPL", "stock", 1, 200), H("d", "MSFT", "stock", 1, 50, 10)];
    const a = analyzePortfolio(holdings, 10_000, { "crypto:BTC": M(101, c), "crypto:ETH": M(101, c), "stock:AAPL": M(101, c), "stock:MSFT": M(101, c) });
    const stops = Object.fromEntries(holdings.map((h) => [h.id, h.stop]));
    const d = dangerousPositions(a, DEFAULT_RISK, { "crypto:BTC": c, "crypto:ETH": c, "stock:AAPL": c, "stock:MSFT": c }, stops);
    const by = Object.fromEntries(d.map((x) => [x.symbol, x.reasons.map((r) => r.code)]));
    expect(by.BTC).toEqual(["stop_broken"]);
    expect(by.ETH).toEqual(["near_stop"]);
    // AAPL: −99 $ latent on a 10 404 $ portfolio (1 % = 104 $): not yet; with 0.5 % it is.
    expect(by.AAPL).toBeUndefined();
    expect(by.MSFT).toBeUndefined();
    const strict = dangerousPositions(a, { ...DEFAULT_RISK, riskPerTradePercent: 0.5 }, {}, {});
    expect(strict.map((x) => x.symbol)).toEqual(["AAPL"]);
  });
});
