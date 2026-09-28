import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import aaplRaw from "../../backend/tests/samples/strategies-aapl.json";
import btcRaw from "../../backend/tests/samples/strategies-btc.json";
import {
  checkpoints, COLORS, DEFAULT_SELECTION, drawable, geometry, indexAt, lowSampleText, ORDER, placeLabels, signedPct, strategiesUrl,
  type StrategiesReport,
} from "../src/webapp/strategies";
import { Chart, ListView, StrategyBlock } from "../src/webapp/StrategiesCard";

const aapl = aaplRaw as unknown as StrategiesReport;
const btc = btcRaw as unknown as StrategiesReport;
const BOX = { w: 320, h: 180, l: 34, r: 62, t: 10, b: 18 };

describe("contract samples (real /api/strategies answers)", () => {
  test("eight strategies in the fixed order, curves on the same dates, ≤ 200 points", () => {
    for (const r of [aapl, btc]) {
      expect(r.strategies.map((s) => s.id)).toEqual(ORDER);
      const avail = r.strategies.filter((s) => s.available);
      for (const s of avail) {
        expect(s.equity.length).toBeLessThanOrEqual(200);
        expect(s.equity[0]![0]).toBe(r.from);
        expect(s.equity.at(-1)![0]).toBe(r.to);
        expect(s.metrics).not.toBeNull();
        expect(s.rule.length).toBeGreaterThan(20);
      }
      expect(r.notes.some((n) => n.includes("aucune optimisation"))).toBe(true);
    }
  });

  test("value: computed on a stock, not applicable on a crypto", () => {
    expect(aapl.strategies.find((s) => s.id === "value")!.available).toBe(true);
    const v = btc.strategies.find((s) => s.id === "value")!;
    expect(v.available).toBe(false);
    expect(v.unavailable).toContain("pas de bénéfices");
  });

  test("low sample flag follows the 10-trade rule, never on buy and hold or DCA", () => {
    for (const s of [...aapl.strategies, ...btc.strategies].filter((x) => x.metrics)) {
      if (s.id === "buyHold" || s.id === "dca") expect(s.lowSample).toBe(false);
      else expect(s.lowSample).toBe(s.metrics!.trades < 10);
      expect(lowSampleText(s) != null).toBe(s.lowSample);
    }
  });
});

describe("chart helpers", () => {
  test("default selection draws the five default strategies, each with its own colour", () => {
    const d = drawable(btc, DEFAULT_SELECTION);
    expect(d.map((s) => s.id)).toEqual(DEFAULT_SELECTION);
    expect(new Set(ORDER.map((id) => COLORS[id])).size).toBe(ORDER.length);
    // Unavailable strategies are never drawn.
    expect(drawable(btc, ["value"])).toEqual([]);
  });

  test("single axis keeps 100 inside, pointer maps back to the nearest point", () => {
    const d = drawable(aapl, DEFAULT_SELECTION);
    const g = geometry(d, BOX)!;
    expect(g.lo).toBeLessThan(100);
    expect(g.hi).toBeGreaterThan(100);
    expect(g.y(g.hi)).toBeCloseTo(BOX.t);
    expect(g.y(g.lo)).toBeCloseTo(BOX.h - BOX.b);
    expect(indexAt(g.x(37), g, BOX)).toBe(37);
    expect(indexAt(-50, g, BOX)).toBe(0);
    expect(indexAt(999, g, BOX)).toBe(g.n - 1);
  });

  test("direct labels never overlap and stay in the plot", () => {
    const ys = [50, 52, 51, 170, 171];
    const out = placeLabels(ys, 11, 14, 162);
    const sorted = [...out].sort((a, b) => a - b);
    for (let k = 1; k < sorted.length; k++) expect(sorted[k]! - sorted[k - 1]!).toBeGreaterThanOrEqual(11 - 1e-9);
    expect(Math.max(...out)).toBeLessThanOrEqual(162);
    expect(Math.min(...out)).toBeGreaterThanOrEqual(14 - 1e-9);
    // Order preserved: the highest curve keeps the highest label.
    expect(out[0]!).toBeLessThan(out[3]!);
  });

  test("list view checkpoints: first and last dates, five at most", () => {
    const s = aapl.strategies[0]!;
    const c = checkpoints(s);
    expect(c.length).toBe(5);
    expect(c[0]!.t).toBe(s.equity[0]![0]);
    expect(c.at(-1)!.t).toBe(s.equity.at(-1)![0]);
  });

  test("formatting and url", () => {
    expect(signedPct(12.345)).toBe("+12,3 %");
    expect(signedPct(-4)).toBe("−4,0 %");
    expect(signedPct(null)).toBe("—");
    expect(strategiesUrl("BRK.B", "stock")).toBe("/api/strategies?symbol=BRK.B&kind=stock");
  });
});

describe("views", () => {
  test("chart: legend, direct labels up to 4 coloured curves, tooltip line", () => {
    const html = renderToStaticMarkup(createElement(Chart, { series: drawable(btc, DEFAULT_SELECTION) }));
    expect(html).toContain("valeur de 100 investis");
    expect(html).toContain("strat-label");
    expect(html).toContain("history-legend");
    const all = renderToStaticMarkup(createElement(Chart, { series: drawable(aapl, ORDER) }));
    expect(all).not.toContain("strat-label");
    expect(all).toContain("passez en liste");
  });

  test("list view and metric cards", () => {
    expect(renderToStaticMarkup(createElement(ListView, { series: drawable(aapl, DEFAULT_SELECTION) }))).toContain("strat-steps");
    const dca = renderToStaticMarkup(createElement(StrategyBlock, { s: aapl.strategies.find((s) => s.id === "dca")! }));
    expect(dca).toContain("TRI");
    expect(dca).toContain("tout investir au départ");
    const value = renderToStaticMarkup(createElement(StrategyBlock, { s: btc.strategies.find((s) => s.id === "value")! }));
    expect(value).toContain("Non applicable");
  });
});
