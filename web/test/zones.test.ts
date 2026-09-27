import { expect, test } from "bun:test";
import { fibZone, fibZones, level, swingAt, weekly, zoneEvidence, zoneState } from "../src/engine/fibonacci";
import { align, headlineThemes, macroAdvice, macroEvidence, macroReport, marketStress, MACRO, type MacroSeries } from "../src/engine/macro";
import { guard, weigh } from "../src/engine/guard";
import { adviseAsset } from "../src/engine/advice";
import { DEFAULT_RISK } from "../src/engine/risk";
import type { Candle } from "../src/engine/signal";

const D = 86_400_000;
/** Candles following a path of closes (small wicks). */
const path = (closes: number[], step = D, t0 = Date.UTC(2024, 0, 1)): Candle[] =>
  closes.map((c, i) => ({ time: t0 + i * step, open: i ? closes[i - 1]! : c, high: Math.max(c, i ? closes[i - 1]! : c) * 1.002, low: Math.min(c, i ? closes[i - 1]! : c) * 0.998, close: c, volume: 1000 }));
const lin = (from: number, to: number, n: number) => Array.from({ length: n }, (_, i) => from + ((to - from) * (i + 1)) / n);

test("semaines : bougies journalières regroupées du lundi au dimanche", () => {
  const monday = Date.UTC(2026, 8, 21); // Monday 21/09/2026
  const w = weekly(path(lin(100, 113, 14), D, monday));
  expect(w.length).toBe(2);
  expect(w[0]!.time).toBe(monday);
  expect(w[0]!.close).toBeCloseTo(100 + 13 * 7 / 14, 9);
  expect(w[1]!.high).toBeGreaterThan(w[0]!.high);
  expect(w[0]!.volume).toBe(7000);
});

test("mouvement : hausse, baisse, et rebond après un plus bas qui devient le mouvement en cours", () => {
  const up = path([...lin(100, 100, 30), ...lin(100, 150, 40), ...lin(150, 135, 10)]);
  const s = swingAt(up, up.length - 1, 120, 4)!;
  expect(s.trend).toBe("up");
  expect(s.low).toBeCloseTo(100 * 0.998, 6);
  expect(s.high).toBeCloseTo(150 * 1.002, 6);
  const down = path([...lin(150, 150, 30), ...lin(150, 100, 40), ...lin(100, 105, 10)]);
  expect(swingAt(down, down.length - 1, 120, 4)!.trend).toBe("down");
  // Rebound of more than 38.2 % of the fall: the rebound is the current up move.
  const rebound = path([...lin(150, 150, 30), ...lin(150, 100, 40), ...lin(100, 130, 30)]);
  const r = swingAt(rebound, rebound.length - 1, 150, 4)!;
  expect(r.trend).toBe("up");
  expect(r.low).toBeCloseTo(100 * 0.998, 6);
  expect(r.high).toBeCloseTo(130 * 1.002, 6);
  // A flat market: no move to trace levels on.
  expect(swingAt(path(lin(100, 100.5, 80)), 79, 120, 4)).toBeNull();
});

test("niveaux de Fibonacci et position du prix", () => {
  const s = { trend: "up" as const, low: 100, high: 200, lowIndex: 0, highIndex: 10, lowTime: 0, highTime: 1 };
  expect(level(s, 0.382)).toBeCloseTo(161.8, 9);
  expect(level(s, 0.618)).toBeCloseTo(138.2, 9);
  expect(zoneState(s, 170).status).toBe("above");
  expect(zoneState(s, 170).distance).toBeCloseTo((1 - 161.8 / 170) * 100, 9);
  expect(zoneState(s, 150).status).toBe("inZone");
  expect(zoneState(s, 136).status).toBe("golden");
  expect(zoneState(s, 120).status).toBe("deep");
  expect(zoneState(s, 99).status).toBe("broken");
  expect(zoneState(s, 150).text).toContain("zone d'achat");
});

test("zone d'un horizon : bornes, objectifs, tendance baissière sans zone d'achat", () => {
  const c = path([...lin(100, 100, 30), ...lin(100, 200, 50), ...lin(200, 150, 15)]);
  const z = fibZone(c, "medium");
  expect(z.status).toBe("inZone");
  expect(z.zone!.to).toBeCloseTo(level(z.swing!, 0.382), 9);
  expect(z.golden!.from).toBeCloseTo(level(z.swing!, 0.65), 9);
  expect(z.invalidation).toBe(z.swing!.low);
  expect(z.targets[0]).toBe(z.swing!.high);
  expect(z.targets[2]).toBeCloseTo(z.swing!.low + 1.618 * (z.swing!.high - z.swing!.low), 9);
  const down = fibZone(path([...lin(200, 200, 30), ...lin(200, 100, 50), ...lin(100, 104, 10)]), "medium");
  expect(down.status).toBe("downtrend");
  expect(down.zone).toBeNull();
  expect(fibZone(path(lin(100, 110, 10)), "short").status).toBe("none");
  // Three horizons; the long one uses the weekly candles of the long history.
  const zs = fibZones(c, c, c);
  expect(zs.map((x) => x.horizon)).toEqual(["short", "medium", "long"]);
  expect(zs[2]!.unit).toBe("1 sem.");
});

test("historique des zones : aucun regard vers le futur et taux cohérents", () => {
  // Repeated impulses and pull-backs.
  const closes: number[] = [];
  let p = 100;
  for (let k = 0; k < 8; k++) {
    closes.push(...lin(p, p * 1.3, 25));
    p *= 1.3;
    closes.push(...lin(p, p * 0.86, 12));
    p *= 0.86;
  }
  const c = path(closes);
  const e = zoneEvidence(c, "medium")!;
  expect(e.samples).toBeGreaterThan(0);
  expect(e.rate).toBeGreaterThanOrEqual(0);
  expect(e.rate).toBeLessThanOrEqual(100);
  expect(e.lift).toBeCloseTo(e.base > 0 ? e.rate / e.base : e.lift, 9);
  // The move known at a past candle does not change when the future changes.
  const i = 150;
  const altered = c.map((x, j) => (j > i ? { ...x, high: x.high * 3, low: x.low * 3, close: x.close * 3, open: x.open * 3 } : x));
  expect(swingAt(altered, i, 120, 4)).toEqual(swingAt(c, i, 120, 4));
});

// ---------- Macro ----------

function macroSeries(stressAtEnd: boolean, stressFrom = 395): MacroSeries {
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

test("stress de marché : peur, chute du S&P 500 et pétrole inhabituel ; calme sinon", () => {
  const calm = macroReport(macroSeries(false), [], Date.UTC(2026, 0, 1));
  expect(calm.level).toBe("calm");
  expect(calm.factors).toEqual([]);
  const tense = macroReport(macroSeries(true), [], Date.UTC(2026, 0, 1));
  expect(tense.factors.map((f) => f.code)).toEqual(expect.arrayContaining(["vixHigh", "vixJump", "spxDrawdown", "oil"]));
  expect(tense.level).toBe("high");
  expect(tense.values.vix!.value).toBe(34);
  // No look-ahead: the stress of a past day only depends on the data up to that day.
  const a = align(macroSeries(true));
  expect(marketStress(a.values, 300).score).toBe(marketStress(align(macroSeries(false)).values, 300).score);
});

test("actualités : thèmes du jour, escalade comptée, spéculation (question) ignorée", () => {
  const now = Date.UTC(2026, 8, 27, 12);
  const items = [
    { title: "Country X declares war on country Y - Reuters", time: now - 3_600_000 },
    { title: "Fed signals more rate hikes as inflation persists - WSJ", time: now - 7_200_000 },
    { title: "Is an Agentic Bank Run Coming? - Apollo", time: now - 3_600_000 },
    { title: "New tariffs on steel imports - CNN", time: now - 20 * 3_600_000 },
    { title: "Old war story", time: now - 3 * D },
  ];
  const h = headlineThemes(items, now);
  expect(h.themes.map((t) => t.theme)).toEqual(expect.arrayContaining(["geopolitics", "monetary", "trade"]));
  expect(h.factors).toHaveLength(1);
  expect(h.factors[0]!.text).toContain("déclaration de guerre");
  expect(h.factors[0]!.text).not.toContain("panique bancaire");
});

test("conseil macro par horizon et intégration au garde-fou", () => {
  expect(macroAdvice("calm", "short")).toBeNull();
  expect(macroAdvice("high", "short")).toContain("éviter");
  expect(macroAdvice("high", "long")).toContain("échelonnés");
  const report = macroReport(macroSeries(true), [], Date.UTC(2026, 0, 1));
  const daily = path(lin(100, 140, 400));
  const input = { kind: "stock" as const, daily, h4: [], h1: [] };
  const verified = { samples: 150, rate: 48, base: 24, lift: 2 };
  const g = guard({ ...input, macro: { report, evidence: verified } });
  const f = g.shock.factors.find((x) => x.code === "macro")!;
  expect(f.status).toBe("verified");
  expect(f.points).toBe(35);
  expect(g.shock.level).not.toBe("calm");
  // On an asset where macro stress never preceded falls, the factor is shown but not counted.
  const rejected = guard({ ...input, macro: { report, evidence: { samples: 50, rate: 16, base: 16, lift: 1 } } });
  expect(rejected.shock.factors.find((x) => x.code === "macro")!.points).toBe(0);
  expect(weigh(null, true).status).toBe("unproven");
  expect(guard({ ...input, macro: { report: macroReport(macroSeries(false)), evidence: verified } }).shock.factors.map((x) => x.code)).not.toContain("macro");
});

test("preuve macro sur un actif : jours de stress suivis d'une baisse, comparés à tous les jours", () => {
  // Stress on days 300–304, the asset falls by 40 % right after.
  const s = macroSeries(true, 300);
  const closes = Array.from({ length: 400 }, (_, i) => (i >= 302 && i < 310 ? 60 : 100 + Math.sin(i / 5)));
  const e = macroEvidence(path(closes, D, Date.UTC(2025, 0, 1)), s);
  expect(e).not.toBeNull();
  expect(e!.rate).toBeGreaterThan(e!.base);
  expect(e!.lift).toBeCloseTo(e!.base > 0 ? e!.rate / e!.base : 1, 9);
  expect(MACRO.tense).toBeLessThan(MACRO.high);
});

test("le conseil ajoute la zone de votre horizon et la prudence macro", () => {
  const a = adviseAsset({
    signal: null, reliability: "high", price: 100, risk: DEFAULT_RISK,
    zone: { label: "Moyen terme", status: "above", text: "Prix au-dessus de la zone : attendre un repli.", macroNote: "Contexte macro tendu : entrer en deux ou trois fois." },
  });
  expect(a.points.at(-2)).toBe("Moyen terme (votre horizon) : Prix au-dessus de la zone : attendre un repli.");
  expect(a.points.at(-1)).toContain("macro");
  const none = adviseAsset({ signal: null, reliability: "high", price: 100, risk: DEFAULT_RISK, zone: { label: "Court terme", status: "none", text: "" } });
  expect(none.points.join(" ")).not.toContain("votre horizon");
});
