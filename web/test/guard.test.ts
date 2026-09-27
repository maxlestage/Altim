import { expect, test } from "bun:test";
import { divergence, guard, GUARD, newsTone, pivots, regime, reversal, reversalEvidence, shock, shockEvidence, technicalReversal, weigh, type GuardInput } from "../src/engine/guard";
import { atr, ema, rsi, type Candle } from "../src/engine/signal";

// Deterministic generator (same seed → same candles), shared scenarios with GuardTests.swift.
function rng(seed: number) {
  let s = seed >>> 0;
  return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
}
function series(n: number, step: number, drift: number, vol: number, seed = 1, start = 100, t0 = 1_700_000_000_000): Candle[] {
  const r = rng(seed);
  let p = start;
  return Array.from({ length: n }, (_, i) => {
    const o = p;
    p = p * (1 + drift + (r() - 0.5) * 2 * vol);
    const hi = Math.max(o, p) * (1 + r() * vol * 0.5), lo = Math.min(o, p) * (1 - r() * vol * 0.5);
    return { time: t0 + i * step, open: o, high: hi, low: lo, close: p, volume: 1000 + r() * 200 };
  });
}
const D = 86_400_000, H4 = 14_400_000, H1 = 3_600_000;
const calmUp = (): GuardInput => ({
  kind: "crypto",
  daily: series(400, D, 0.004, 0.01, 1),
  h4: series(300, H4, 0.0007, 0.004, 2),
  h1: series(400, H1, 0.0002, 0.002, 3),
});

test("régime : tendance de fond haussière, baissière, sans direction", () => {
  expect(regime(series(400, D, 0.004, 0.01, 1), series(300, H4, 0.0007, 0.004, 2)).trend).toBe("up");
  expect(regime(series(400, D, -0.004, 0.01, 4), series(300, H4, -0.0007, 0.004, 5)).trend).toBe("down");
  // Long rise then a sharp fall below the 200-day average, averages not yet reversed: no clear trend.
  const broken = [...series(355, D, 0.003, 0.005, 6), ...series(25, D, -0.012, 0.005, 7, 100 * 1.003 ** 355, 1_700_000_000_000 + 355 * D)];
  expect(regime(broken, series(300, H4, 0, 0.004, 7)).trend).toBe("range");
  expect(regime(broken, []).text).toContain("en dessous de la moyenne 200 jours");
  expect(regime(series(30, D, 0.004, 0.01, 1), []).text).toContain("insuffisant");
  const up = regime(series(400, D, 0.004, 0.01, 1), series(300, H4, 0.0007, 0.004, 2));
  expect(up.strength).toBeGreaterThan(0);
  expect(up.strength).toBeLessThanOrEqual(100);
});

test("marché calme : pas de choc, pas de retournement, bots autorisés", () => {
  const g = guard(calmUp());
  expect(g.regime.trend).toBe("up");
  expect(g.shock.level).toBe("calm");
  expect(g.policy.scalping).toBe("ok");
  expect(g.policy.sizeMultiplier).toBe(1);
});

test("choc : un saut de prix de plusieurs écarts-types est détecté et freine le court terme", () => {
  const input = calmUp();
  const h1 = input.h1.slice();
  const lastC = h1[h1.length - 1]!;
  // A 5 % drop in one hour, then two agitated hours.
  h1[h1.length - 3] = { ...h1[h1.length - 3]!, close: lastC.open * 0.95, low: lastC.open * 0.94, volume: 20_000 };
  h1[h1.length - 2] = { ...h1[h1.length - 2]!, open: lastC.open * 0.95, close: lastC.open * 0.97, high: lastC.open * 0.975, low: lastC.open * 0.945, volume: 18_000 };
  h1[h1.length - 1] = { ...lastC, open: lastC.open * 0.97, close: lastC.open * 0.955, high: lastC.open * 0.975, low: lastC.open * 0.95, volume: 15_000 };
  const s = shock({ ...input, h1 });
  const codes = s.factors.map((f) => f.code);
  expect(codes).toContain("jump4");
  expect(codes).toContain("volume");
  // Never seen on this (synthetic, calm) history: counted at half weight, not ignored.
  for (const f of s.factors.filter((x) => ["jump4", "volume"].includes(x.code))) {
    expect(f.status).toBe("unproven");
    expect(f.points).toBe(Math.round(f.basePoints * 0.5));
  }
  expect(s.level).not.toBe("calm");
  expect(guard({ ...input, h1 }).policy.scalping).not.toBe("ok");
});

test("auto-validation : un facteur compte selon ce qu'il a réellement annoncé sur cet actif", () => {
  expect(weigh(null, false)).toEqual({ weight: GUARD.unverifiableWeight, status: "unverifiable" });
  expect(weigh({ samples: 5, rate: 80, base: 20, lift: 4 }, true)).toEqual({ weight: 0.5, status: "unproven" });
  expect(weigh({ samples: 100, rate: 20, base: 20, lift: 1 }, true)).toEqual({ weight: 0, status: "rejected" });
  expect(weigh({ samples: 100, rate: 30, base: 20, lift: 1.5 }, true)).toEqual({ weight: 1, status: "verified" });
  expect(weigh({ samples: 100, rate: 25, base: 20, lift: 1.25 }, true).weight).toBeCloseTo(0.5, 9);
  expect(weigh({ samples: 100, rate: 22.4, base: 20, lift: 1.12 }, true).weight).toBeCloseTo(0.24, 9);
});

test("choc : rafale d'actualités et VIX (actions)", () => {
  const now = 1_800_000_000_000;
  const news = [
    ...Array.from({ length: 10 }, (_, i) => ({ title: `Headline ${i}`, time: now - (i + 1) * 20 * H1 })),
    ...Array.from({ length: 6 }, (_, i) => ({ title: `Breaking ${i}`, time: now - (i + 1) * 0.5 * H1 })),
  ];
  const s = shock({ ...calmUp(), kind: "stock", news, vix: [18, 24, 31], now });
  const codes = s.factors.map((f) => f.code);
  expect(codes).toContain("newsBurst");
  expect(codes).toContain("vixHigh");
  expect(codes).toContain("vixJump");
  // VIX is ignored for a crypto.
  expect(shock({ ...calmUp(), vix: [18, 24, 31] }).factors.map((f) => f.code)).not.toContain("vixHigh");
});

test("pivots et divergence : nouveau sommet du prix sans nouveau sommet du RSI", () => {
  expect(pivots([1, 2, 3, 9, 3, 2, 1, 2, 3, 4, 10, 4, 3, 2], "high")).toEqual([3, 10]);
  // Strong rally to a first top, pull-back, then a slow climb to a slightly higher top.
  const closes = [
    ...Array.from({ length: 40 }, (_, i) => 100 + i * 0.1),
    ...Array.from({ length: 12 }, (_, i) => 104 + i * 1.5), // sharp rise → high RSI
    ...Array.from({ length: 10 }, (_, i) => 119.3 - i * 1.2), // pull-back
    ...Array.from({ length: 20 }, (_, i) => 108.5 + i * 0.65), // slow climb to a higher high
    ...Array.from({ length: 5 }, (_, i) => 121 - i * 0.8), // confirmation of the second top
  ];
  const c: Candle[] = closes.map((x, i) => ({ time: i * H4, open: x, high: x * 1.001, low: x * 0.999, close: x, volume: 1000 }));
  const r = rsi(closes);
  expect(divergence(c, r, "down")).toBe(true);
  expect(divergence(c, r, "up")).toBe(false);
});

test("retournement : levier surchargé, avidité extrême, foule unanime et actualités négatives en pleine hausse", () => {
  const now = 1_800_000_000_000;
  const input: GuardInput = {
    ...calmUp(),
    positioning: { fundingRate: 0.0008, longShortRatio: [...Array.from({ length: 48 }, (_, i) => 1.2 + (i % 5) * 0.02), 1.9] },
    sentiment: { fearGreed: [60, 70, 86], socialBullish: 91, socialSample: 30 },
    news: [
      { title: "Exchange hacked, $200M stolen", time: now - H1 },
      { title: "SEC opens investigation into token issuer", time: now - 2 * H1 },
      { title: "Bitcoin rally continues", time: now - 3 * H1 },
    ],
    now,
  };
  const r = reversal(input, "up");
  expect(r.direction).toBe("down");
  const codes = r.factors.map((f) => f.code);
  for (const c of ["funding", "longShort", "fearGreed", "social", "newsTone"]) expect(codes).toContain(c);
  expect(r.factors.find((f) => f.code === "funding")!.basePoints).toBe(25);
  expect(r.score).toBeGreaterThanOrEqual(GUARD.reversalHigh);
  const g = guard(input);
  expect(g.policy.notes.join(" ")).toContain("retournement à la baisse");
  expect(g.policy.scalping).not.toBe("ok");
});

test("retournement à la hausse : vendeurs à découvert surchargés et peur extrême en tendance baissière", () => {
  const r = reversal({
    kind: "crypto",
    daily: series(400, D, -0.004, 0.01, 4),
    h4: series(300, H4, -0.0007, 0.004, 5),
    h1: [],
    positioning: { fundingRate: -0.0005 },
    sentiment: { fearGreed: [30, 18] },
  }, "down");
  expect(r.direction).toBe("up");
  expect(r.factors.map((f) => f.code)).toEqual(expect.arrayContaining(["funding", "fearGreed"]));
});

test("ton des actualités : un titre compte une fois, positif et négatif s'annulent", () => {
  expect(newsTone([
    { title: "Company hit by lawsuit", time: 0 },
    { title: "Shares surge after upgrade", time: 0 },
    { title: "Stock plunges then surges", time: 0 },
    { title: "Quarterly report published", time: 0 },
    { title: "Bankruptcy fears", time: 0 },
  ])).toEqual({ negative: 2, positive: 1 });
  // A word inside another word does not count ("banner" is not "ban").
  expect(newsTone([{ title: "New banner campaign", time: 0 }])).toEqual({ negative: 0, positive: 0 });
});

test("preuves sur l'historique : taux cohérents et aucun regard vers le futur", () => {
  const h4 = series(400, H4, 0.001, 0.01, 9);
  for (const e of Object.values(reversalEvidence(h4, "4h"))) {
    expect(e.samples).toBeGreaterThan(0);
    expect(e.rate).toBeGreaterThanOrEqual(0);
    expect(e.rate).toBeLessThanOrEqual(100);
    expect(e.base).toBeGreaterThan(0);
    expect(e.lift).toBeCloseTo(e.rate / e.base, 9);
  }
  expect(reversalEvidence(h4.slice(0, 80), "4h")).toEqual({});
  const shockEv = shockEvidence(series(500, H1, 0, 0.003, 10), series(400, H4, 0, 0.006, 11));
  for (const e of Object.values(shockEv)) expect(e.lift).toBeCloseTo(e.rate / e.base, 9);
  // The factors at candle i must not change when the future is modified.
  const i = 250;
  const closes = h4.map((c) => c.close);
  const before = technicalReversal(h4, rsi(closes), ema(closes, 20), atr(h4), i, "down", "4h").map((f) => f.code);
  const altered = h4.map((c, j) => (j > i ? { ...c, close: c.close * 1.5, high: c.high * 1.5, low: c.low * 1.5, open: c.open * 1.5 } : c));
  const ac = altered.map((c) => c.close);
  const after = technicalReversal(altered, rsi(ac), ema(ac, 20), atr(altered), i, "down", "4h").map((f) => f.code);
  expect(after).toEqual(before);
});

test("les données sans historique (financement, sentiment, actualités) sont comptées mais signalées non vérifiées", () => {
  const r = reversal({ ...calmUp(), positioning: { fundingRate: 0.0008 } }, "up");
  const f = r.factors.find((x) => x.code === "funding")!;
  expect(f.status).toBe("unverifiable");
  expect(f.points).toBe(Math.round(25 * GUARD.unverifiableWeight));
});
