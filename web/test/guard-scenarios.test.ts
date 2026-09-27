/**
 * Reference scenarios of the market guard (deterministic candles): each of the three layers must be exercised.
 */
import { expect, test } from "bun:test";
import { guard, type GuardInput, type GuardResult } from "../src/engine/guard";
import type { Candle } from "../src/engine/signal";

function rng(seed: number) {
  let s = seed >>> 0;
  return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
}
export function series(n: number, step: number, drift: number, vol: number, seed: number, start = 100, t0 = 1_700_000_000_000): Candle[] {
  const r = rng(seed);
  let p = start;
  return Array.from({ length: n }, (_, i) => {
    const o = p;
    p = p * (1 + drift + (r() - 0.5) * 2 * vol);
    const hi = Math.max(o, p) * (1 + r() * vol * 0.5), lo = Math.min(o, p) * (1 - r() * vol * 0.5);
    return { time: t0 + i * step, open: o, high: hi, low: lo, close: p, volume: 1000 + r() * 200 };
  });
}
const D = 86_400_000, H4 = 14_400_000, H1 = 3_600_000, NOW = 1_800_000_000_000;

function calmUp(): GuardInput {
  return { kind: "crypto", daily: series(400, D, 0.004, 0.01, 1), h4: series(300, H4, 0.0007, 0.004, 2), h1: series(400, H1, 0.0002, 0.002, 3), now: NOW };
}
function jumped(): GuardInput {
  const input = calmUp();
  const h1 = input.h1.slice();
  const L = h1[h1.length - 1]!;
  h1[h1.length - 3] = { ...h1[h1.length - 3]!, close: L.open * 0.95, low: L.open * 0.94, volume: 20_000 };
  h1[h1.length - 2] = { ...h1[h1.length - 2]!, open: L.open * 0.95, close: L.open * 0.97, high: L.open * 0.975, low: L.open * 0.945, volume: 18_000 };
  h1[h1.length - 1] = { ...L, open: L.open * 0.97, close: L.open * 0.955, high: L.open * 0.975, low: L.open * 0.95, volume: 15_000 };
  return { ...input, h1 };
}

export const SCENARIOS: Record<string, () => GuardInput> = {
  calmUp,
  down: () => ({
    kind: "crypto", daily: series(400, D, -0.004, 0.01, 4), h4: series(300, H4, -0.0007, 0.004, 5), h1: series(400, H1, -0.0002, 0.002, 12),
    positioning: { fundingRate: -0.0005 }, sentiment: { fearGreed: [30, 18] }, now: NOW,
  }),
  broken: () => ({
    kind: "crypto",
    daily: [...series(355, D, 0.003, 0.005, 6), ...series(25, D, -0.012, 0.005, 7, 289, 1_700_000_000_000 + 355 * D)],
    h4: series(300, H4, 0, 0.004, 7), h1: series(400, H1, 0, 0.003, 13), now: NOW,
  }),
  jumped,
  crowd: () => ({
    ...calmUp(),
    positioning: { fundingRate: 0.0008, longShortRatio: [...Array.from({ length: 48 }, (_, i) => 1.2 + (i % 5) * 0.02), 1.9] },
    sentiment: { fearGreed: [60, 70, 86], socialBullish: 91, socialSample: 30 },
    news: [
      { title: "Exchange hacked, $200M stolen", time: NOW - H1 },
      { title: "SEC opens investigation into token issuer", time: NOW - 2 * H1 },
      { title: "Bitcoin rally continues", time: NOW - 3 * H1 },
    ],
  }),
  stock: () => ({
    ...calmUp(),
    kind: "stock",
    news: [
      ...Array.from({ length: 10 }, (_, i) => ({ title: `Headline ${i}`, time: NOW - (i + 1) * 20 * H1 })),
      ...Array.from({ length: 6 }, (_, i) => ({ title: `Breaking ${i}`, time: NOW - (i + 1) * 0.5 * H1 })),
    ],
    vix: [18, 24, 31],
  }),
};

const factors = (fs: GuardResult["shock"]["factors"]) =>
  fs.map((f) => ({ code: f.code, points: f.points, basePoints: f.basePoints, status: f.status, samples: f.evidence?.samples ?? null, lift: f.evidence?.lift ?? null }));

test("scénarios de référence du garde-fou", () => {
  const out = Object.entries(SCENARIOS).map(([name, make]) => {
    const g = guard(make());
    return {
      name,
      regime: { trend: g.regime.trend, strength: g.regime.strength },
      shock: { score: g.shock.score, level: g.shock.level, factors: factors(g.shock.factors) },
      reversal: { score: g.reversal.score, direction: g.reversal.direction, factors: factors(g.reversal.factors) },
      policy: { scalping: g.policy.scalping, sizeMultiplier: g.policy.sizeMultiplier, stopMultiplier: g.policy.stopMultiplier },
    };
  });
  // The scenarios must actually exercise the three layers.
  const byName = Object.fromEntries(out.map((o) => [o.name, o]));
  expect(byName.calmUp!.regime.trend).toBe("up");
  expect(byName.down!.regime.trend).toBe("down");
  expect(byName.broken!.regime.trend).toBe("range");
  expect(byName.jumped!.shock.level).not.toBe("calm");
  expect(byName.crowd!.reversal.score).toBeGreaterThanOrEqual(50);
  expect(byName.stock!.shock.factors.map((f) => f.code)).toEqual(expect.arrayContaining(["newsBurst", "vixHigh", "vixJump"]));
});
