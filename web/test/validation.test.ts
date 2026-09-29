import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import raw from "../../backend/tests/samples/validation.json";
import {
  beatText, CLASS_ORDER, pooledText, regimeDaysText, regimesOf, signedPct, sortAssets, validationUrl, verdictTone, type RegimeGroup, type ValidationReport,
} from "../src/webapp/model-validation";
import { AssetRow, GroupCard, RegimeCard, ValidationView } from "../src/webapp/Validation";

const report = raw as unknown as ValidationReport;

describe("contract sample (real /api/validation answer)", () => {
  test("every basket asset is tested or listed as a failure, groups add up", () => {
    expect(validationUrl()).toBe("/api/validation");
    expect(report.assets.length + report.failures.length).toBe(report.basket.length);
    expect(report.overall.trades).toBe(report.assets.reduce((s, a) => s + a.trades, 0));
    expect(report.classes.reduce((s, g) => s + g.assets, 0)).toBe(report.overall.assets);
    expect(report.overall.regimes.reduce((s, g) => s + g.trades, 0)).toBe(report.overall.trades);
    expect(report.overall.beatHold).toBe(report.assets.filter((a) => a.beatHold).length);
    expect(report.classes.map((g) => g.id)).toEqual(CLASS_ORDER.filter((c) => report.classes.some((g) => g.id === c)));
    expect(report.outOfSample.status).toBe("notVerifiable");
    expect(report.parameters.taxRatePct).toBe(30);
    expect(report.headline).toStartWith(`Sur ${report.overall.assets} actifs`);
  });

  test("the list keeps the basket order by default, never ranked by performance", () => {
    const byClass = sortAssets(report.assets, "class");
    expect(byClass.map((a) => a.symbol)).toEqual(report.assets.map((a) => a.symbol));
    const byGap = sortAssets(report.assets, "gap").map((a) => a.totalReturn - a.buyAndHold);
    expect(byGap).toEqual([...byGap].sort((a, b) => b - a));
    const byName = sortAssets(report.assets, "name").map((a) => a.symbol);
    expect(byName).toEqual([...byName].sort((a, b) => a.localeCompare(b, "fr")));
    expect(report.assets[0]!.symbol).toBe(report.basket[0]!.symbol);
  });
});

describe("texts", () => {
  const g: RegimeGroup = {
    regime: "bear", label: "Marché baissier", trades: 7, winRate: 42.9, profitFactor: 0.8, expectancy: -0.31, avgR: -0.1, tStat: -0.4, days: 3922, assets: 20,
    beatHold: 7, beatShare: 35, medianSignal: -1.5, medianHold: -9.25, lowSample: true, verdict: "insufficient", verdictLabel: "Échantillon trop faible",
  };
  test("numbers in French with signs", () => {
    expect(signedPct(12.345)).toBe("+12,3 %");
    expect(signedPct(-4)).toBe("−4 %");
    expect(signedPct(null)).toBe("—");
    expect(beatText(6, 34, 17.6)).toBe("6 sur 34 (18 %)");
    expect(beatText(0, 0, null)).toBe("aucun actif comparable");
    expect(pooledText(g)).toBe("7 trades · réussite 43 % · espérance −0,31 % · facteur de profit 0,8 · t = −0,4");
    expect(pooledText({ ...g, trades: 0 })).toBe("Aucun trade.");
    expect(regimeDaysText(g)).toContain("7 sur 20 (35 %) actifs");
    expect(regimeDaysText({ ...g, assets: 0 })).toContain("Aucun actif");
    expect(verdictTone("edge")).toBe("good");
    expect(verdictTone("negative")).toBe("warning");
    expect(verdictTone("unproven")).toBe("info");
  });

  test("unknown regime last", () => {
    const order = regimesOf(report.overall).map((r) => r.regime);
    expect(order.at(-1)).toBe(order.includes("unknown") ? "unknown" : order.at(-1)!);
    expect(order.slice(0, 4)).toEqual(["bull", "bear", "range", "crisis"]);
  });
});

describe("views", () => {
  test("the whole screen renders from the sample, with its bias sections", () => {
    const html = renderToStaticMarkup(createElement(ValidationView, { report }));
    expect(html).toContain("Biais et limites");
    expect(html).toContain("Protections contre les biais");
    expect(html).toContain("Hors échantillon");
    expect(html).toContain(report.overall.verdictLabel);
    for (const a of report.assets) expect(html).toContain(`/app/actif/${a.kind}/${a.symbol}`);
    // No wide table anywhere (mobile: stacked rows only).
    expect(html).not.toContain("<table");
  });

  test("cards and rows", () => {
    const cls = report.classes[0]!;
    expect(renderToStaticMarkup(createElement(GroupCard, { g: cls, taxRate: 30 }))).toContain(cls.label);
    const reg = report.overall.regimes[0]!;
    expect(renderToStaticMarkup(createElement(RegimeCard, { g: reg }))).toContain(reg.label);
    const a = report.assets.find((x) => x.lowSample)!;
    expect(renderToStaticMarkup(createElement(AssetRow, { a }))).toContain("échantillon trop faible");
  });
});
