/**
 * Golden outputs of the macro and guard engines (web/src/engine/{macro,guard}.ts) and of their server glue
 * (web/server/{guard,macro}.ts, upstream answers mocked), compared by tests/parity_b.rs. Run through golden.ts.
 */
import type { Candle } from "../../web/src/engine/signal";
import { atr, ema, rsi } from "../../web/src/engine/signal";
import {
  divergence, guard, hourlyShockFactors, newsTone, percentileRank, pivots, regime, reversal, reversalEvidence, shock,
  shockEvidence, technicalReversal, weigh, type GuardInput, type NewsItem,
} from "../../web/src/engine/guard";
import { align, headlineThemes, macroAdvice, macroEvidence, macroReport, marketStress, type MacroSeries } from "../../web/src/engine/macro";
import { find, inputs, NOW, write, SERVER } from "./golden";

const D = 86_400_000, H4 = 14_400_000, H1 = 3_600_000;
const SYMBOLS = ["BTC", "ETH", "SOL", "DOGE", "AAPL", "NVDA", "SPY"];
const cut = (c: Candle[], n: number) => c.slice(0, Math.max(0, c.length - n));

// ---------- Deterministic generators (same as web/test/guard*.test.ts and zones.test.ts) ----------

function rng(seed: number) {
  let s = seed >>> 0;
  return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
}
function series(n: number, step: number, drift: number, vol: number, seed: number, start = 100, t0 = 1_700_000_000_000): Candle[] {
  const r = rng(seed);
  let p = start;
  return Array.from({ length: n }, (_, i) => {
    const o = p;
    p = p * (1 + drift + (r() - 0.5) * 2 * vol);
    const hi = Math.max(o, p) * (1 + r() * vol * 0.5), lo = Math.min(o, p) * (1 - r() * vol * 0.5);
    return { time: t0 + i * step, open: o, high: hi, low: lo, close: p, volume: 1000 + r() * 200 };
  });
}
const path = (closes: number[], step = D, t0 = Date.UTC(2024, 0, 1)): Candle[] =>
  closes.map((c, i) => ({ time: t0 + i * step, open: i ? closes[i - 1]! : c, high: Math.max(c, i ? closes[i - 1]! : c) * 1.002, low: Math.min(c, i ? closes[i - 1]! : c) * 0.998, close: c, volume: 1000 }));
const lin = (from: number, to: number, n: number) => Array.from({ length: n }, (_, i) => from + ((to - from) * (i + 1)) / n);

function synthMacro(stressAtEnd: boolean, stressFrom = 395): MacroSeries {
  const n = 400;
  const t = (i: number) => Date.UTC(2025, 0, 1) + i * D;
  const wave = (i: number, base: number, amp: number) => base * (1 + amp * Math.sin(i / 7));
  const mk = (f: (i: number) => number) => Array.from({ length: n }, (_, i) => ({ time: t(i), close: f(i) }));
  const end = (i: number) => stressAtEnd && i >= stressFrom && i < stressFrom + 5;
  return {
    vix: mk((i) => (end(i) ? 34 : wave(i, 15, 0.05))),
    spx: mk((i) => (end(i) ? 5000 * 0.9 : wave(i, 5000, 0.005))),
    oil: mk((i) => (end(i) ? 80 * 1.25 : wave(i, 80, 0.01))),
    gold: mk((i) => wave(i, 2000, 0.005)),
    dollar: mk((i) => wave(i, 100, 0.002)),
    rates: mk((i) => wave(i, 4, 0.01)),
  };
}

/** Macro series built from the real closes (stock dates, crypto dates with weekends, different starts). */
function realMacro(): MacroSeries {
  const pts = (sym: string, f: (c: number, i: number) => number) => find(sym, "long")!.candles.map((c, i) => ({ time: c.time, close: f(c.close, i) }));
  const nv = find("NVDA", "long")!.candles.map((c) => c.close);
  const nvMean = nv.reduce((a, b) => a + b, 0) / nv.length;
  return {
    vix: pts("NVDA", (c) => (c / nvMean) * 18),
    spx: pts("SPY", (c) => c * 10),
    oil: pts("ETH", (c) => c / 40),
    gold: pts("BTC", (c) => c / 30),
    dollar: pts("AAPL", (c) => c / 2),
    rates: pts("SOL", (c) => c / 40),
  };
}
const macroSets: Record<string, () => MacroSeries> = {
  calm: () => synthMacro(false),
  tense: () => synthMacro(true),
  stress300: () => synthMacro(true, 300),
  real: realMacro,
  spxOnly: () => ({ spx: realMacro().spx }),
  noVix: () => { const m = realMacro(); return { ...m, vix: [] }; },
  empty: () => ({}),
  short: () => { const m = synthMacro(true, 2); return Object.fromEntries(Object.entries(m).map(([k, v]) => [k, v!.slice(0, 7)])); },
};

// ---------- Headlines ----------

const T0 = Date.UTC(2026, 8, 27, 12);
const headlines: NewsItem[] = [
  { title: "Country X declares war on country Y - Reuters", time: T0 - H1 },
  { title: "Fed signals more rate hikes as inflation persists - WSJ", time: T0 - 2 * H1 },
  { title: "Is an Agentic Bank Run Coming? - Apollo", time: T0 - H1 },
  { title: "New tariffs on steel imports - CNN", time: T0 - 20 * H1 },
  { title: "Old war story", time: T0 - 3 * D },
  { title: "Russia invades neighbouring region, troops cross border", time: T0 - 3 * H1, source: "BBC" },
  { title: "Nuclear threat raised after missile test", time: T0 - 4 * H1 },
  { title: "Missile strikes on oil facilities rattle markets", time: T0 - 5 * H1 },
  { title: "Iran closes the Strait of Hormuz", time: T0 - 6 * H1 },
  { title: "President declares martial law", time: T0 - 7 * H1 },
  { title: "Regional bank collapse triggers contagion fears", time: T0 - 8 * H1 },
  { title: "Trading halted after circuit breaker trips", time: T0 - 9 * H1 },
  { title: "Country defaults on its debt", time: T0 - 10 * H1 },
  { title: "Will the ECB cut interest rates? - FT", time: T0 - 2 * H1 },
  { title: "Recession fears grow as sell-off deepens", time: T0 - 13 * H1 },
  { title: "FOMC minutes: Powell sees CPI cooling", time: T0 - 14 * H1 },
  { title: "Trade war escalates with new export controls", time: T0 - 15 * H1 },
  { title: "Embargo on chips widens", time: T0 - 16 * H1 },
  { title: "Future headline", time: T0 + H1 },
  { title: "Federal Reserve holds; treasury yields rise", time: T0 - 23 * H1 },
  { title: "Ceasefire talks stall; hostages still held", time: T0 - 11 * H1 },
  { title: "Fedora release notes (not the Fed)", time: T0 - H1 },
  { title: "Airstrikes on the capital intensify", time: T0 - 2 * H1 },
  { title: "Bank runs spread? - Opinion - Some Blog", time: T0 - H1 },
  { title: "Défaut de paiement : la banque centrale réagit", time: T0 - H1 },
];

// ---------- macro ----------
{
  const cases: { args: unknown; output: unknown }[] = [];
  for (const [name, make] of Object.entries(macroSets)) {
    const s = make();
    const a = align(s);
    cases.push({ args: { fn: "align", set: name }, output: a });
    const n = a.dates.length;
    const idx = [...new Set([0, 4, 5, 60, 65, 66, 200, 299, 300, 301, 302, 303, 304, 305, 306, 390, 395, 396, 399, n - 2, n - 1, ...Array.from({ length: Math.floor(n / 9) }, (_, k) => k * 9)])].filter((i) => i >= 0 && i < n);
    for (const i of idx) cases.push({ args: { fn: "marketStress", set: name, i }, output: marketStress(a.values, i) });
    for (const [newsName, news] of [["none", []], ["all", headlines], ["few", headlines.slice(0, 5)]] as const) {
      for (const now of [T0, T0 + 13 * H1, Date.UTC(2026, 0, 1)]) {
        cases.push({ args: { fn: "macroReport", set: name, news: newsName, now }, output: macroReport(s, news as NewsItem[], now) });
      }
    }
    for (const sym of SYMBOLS) {
      for (const iv of ["1d", "long"]) {
        cases.push({ args: { fn: "macroEvidence", set: name, symbol: sym, interval: iv }, output: macroEvidence(find(sym, iv)!.candles, s) });
      }
    }
  }
  // Asset whose fall follows the stress (zones.test.ts).
  const closes = Array.from({ length: 400 }, (_, i) => (i >= 302 && i < 310 ? 60 : 100 + Math.sin(i / 5)));
  const asset = path(closes, D, Date.UTC(2025, 0, 1));
  cases.push({ args: { fn: "macroEvidenceCandles", set: "stress300", candles: asset }, output: macroEvidence(asset, synthMacro(true, 300)) });
  cases.push({ args: { fn: "macroEvidenceCandles", set: "stress300", candles: asset.slice(0, 10) }, output: macroEvidence(asset.slice(0, 10), synthMacro(true, 300)) });
  for (const now of [T0, T0 + 6 * H1, T0 + 12 * H1, T0 + 30 * H1]) {
    for (const k of [1, 3, 5, headlines.length]) {
      const items = headlines.slice(0, k);
      cases.push({ args: { fn: "headlineThemes", items, now }, output: headlineThemes(items, now) });
    }
    for (const h of headlines) cases.push({ args: { fn: "headlineThemes", items: [h], now }, output: headlineThemes([h], now) });
  }
  for (const level of ["calm", "tense", "high"] as const) {
    for (const hz of ["short", "medium", "long"] as const) cases.push({ args: { fn: "macroAdvice", level, horizon: hz }, output: macroAdvice(level, hz) });
  }
  write("macro", cases);
}

// ---------- guard ----------

/** Real-data guard input: candles referenced by symbol, the other inputs inline. */
type RealArgs = {
  symbol: string; daily: "1d" | "long"; cutD: number; cutH4: number; cutH1: number;
  positioning?: GuardInput["positioning"]; sentiment?: GuardInput["sentiment"]; news?: NewsItem[]; vix?: number[] | null;
  macro?: GuardInput["macro"]; kind?: "crypto" | "stock"; now: number;
};
function realInput(a: RealArgs): GuardInput {
  const i = find(a.symbol, a.daily)!;
  return {
    kind: a.kind ?? i.kind,
    daily: cut(i.candles, a.cutD),
    h4: cut(find(a.symbol, "4h")!.candles, a.cutH4),
    h1: cut(find(a.symbol, "1h")!.candles, a.cutH1),
    positioning: a.positioning, sentiment: a.sentiment, news: a.news, vix: a.vix, macro: a.macro, now: a.now,
  };
}

const tenseReport = macroReport(synthMacro(true), headlines, T0);
const highReport = macroReport(synthMacro(true), [], T0);
const tenseOnly = { ...highReport, marketScore: 30, factors: [...highReport.factors.slice(0, 2), { code: "escalation", points: 20, text: "Actualité : invasion, menace nucléaire (dernières 12 h, non vérifiable sur l'historique)." }] };
const calmReport = macroReport(synthMacro(false), [], T0);
const macros = [
  null,
  { report: tenseReport, evidence: { samples: 150, rate: 48, base: 24, lift: 2 } },
  { report: highReport, evidence: { samples: 50, rate: 16, base: 16, lift: 1 } },
  { report: tenseOnly, evidence: null },
  { report: tenseOnly, evidence: { samples: 10, rate: 40, base: 20, lift: 2 } },
  { report: calmReport, evidence: { samples: 150, rate: 30, base: 24, lift: 1.25 } },
];
const newsSet = (now: number): NewsItem[] => [
  { title: "Exchange hacked, $200M stolen", time: now - H1 },
  { title: "SEC opens investigation into token issuer", time: now - 2 * H1 },
  { title: "Bitcoin rally continues", time: now - 3 * H1 },
  { title: "Shares surge after upgrade", time: now - 4 * H1 },
  { title: "ETF approval: record high for the token", time: now - 5 * H1, source: "Reuters" },
  { title: "Stock plunges then surges", time: now - 30 * H1 },
  ...Array.from({ length: 8 }, (_, i) => ({ title: `Breaking ${i}`, time: now - (i + 1) * 0.5 * H1 })),
  ...Array.from({ length: 10 }, (_, i) => ({ title: `Headline ${i}`, time: now - (i + 1) * 15 * H1 })),
  { title: "Tomorrow's news", time: now + H1 },
];
const positionings = [
  undefined,
  { fundingRate: 0.0008, longShortRatio: [...Array.from({ length: 48 }, (_, i) => 1.2 + (i % 5) * 0.02), 1.9], openInterest: [...Array.from({ length: 30 }, (_, i) => 1e9 * (1 + i * 0.01))] },
  { fundingRate: 0.0004, longShortRatio: [...Array.from({ length: 30 }, (_, i) => 1.5 - (i % 7) * 0.03), 0.9] },
  { fundingRate: -0.0005, longShortRatio: [...Array.from({ length: 30 }, (_, i) => 1.5 - (i % 7) * 0.03), 0.9], openInterest: Array.from({ length: 26 }, (_, i) => 1e9 * (1 + i * 0.02)) },
  { fundingRate: -0.00015 },
  { fundingRate: 0.00001234, longShortRatio: [1, 2, 3] },
  { fundingRate: null, openInterest: Array.from({ length: 40 }, (_, i) => 2e9 * (1 + i * 0.01)) },
];
const sentiments = [
  undefined,
  { fearGreed: [60, 70, 86], socialBullish: 91, socialSample: 30 },
  { fearGreed: [60, 77], socialBullish: 20, socialSample: 25 },
  { fearGreed: [30, 18], socialBullish: 95, socialSample: 10 },
  { fearGreed: [30, 23] },
  { fearGreed: [], socialBullish: null, socialSample: 0 },
];

{
  const cases: { args: unknown; output: unknown }[] = [];
  // Real data, every asset, several cuts and every optional input.
  let v = 0;
  for (const symbol of SYMBOLS) {
    for (const daily of ["long", "1d"] as const) {
      for (const [cutD, cutH4, cutH1] of [[0, 0, 0], [30, 50, 100], [100, 6, 7], [500, 200, 250]]) {
        const base: RealArgs = { symbol, daily, cutD, cutH4, cutH1, now: NOW };
        cases.push({ args: base, output: guard(realInput(base)) });
        v++;
        const extra: RealArgs = {
          ...base,
          positioning: positionings[v % positionings.length],
          sentiment: sentiments[v % sentiments.length],
          news: newsSet(NOW),
          vix: v % 2 ? [18, 24, 31] : [20, 21],
          macro: macros[v % macros.length],
        };
        cases.push({ args: extra, output: guard(realInput(extra)) });
      }
    }
  }
  write("guard-real", cases);
}

{
  // Reference scenarios (guard-scenarios.test.ts, guard.test.ts, zones.test.ts), full inputs.
  const NOWS = 1_800_000_000_000;
  const calmUp = (): GuardInput => ({ kind: "crypto", daily: series(400, D, 0.004, 0.01, 1), h4: series(300, H4, 0.0007, 0.004, 2), h1: series(400, H1, 0.0002, 0.002, 3), now: NOWS });
  const jumped = (): GuardInput => {
    const input = calmUp();
    const h1 = input.h1.slice();
    const L = h1[h1.length - 1]!;
    h1[h1.length - 3] = { ...h1[h1.length - 3]!, close: L.open * 0.95, low: L.open * 0.94, volume: 20_000 };
    h1[h1.length - 2] = { ...h1[h1.length - 2]!, open: L.open * 0.95, close: L.open * 0.97, high: L.open * 0.975, low: L.open * 0.945, volume: 18_000 };
    h1[h1.length - 1] = { ...L, open: L.open * 0.97, close: L.open * 0.955, high: L.open * 0.975, low: L.open * 0.95, volume: 15_000 };
    return { ...input, h1 };
  };
  const zonesDaily = path(lin(100, 140, 400));
  const scenarios: Record<string, () => GuardInput> = {
    calmUp,
    down: () => ({
      kind: "crypto", daily: series(400, D, -0.004, 0.01, 4), h4: series(300, H4, -0.0007, 0.004, 5), h1: series(400, H1, -0.0002, 0.002, 12),
      positioning: { fundingRate: -0.0005 }, sentiment: { fearGreed: [30, 18] }, now: NOWS,
    }),
    broken: () => ({
      kind: "crypto",
      daily: [...series(355, D, 0.003, 0.005, 6), ...series(25, D, -0.012, 0.005, 7, 289, 1_700_000_000_000 + 355 * D)],
      h4: series(300, H4, 0, 0.004, 7), h1: series(400, H1, 0, 0.003, 13), now: NOWS,
    }),
    jumped,
    crowd: () => ({
      ...calmUp(),
      positioning: { fundingRate: 0.0008, longShortRatio: [...Array.from({ length: 48 }, (_, i) => 1.2 + (i % 5) * 0.02), 1.9] },
      sentiment: { fearGreed: [60, 70, 86], socialBullish: 91, socialSample: 30 },
      news: [
        { title: "Exchange hacked, $200M stolen", time: NOWS - H1 },
        { title: "SEC opens investigation into token issuer", time: NOWS - 2 * H1 },
        { title: "Bitcoin rally continues", time: NOWS - 3 * H1 },
      ],
    }),
    stock: () => ({
      ...calmUp(),
      kind: "stock",
      news: [
        ...Array.from({ length: 10 }, (_, i) => ({ title: `Headline ${i}`, time: NOWS - (i + 1) * 20 * H1 })),
        ...Array.from({ length: 6 }, (_, i) => ({ title: `Breaking ${i}`, time: NOWS - (i + 1) * 0.5 * H1 })),
      ],
      vix: [18, 24, 31],
    }),
    busy: () => ({
      ...calmUp(),
      news: [
        ...Array.from({ length: 20 }, (_, i) => ({ title: `Headline ${i}`, time: NOWS - (i + 1) * 7 * H1 })),
        ...Array.from({ length: 3 }, (_, i) => ({ title: `Breaking ${i}`, time: NOWS - (i + 1) * H1 })),
      ],
    }),
    upReversal: () => ({
      kind: "crypto", daily: series(400, D, -0.004, 0.01, 4), h4: series(300, H4, -0.0007, 0.004, 5), h1: [],
      positioning: { fundingRate: -0.0005 }, sentiment: { fearGreed: [30, 18] }, now: NOWS,
    }),
    shortHistory: () => ({ kind: "crypto", daily: series(30, D, 0.004, 0.01, 1), h4: [], h1: [], now: NOWS }),
    rangeMove: () => ({ kind: "crypto", daily: [...series(200, D, 0, 0.004, 21), ...series(6, D, 0.02, 0.001, 22, 100, 1_700_000_000_000 + 200 * D)], h4: series(100, H4, 0, 0.004, 23), h1: series(120, H1, 0, 0.002, 24), now: NOWS }),
    rangeFlat: () => ({ kind: "crypto", daily: series(90, D, 0, 0.002, 25), h4: series(61, H4, 0, 0.004, 26), h1: series(97, H1, 0, 0.002, 27), now: NOWS }),
    zonesVerified: () => ({ kind: "stock", daily: zonesDaily, h4: [], h1: [], macro: { report: highReport, evidence: { samples: 150, rate: 48, base: 24, lift: 2 } }, now: NOWS }),
    zonesRejected: () => ({ kind: "stock", daily: zonesDaily, h4: [], h1: [], macro: { report: highReport, evidence: { samples: 50, rate: 16, base: 16, lift: 1 } }, now: NOWS }),
    zonesCalm: () => ({ kind: "stock", daily: zonesDaily, h4: [], h1: [], macro: { report: calmReport, evidence: { samples: 150, rate: 48, base: 24, lift: 2 } }, vix: [18, 40], now: NOWS }),
    squeeze: () => {
      const h4 = series(300, H4, 0, 0.01, 30);
      const flat = h4.slice(280).map((c, i) => ({ ...c, open: 100, close: 100 + (i % 2) * 0.01, high: 100.02, low: 99.99 }));
      return { ...calmUp(), h4: [...h4.slice(0, 280), ...flat] };
    },
    volSpike: () => {
      const h1 = series(400, H1, 0, 0.002, 31);
      const wild = series(24, H1, 0, 0.012, 32, h1[375]!.close, h1[376]!.time);
      return { ...calmUp(), h1: [...h1.slice(0, 376), ...wild] };
    },
  };
  const cases: { args: unknown; output: unknown }[] = [];
  for (const [name, make] of Object.entries(scenarios)) {
    const input = make();
    cases.push({ args: { name, input }, output: guard(input) });
  }
  // Direct reversal calls with a forced trend (guard.test.ts).
  for (const trend of ["up", "down", "range"] as const) {
    const input: GuardInput = { ...calmUp(), positioning: { fundingRate: 0.0008 } };
    cases.push({ args: { name: `reversal-${trend}`, input, trend }, output: reversal(input, trend) });
  }
  write("guard-scenarios", cases);
}

{
  // Building blocks on real data.
  const cases: { args: unknown; output: unknown }[] = [];
  for (const symbol of SYMBOLS) {
    for (const daily of ["1d", "long"]) {
      for (const h4cut of [0, 100]) {
        const d = find(symbol, daily)!.candles, h4 = cut(find(symbol, "4h")!.candles, h4cut);
        cases.push({ args: { fn: "regime", symbol, daily, h4cut }, output: regime(d, h4) });
      }
    }
    cases.push({ args: { fn: "regimeNoH4", symbol }, output: regime(find(symbol, "1d")!.candles, []) });
    for (const c of [0, 150]) {
      cases.push({ args: { fn: "shockEvidence", symbol, cut: c }, output: shockEvidence(cut(find(symbol, "1h")!.candles, c), cut(find(symbol, "4h")!.candles, c)) });
    }
    for (const iv of ["4h", "1d", "long"]) {
      const tf = iv === "4h" ? "4h" : "1d";
      const c = find(symbol, iv)!.candles;
      cases.push({ args: { fn: "reversalEvidence", symbol, interval: iv, tf }, output: reversalEvidence(c, tf) });
      const closes = c.map((x) => x.close);
      const r = rsi(closes), e20 = ema(closes, 20), a = atr(c);
      const out = [];
      for (let i = 0; i < c.length; i += 3) {
        for (const dir of ["down", "up"] as const) {
          out.push({ i, dir, f: technicalReversal(c, r, e20, a, i, dir, tf), div: divergence(c, r, dir, i) });
        }
      }
      cases.push({ args: { fn: "technicalReversal", symbol, interval: iv, tf }, output: out });
      cases.push({ args: { fn: "pivots", symbol, interval: iv }, output: { high: pivots(c.map((x) => x.high), "high"), low: pivots(c.map((x) => x.low), "low", 5, c.length - 10) } });
    }
    const h1 = find(symbol, "1h")!.candles;
    const r = h1.slice(1).map((c, i) => Math.log(c.close / h1[i]!.close));
    const hs = [];
    for (let i = 90; i <= r.length; i += 2) hs.push({ i, f: hourlyShockFactors(h1, r, i) });
    cases.push({ args: { fn: "hourlyShockFactors", symbol }, output: hs });
  }
  for (const [values, x] of [[[], 1], [[1, 2, 3], 2], [[1, 1, 1], 1], [[5, 4, 3, 2], 10], [[0.5, 0.2], -1]] as [number[], number][]) {
    cases.push({ args: { fn: "percentileRank", values, x }, output: percentileRank(values, x) });
  }
  const evs = [null, { samples: 5, rate: 80, base: 20, lift: 4 }, { samples: 100, rate: 20, base: 20, lift: 1 }, { samples: 100, rate: 30, base: 20, lift: 1.5 },
    { samples: 100, rate: 25, base: 20, lift: 1.25 }, { samples: 100, rate: 22.4, base: 20, lift: 1.12 }, { samples: 20, rate: 21.9, base: 20, lift: 1.095 }, { samples: 30, rate: 90, base: 20, lift: 4.5 }];
  for (const e of evs) for (const historical of [true, false]) cases.push({ args: { fn: "weigh", e, historical }, output: weigh(e, historical) });
  const toneItems: NewsItem[][] = [
    [
      { title: "Company hit by lawsuit", time: 0 },
      { title: "Shares surge after upgrade", time: 0 },
      { title: "Stock plunges then surges", time: 0 },
      { title: "Quarterly report published", time: 0 },
      { title: "Bankruptcy fears", time: 0 },
    ],
    [{ title: "New banner campaign", time: 0 }],
    [{ title: "SEC CHARGES founder", time: 0 }, { title: "Piratage massif : enquête ouverte", time: 0 }, { title: "Hausse record du bitcoin", time: 0 }],
    [{ title: "Sell-off! Crash.", time: 0 }, { title: "all-time high (again)", time: 0 }, { title: "Buyback; partnership", time: 0 }, { title: "banned-list", time: 0 }],
    [{ title: "Chute de l'action", time: 0 }, { title: "L'interdiction tombe", time: 0 }, { title: "rachat d'actions", time: 0 }, { title: "Élan: la hausseà venir", time: 0 }],
    [{ title: "Ünterbanned halt", time: 0 }, { title: "ÉTF INFLOWS 🚀rally🚀", time: 0 }, { title: "", time: 0 }],
  ];
  for (const items of toneItems) cases.push({ args: { fn: "newsTone", items }, output: newsTone(items) });
  for (const h of headlines) cases.push({ args: { fn: "newsTone", items: [h] }, output: newsTone([h]) });
  // Divergence reference series (guard.test.ts).
  const closes = [
    ...Array.from({ length: 40 }, (_, i) => 100 + i * 0.1),
    ...Array.from({ length: 12 }, (_, i) => 104 + i * 1.5),
    ...Array.from({ length: 10 }, (_, i) => 119.3 - i * 1.2),
    ...Array.from({ length: 20 }, (_, i) => 108.5 + i * 0.65),
    ...Array.from({ length: 5 }, (_, i) => 121 - i * 0.8),
  ];
  const c: Candle[] = closes.map((x, i) => ({ time: i * H4, open: x, high: x * 1.001, low: x * 0.999, close: x, volume: 1000 }));
  cases.push({ args: { fn: "divergenceRef", candles: c }, output: { down: divergence(c, rsi(closes), "down"), up: divergence(c, rsi(closes), "up") } });
  cases.push({ args: { fn: "pivotsRef" }, output: pivots([1, 2, 3, 9, 3, 2, 1, 2, 3, 4, 10, 4, 3, 2], "high") });
  // Shock on its own with the stock inputs and macro variants.
  for (const [k, m] of macros.entries()) {
    const a: RealArgs = { symbol: "SPY", daily: "1d", cutD: 0, cutH4: 0, cutH1: 0, news: newsSet(NOW), vix: [20, 31], macro: m, now: NOW };
    cases.push({ args: { fn: "shock", k, input: a }, output: shock(realInput(a)) });
  }
  write("guard-parts", cases);
}

// ---------- server: parsers, guardReport and macro() with upstream answers mocked ----------
// Recorded from the TypeScript server (web/server) before it was replaced by the Rust one: these golden files are
// frozen (tests/golden/guard-report.json, guard-parse.json, macro-server.json) and only rewritten while it exists.
if (SERVER) {
const { clearCache } = await import("../../web/server/cache");
const { guardReport, parseGuard } = await import("../../web/server/guard");
const { macro } = await import("../../web/server/macro");

const funding = { code: "0", data: [{ instId: "BTC-USDT-SWAP", fundingRate: "-0.0000093022018830", fundingTime: "1790524800000" }] };
const ls = { code: "0", data: [["1790517600000", "1.2"], ["1790514000000", "1.27"], ["1790510400000", "1.29"]], msg: "" };
const oi = { code: "0", data: [["1790521200000", "3096187639.1516", "373921486.7371"], ["1790517600000", "3151244635.3541", "379272299.6517"], ["1790514000000", "3131575585.0841", "119727471.7871"]], msg: "" };
const fng = { data: [{ value: "70", value_classification: "Greed", timestamp: "1790467200" }, { value: "74", value_classification: "Greed", timestamp: "1790380800" }, { value: "71", value_classification: "Greed", timestamp: "1790294400" }] };
const vixBody = { data: [{ date: "2026-09-23", volume: "0.0", open: "15.10", high: "15.80", low: "14.90", close: "15.42" }, { date: "2026-09-24", volume: "0.0", open: "15.40", high: "16.20", low: "15.00", close: "15.61" }, { date: "2026-09-25", volume: "0.0", open: "15.60", high: "15.90", low: "14.70", close: "14.87" }] };
const rss = '<rss><channel><title>Google News</title><item><title>Is It Too Late to Buy Bitcoin After a 32% Rally in Two Months? - 24/7 Wall St.</title><pubDate>Sun, 27 Sep 2026 14:25:00 GMT</pubDate><source url="https://247wallst.com">24/7 Wall St.</source></item><item><title>Bitcoin Holders Are Selling, But This Time It’s Different: What You Need to Know - Yahoo Finance</title><pubDate>Sun, 27 Sep 2026 11:00:58 GMT</pubDate><source url="https://finance.yahoo.com">Yahoo Finance</source></item><item><title><![CDATA[S&amp;P 500 &amp; Bitcoin: “risk-on” returns]]></title><pubDate>Sat, 26 Sep 2026 22:15:03 GMT</pubDate></item><item><title>No date here</title></item></channel></rss>';
const stocktwits = { messages: [
  { entities: { sentiment: { basic: "Bullish" } } }, { entities: { sentiment: { basic: "Bearish" } } }, { entities: { sentiment: null } },
  { entities: {} }, {}, null, { entities: { sentiment: { basic: "Bullish" } } }, { entities: { sentiment: { basic: "" } } },
] };

/** RSS feed of headlines relative to NOW (for the report's 24 h window). */
function rssOf(items: { title: string; ago: number; source?: string }[]) {
  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
  return `<rss><channel>${items.map((i) => `<item><title>${esc(i.title)}</title><pubDate>${new Date(NOW - i.ago).toUTCString()}</pubDate>${i.source ? `<source url="https://x">${esc(i.source)}</source>` : ""}</item>`).join("")}</channel></rss>`;
}
const assetRss = rssOf([
  { title: "Exchange hacked, $200M stolen", ago: H1, source: "CoinDesk" },
  { title: "SEC opens investigation into token issuer", ago: 2 * H1 },
  { title: "Shares surge after upgrade", ago: 3 * H1, source: "Reuters" },
  { title: "Lawsuit filed & dismissed", ago: 4 * H1 },
  ...Array.from({ length: 7 }, (_, i) => ({ title: `Breaking ${i}`, ago: (i + 1) * 0.4 * H1 })),
  { title: "Old news", ago: 3 * D },
]);

{
  const cases: { args: unknown; output: unknown }[] = [];
  const P = parseGuard;
  const samples: [string, unknown][] = [
    ["funding", funding], ["funding", { code: "51001", data: [] }], ["funding", { code: "0", data: [{ fundingRate: "" }] }],
    ["funding", { code: "0", data: [{ fundingRate: "abc" }] }], ["funding", { code: "0", data: [] }], ["funding", { code: 0, data: [{ fundingRate: "0.1" }] }],
    ["funding", { code: "0", data: [{ fundingRate: 0.00025 }] }], ["funding", { code: "0", data: [{ fundingRate: " 1e-4 " }] }], ["funding", null],
    ["rubik", ls], ["rubik", oi], ["rubik", { code: "50011", data: null }], ["rubik", { code: "0", data: [["1", "x"], ["2"], ["3", "4.5"], ["4", "Infinity"]] }],
    ["fearGreed", fng], ["fearGreed", {}], ["fearGreed", { data: [{ value: "12" }, { value: "n/a" }, { value: 40 }] }],
    ["stocktwits", stocktwits], ["stocktwits", { messages: [] }], ["stocktwits", {}],
    ["vix", vixBody], ["vix", { data: Array.from({ length: 80 }, (_, i) => ({ close: String(10 + i / 3) })) }], ["vix", { data: [{ close: "bad" }] }],
  ];
  for (const [fn, d] of samples) cases.push({ args: { fn, d }, output: (P as any)[fn](d) });
  for (const xml of [rss, assetRss, "", "<rss><item><title>  &lt;b&gt; &quot;hi&quot; &#39;x&apos; &amp;amp; </title><pubDate>2026-09-27T10:00:00Z</pubDate><source>  </source></item></rss>"]) {
    cases.push({ args: { fn: "rss", d: xml }, output: P.rss(xml) });
  }

  // guardReport with mocked upstreams.
  const realFetch = globalThis.fetch;
  type Body = { status: number; body: unknown };
  const run = async (symbol: string, kind: "crypto" | "stock", name: string, bodies: Record<string, Body>, withMacro: number | null, missing: string[]) => {
    clearCache();
    const urls: string[] = [];
    globalThis.fetch = (async (u: string | URL) => {
      const url = String(u);
      urls.push(url);
      const key = url.includes("funding-rate") ? "funding" : url.includes("long-short") ? "ls" : url.includes("open-interest") ? "oi"
        : url.includes("alternative.me") ? "fng" : url.includes("stocktwits") ? "st" : url.includes("news.google") ? "news" : url.includes("cboe") ? "vix" : "?";
      const b = bodies[key] ?? { status: 404, body: {} };
      return new Response(typeof b.body === "string" ? b.body : JSON.stringify(b.body), { status: b.status });
    }) as typeof fetch;
    const candles = async (iv: "1h" | "4h" | "1d") => {
      if (missing.includes(iv)) throw new Error(`bougies ${iv} indisponibles`);
      return find(symbol, iv)!.candles;
    };
    const mac = withMacro === null ? undefined : async () => macros[withMacro] ?? null;
    let output: unknown;
    try {
      output = await guardReport(symbol, kind, name, candles, NOW, mac);
    } catch (e) {
      output = { error: String((e as Error).message) };
    }
    globalThis.fetch = realFetch;
    return { args: { symbol, kind, name, bodies, macro: withMacro, missing, urls: urls.sort() }, output };
  };
  const ok = (body: unknown): Body => ({ status: 200, body });
  const fail: Body = { status: 500, body: "err" };
  cases.push(await run("BTC", "crypto", "Bitcoin", { funding: ok(funding), ls: ok(ls), oi: ok(oi), fng: ok(fng), st: ok(stocktwits), news: ok(assetRss) }, null, []));
  cases.push(await run("ETH", "crypto", "Ethereum", { funding: ok({ code: "0", data: [{ fundingRate: "0.0009" }] }), ls: fail, oi: ok({ code: "0", data: [] }), fng: ok({ data: [{ value: "85" }] }), st: fail, news: fail }, 1, ["1h"]));
  cases.push(await run("SOL", "crypto", "Solana", { funding: fail, ls: fail, oi: fail, fng: fail, st: ok({ messages: [] }), news: ok(rss) }, 3, []));
  cases.push(await run("AAPL", "stock", "Apple Inc.", { st: ok(stocktwits), news: ok(assetRss), vix: ok(vixBody) }, null, []));
  cases.push(await run("NVDA", "stock", "NVIDIA Corporation", { st: fail, news: ok(assetRss), vix: ok({ data: [{ close: "20" }, { close: "31" }] }) }, 2, []));
  cases.push(await run("SPY", "stock", "SPDR S&P 500 ETF Trust", { vix: fail }, 0, ["1h"]));
  cases.push(await run("BRK-B", "stock", "Berkshire Hathaway, Inc.", {}, null, ["1d"]));
  write("guard-report", cases.filter((c) => (c.args as any).fn === undefined));
  write("guard-parse", cases.filter((c) => (c.args as any).fn !== undefined));
}

{
  // macro(): Yahoo series (real closes in Yahoo's chart format) and world headlines, mocked.
  const realFetch = globalThis.fetch;
  const yahooOf = (c: Candle[], holes = false) => ({
    chart: {
      result: [{
        timestamp: c.map((x) => x.time / 1000),
        indicators: { quote: [{ open: c.map((x, i) => (holes && i % 17 === 3 ? null : x.open)), high: c.map((x) => x.high), low: c.map((x) => x.low), close: c.map((x) => x.close), volume: c.map((x, i) => (i % 5 ? x.volume : null)) }] },
      }],
      error: null,
    },
  });
  const yahooBodies: Record<string, { status: number; body: unknown }> = {
    "^VIX": { status: 200, body: yahooOf(realMacro().vix!.map((p) => ({ time: p.time, open: p.close, high: p.close, low: p.close, close: p.close, volume: 0 }))) },
    "^GSPC": { status: 200, body: yahooOf(find("SPY", "long")!.candles, true) },
    "CL=F": { status: 200, body: yahooOf(find("ETH", "long")!.candles) },
    "GC=F": { status: 500, body: "down" },
    "DX-Y.NYB": { status: 200, body: { chart: { result: null, error: { code: "Not Found", description: "No data found, symbol may be delisted" } } } },
    "^TNX": { status: 200, body: yahooOf(find("SOL", "long")!.candles.slice(-300)) },
  };
  const worldRss = [
    rssOf([{ title: "Country X declares war on country Y - Reuters", ago: H1 }, { title: "Missile strikes on oil facilities rattle markets", ago: 2 * H1, source: "AP" }, { title: "Shared headline", ago: 3 * H1 }]),
    rssOf([{ title: "Fed signals more rate hikes as inflation persists - WSJ", ago: 2 * H1 }, { title: "Shared headline", ago: 5 * H1 }, { title: "Is a bank run coming?", ago: H1 }]),
  ];
  const cases: { args: unknown; output: unknown }[] = [];
  const run = async (name: string, yahoo: typeof yahooBodies, news: { status: number; body: string }[]) => {
    clearCache();
    let q = 0;
    globalThis.fetch = (async (u: string | URL) => {
      const url = String(u);
      if (url.includes("news.google")) {
        const i = url.includes("Federal") ? 1 : 0;
        q++;
        const b = news[i]!;
        return new Response(b.body, { status: b.status });
      }
      const sym = decodeURIComponent(url.split("/chart/")[1]!.split("?")[0]!);
      const b = yahoo[sym] ?? { status: 404, body: {} };
      return new Response(typeof b.body === "string" ? b.body : JSON.stringify(b.body), { status: b.status });
    }) as typeof fetch;
    let output: unknown;
    try {
      output = await macro(NOW);
    } catch (e) {
      output = { error: String((e as Error).message) };
    }
    globalThis.fetch = realFetch;
    cases.push({ args: { name, yahoo, news, queries: q }, output });
  };
  await run("full", yahooBodies, worldRss.map((body) => ({ status: 200, body })));
  await run("newsDown", yahooBodies, [{ status: 503, body: "" }, { status: 200, body: worldRss[1]! }]);
  await run("noVix", { ...yahooBodies, "^VIX": { status: 500, body: "x" } }, worldRss.map((body) => ({ status: 200, body })));
  await run("nothing", { "^VIX": { status: 500, body: "x" }, "^GSPC": { status: 200, body: { chart: { result: [{ timestamp: [], indicators: { quote: [{}] } }] } } } }, [{ status: 500, body: "" }, { status: 500, body: "" }]);
  clearCache();
  write("macro-server", cases);
}
}

// Shared fixtures of the cases above (macro series, headlines, macro contexts), so Rust uses the exact same inputs.
write("macro-sets", [
  ...Object.entries(macroSets).map(([name, make]) => ({ args: { name }, output: make() as unknown })),
  { args: { name: "headlines" }, output: headlines },
  { args: { name: "macros" }, output: macros },
]);
