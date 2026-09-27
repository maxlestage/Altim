import { expect, test } from "bun:test";
import { alignSeries, factorsAt, isPegged, pick, ranks, roles, scoreUniverse, span, toHorizon, validate, CRITERIA, HORIZON_LIST, SECTOR_CAP, SPECS } from "../src/engine/screener";
import { parseListed } from "../server/screener";
import type { Candle } from "../src/engine/signal";

const D = 86_400_000;
/** Daily candles growing at a constant daily rate (small wicks). */
const trendSeries = (n: number, daily: number, start = 100, seed = 1): Candle[] => {
  let s = seed, p = start;
  const r = () => ((s = (s * 1664525 + 1013904223) >>> 0) / 2 ** 32);
  return Array.from({ length: n }, (_, i) => {
    const o = p;
    p = p * (1 + daily + (r() - 0.5) * 0.01);
    return { time: Date.UTC(2020, 0, 1) + i * D, open: o, high: Math.max(o, p) * 1.003, low: Math.min(o, p) * 0.997, close: p, volume: 1000 };
  });
};

test("rangs en centiles : le meilleur à 100, le pire à 0, les inconnus restent inconnus", () => {
  expect(ranks([10, 30, 20, null], true)).toEqual([0, 100, 50, null]);
  expect(ranks([10, 30, 20], false)).toEqual([100, 0, 50]);
  expect(ranks([5, 5, 5], true)).toEqual([50, 50, 50]);
});

test("sélection : les meilleurs, au plus 3 par secteur", () => {
  const items = [
    { s: "A", v: 99, sec: "Tech" }, { s: "B", v: 98, sec: "Tech" }, { s: "C", v: 97, sec: "Tech" }, { s: "D", v: 96, sec: "Tech" },
    { s: "E", v: 95, sec: "Santé" }, { s: "F", v: null, sec: "Santé" }, { s: "G", v: 10, sec: "Énergie" },
  ];
  expect(SECTOR_CAP).toBe(3);
  expect(pick(items, (x) => x.v, (x) => x.sec, 5).map((x) => x.s)).toEqual(["A", "B", "C", "E", "G"]);
  expect(pick(items, (x) => x.v, () => undefined, 2).map((x) => x.s)).toEqual(["A", "B"]);
});

const M = SPECS.stock["3m"], L = SPECS.stock["6m"];

test("critères d'une action : force relative mesurée, et rien ne dépend du futur", () => {
  const up = trendSeries(600, 0.003);
  const f = factorsAt(up, 599, M)!;
  expect(f.momentum).toBeGreaterThan(30);
  expect(f.trend).toBe(100);
  expect(f.atrPct).toBeGreaterThan(0);
  const altered = up.map((c, i) => (i > 450 ? { ...c, close: c.close * 0.5, open: c.open * 0.5, high: c.high * 0.5, low: c.low * 0.5 } : c));
  expect(factorsAt(altered, 450, M)).toEqual(factorsAt(up, 450, M));
  expect(factorsAt(up, 100, M)).toBeNull();
});

test("classement selon la règle de la durée : force, rebond, signal, calme", () => {
  const list = [trendSeries(600, 0.004, 100, 1), trendSeries(600, 0.001, 100, 2), trendSeries(600, -0.001, 100, 3)].map((c) => factorsAt(c, 599, L));
  expect(scoreUniverse(list, L).map((x) => x!.total)).toEqual([100, 50, 0]);
  expect(scoreUniverse(list, { ...L, rank: "reversal" }).map((x) => x!.total)).toEqual([0, 50, 100]);
  expect(scoreUniverse(list, { ...L, rank: "signal" }).map((x) => x!.total)).toEqual(list.map((f) => f!.signal));
  for (const x of scoreUniverse(list, L)) for (const k of Object.keys(CRITERIA)) expect(x!.scores[k as keyof typeof CRITERIA]).toBeGreaterThanOrEqual(0);
  expect(roles(L, "stock").momentum).toBe("classe les actions");
  expect(roles(SPECS.crypto["6m"], "crypto").risk).toContain("les plus calmes");
  expect(roles(SPECS.stock["1h"], "stock").momentum).toContain("en baisse");
  expect(SPECS.stock["6m"].stopAtr).toBeGreaterThan(SPECS.stock["7d"].stopAtr);
});

test("8 durées pour les actions et les cryptos, anciens noms acceptés", () => {
  expect(HORIZON_LIST).toEqual(["30m", "1h", "5h", "7d", "14d", "1m", "3m", "6m"]);
  for (const m of ["stock", "crypto"] as const) for (const h of HORIZON_LIST) {
    const sp = SPECS[m][h];
    expect(sp.hold).toBeGreaterThan(0);
    expect(sp.evidence.length).toBeGreaterThan(20);
    expect(["5m", "15m", "30m", "1d"]).toContain(sp.interval);
  }
  expect(SPECS.stock["30m"].interval).toBe("5m");
  expect(SPECS.crypto["5h"].interval).toBe("30m");
  expect(span(6, "5m", "stock")).toBe("30 min");
  expect(span(10, "30m", "crypto")).toBe("5 h");
  expect(span(126, "1d", "stock")).toBe("6 mois");
  expect(span(7, "1d", "crypto")).toBe("7 jours");
  expect(toHorizon("medium", "stock")).toBe("3m");
  expect(toHorizon("medium", "crypto")).toBe("1m");
  expect(toHorizon("30m", "crypto")).toBe("30m");
  expect(toHorizon("2y", "stock")).toBeNull();
});

test("jetons adossés repérés à leur calme anormal, quelle que soit la taille des bougies", () => {
  expect(isPegged(0.05, "1d")).toBe(true);
  expect(isPegged(2.5, "1d")).toBe(false);
  expect(isPegged(0.01, "5m")).toBe(true);
  expect(isPegged(0.3, "5m")).toBe(false);
  expect(isPegged(null, "1d")).toBe(false);
});

test("rejeu sur le passé : avance nette, faible ou nulle selon les frais et la régularité", () => {
  // 12 assets with persistent, different trends: the strongest keep leading.
  const series = alignSeries(Array.from({ length: 12 }, (_, k) => trendSeries(700, -0.002 + k * 0.0006, 100, k + 1)));
  const v = validate(series, { ...M, hold: 21, step: 21 }, "1m", 3)!;
  expect(v.periods).toBeGreaterThan(5);
  expect(v.top).toBeGreaterThan(v.universe);
  expect(v.beatRate).toBe(100);
  expect(v.edge).toBe("clear");
  // Same ranking, with costs higher than the advance: no edge.
  expect(validate(series, { ...M, hold: 21, step: 21, cost: 0.5 }, "1m", 3)!.edge).toBe("none");
  // The worst first (rebound rule) on persistent trends: behind the average.
  expect(validate(series, { ...M, hold: 21, step: 21, rank: "reversal" }, "1m", 3)!.edge).toBe("none");
  const withBtc = validate(series, { ...SPECS.crypto["1m"] }, "1m", 3, undefined, 0)!;
  expect(withBtc.hold).toBe(30);
  expect(withBtc.benchmark).not.toBeNull();
});

test("univers : plus grandes sociétés, une seule classe d'actions par société", () => {
  const rows = [
    { symbol: "GOOGL", name: "Alphabet Inc. Class A Common Stock", marketCap: "4187062800000.00", sector: "Technology" },
    { symbol: "GOOG", name: "Alphabet Inc. Class C Capital Stock", marketCap: "4146092300000.00", sector: "Technology" },
    { symbol: "AAPL", name: "Apple Inc. Common Stock", marketCap: "4902476945600.00", sector: "Technology" },
    { symbol: "BRK/B", name: "Berkshire Hathaway Inc.", marketCap: "1000000000000", sector: "Finance" },
    { symbol: "XYZ^", name: "Bad symbol", marketCap: "5", sector: "" },
    { symbol: "ZERO", name: "No cap", marketCap: "", sector: "Energy" },
  ];
  const l = parseListed({ data: { rows } }, 10);
  expect(l.map((x) => x.symbol)).toEqual(["AAPL", "GOOGL", "BRK-B"]);
  expect(l[0]!.name).toBe("Apple Inc.");
});

