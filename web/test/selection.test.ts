import { expect, test } from "bun:test";
import { alignSeries, factorsAt, pick, ranks, scoreUniverse, validate, SECTOR_CAP, STOP_ATR, ROLE, CRITERIA } from "../src/engine/screener";
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

test("critères d'une action : force relative mesurée, et rien ne dépend du futur", () => {
  const up = trendSeries(600, 0.003);
  const f = factorsAt(up, 599, "medium")!;
  expect(f.momentum).toBeGreaterThan(30);
  expect(f.trend).toBe(100);
  expect(f.atrPct).toBeGreaterThan(0);
  const altered = up.map((c, i) => (i > 450 ? { ...c, close: c.close * 0.5, open: c.open * 0.5, high: c.high * 0.5, low: c.low * 0.5 } : c));
  expect(factorsAt(altered, 450, "medium")).toEqual(factorsAt(up, 450, "medium"));
  expect(factorsAt(up, 100, "medium")).toBeNull();
});

test("classement de l'univers : la force relative classe, les autres critères sont notés", () => {
  const list = [trendSeries(600, 0.004, 100, 1), trendSeries(600, 0.001, 100, 2), trendSeries(600, -0.001, 100, 3)].map((c) => factorsAt(c, 599, "long"));
  const sc = scoreUniverse(list, "long");
  expect(sc.map((x) => x!.total)).toEqual([100, 50, 0]);
  for (const x of sc) for (const k of Object.keys(CRITERIA)) expect(x!.scores[k as keyof typeof CRITERIA]).toBeGreaterThanOrEqual(0);
  expect(ROLE.momentum).toBe("classe les actions");
  expect(STOP_ATR.long).toBeGreaterThan(STOP_ATR.short);
});

test("rejeu sur le passé : une sélection des plus fortes bat la moyenne quand la force persiste", () => {
  // 12 stocks with persistent, different trends: the strongest keep leading, so the replay must show it.
  const series = alignSeries(Array.from({ length: 12 }, (_, k) => trendSeries(700, -0.002 + k * 0.0006, 100, k + 1)));
  const v = validate(series, "medium", 3)!;
  expect(v.periods).toBeGreaterThan(5);
  expect(v.top).toBeGreaterThan(v.universe);
  expect(v.beatRate).toBe(100);
  expect(v.hold).toBe(63);
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

test("cryptos : classées par le signal technique, durées de détention propres, comparaison au Bitcoin", async () => {
  const { RANK_BY, ROLES, HOLDS } = await import("../src/engine/screener");
  expect(RANK_BY).toEqual({ stock: "momentum", crypto: "signal" });
  expect(ROLES.crypto.signal).toBe("classe les cryptos");
  expect(HOLDS.crypto).toEqual({ short: 10, medium: 30, long: 90 });
  const list = [trendSeries(600, 0.004, 100, 1), trendSeries(600, -0.004, 100, 2)].map((c) => factorsAt(c, 599, "medium"));
  const sc = scoreUniverse(list, "medium", "crypto");
  expect(sc[0]!.total).toBe(list[0]!.signal);
  expect(sc[1]!.total).toBe(list[1]!.signal);
  expect(sc[0]!.total).toBeGreaterThan(sc[1]!.total);
  const series = alignSeries(Array.from({ length: 12 }, (_, k) => trendSeries(700, -0.002 + k * 0.0006, 100, k + 1)));
  const v = validate(series, "medium", 3, 15, undefined, "crypto", 0)!;
  expect(v.hold).toBe(30);
  expect(v.benchmark).not.toBeNull();
});
