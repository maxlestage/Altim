import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import raw from "../../backend/tests/samples/bot.json";
import {
  ACTION_UI, anyEdge, botSummary, botUrl, botViewsUrl, buyText, calibrationRows, isBotView, points, sellText, skillText, waitText,
  type BotReport, type BotView, type SellStats,
} from "../src/webapp/model-bot";
import { AssetRow, BotReportView, Calibration, GroupCard } from "../src/webapp/Bot";
import { BotLine } from "../src/webapp/DecisionCard";

const report = raw as unknown as BotReport;
/** Narrow no-break spaces (before %) read as plain spaces in the expectations. */
const n = (s: string) => s.replace(/\u202f/g, " ");

describe("contract sample (real /api/bot answer)", () => {
  test("groups, assets and signals add up", () => {
    expect(botUrl()).toBe("/api/bot");
    expect(report.groups.map((g) => g.id)).toEqual(["stock", "crypto"]);
    expect(report.assets.length + report.failures.length).toBe(34);
    expect(report.overall.buy.signals).toBe(report.groups.reduce((s, g) => s + g.buy.signals, 0));
    expect(report.overall.sell.signals).toBe(report.groups.reduce((s, g) => s + g.sell.signals, 0));
    for (const g of report.groups) {
      expect(g.calibrationUp.reduce((s, b) => s + b.rows, 0)).toBe(g.labelled);
      expect(g.model!.up.weights.length).toBe(report.features.length);
      if (g.buy.meanNet != null && g.buy.baselineNet != null && g.buy.excess != null) expect(Math.abs(g.buy.meanNet - g.buy.baselineNet - g.buy.excess)).toBeLessThan(0.02);
    }
    expect(report.parameters.horizonDays).toBe(20);
    // The real run found no out-of-sample edge: the headline says it does not count.
    expect(anyEdge(report)).toBe(false);
    expect(report.headline).toStartWith("Hors échantillon, le bot n'a pas fait mieux");
    expect(report.headline).toContain("il ne compte pas dans les décisions");
  });

  test("the screen renders every part, in French", () => {
    const h = renderToStaticMarkup(createElement(BotReportView, { report }));
    expect(h).toContain(report.headline.replace(/'/g, "&#x27;"));
    for (const t of ["Résultats hors échantillon", "Calibration", "Actif par actif", "Limites", "ACHETER", "VENDRE", "ATTENDRE"]) expect(h).toContain(t);
    expect(h).toContain("pour une entrée au hasard");
    const row = renderToStaticMarkup(createElement(AssetRow, { a: report.assets[0]! }));
    expect(row).toContain(report.assets[0]!.symbol);
    expect(row).toContain("Aujourd&#x27;hui : hausse");
    expect(renderToStaticMarkup(createElement(GroupCard, { g: report.groups[0]! }))).toContain(report.groups[0]!.buy.verdictLabel);
    const cal = renderToStaticMarkup(createElement(Calibration, { title: "t", buckets: report.groups[0]!.calibrationUp, skill: report.groups[0]!.brierSkillUp }));
    expect(cal).toContain("prévu");
  });
});

describe("texts", () => {
  test("points, skill, calibration rows", () => {
    expect(points(0.52)).toBe("+0,52 point");
    expect(points(-2.44)).toBe("−2,44 points");
    expect(points(null)).toBe("—");
    expect(n(skillText(-1.1))).toBe("1,1 % moins bien que la fréquence de base");
    expect(skillText(0.2)).toBe("pas mieux que la fréquence de base");
    expect(skillText(null)).toBe("non calculable");
    expect(calibrationRows([{ from: 0, to: 30, label: "x", rows: 0, predicted: null, realised: null }])).toEqual([]);
    expect(botViewsUrl([{ symbol: "BTC", kind: "crypto" }, { symbol: "AAPL", kind: "stock" }])).toBe("/api/bot/views?symbols=BTC%3Acrypto%2CAAPL%3Astock");
  });

  test("sell text says whether the price did better or worse, without a sign to decode", () => {
    const s: SellStats = {
      signals: 7, meanAfter: -1.01, baselineAfter: -3.45, allDaysAfter: 1.6, avoided: -2.44, tStat: -0.66, fallRate: 71.4, baselineFallRate: 45.7,
      meanDrawdown: -11, baselineDrawdown: -8.8, verdict: "insufficient", verdictLabel: "Trop peu de ventes pour conclure",
    };
    expect(n(sellText(s))).toContain("cours ensuite supérieur de 2,44 points");
    expect(n(sellText({ ...s, avoided: 1.2 }))).toContain("cours ensuite inférieur de 1,2 point");
    expect(sellText({ ...s, signals: 0 })).toBe("Aucune vente pendant les périodes de test.");
    expect(buyText({ ...report.groups[0]!.buy, signals: 0 })).toBe("Aucun achat pendant les périodes de test.");
    expect(waitText(report.groups[0]!.wait)).toStartWith("ATTENDRE");
  });
});

describe("decision line", () => {
  const view: BotView = {
    available: true, group: "crypto", groupLabel: "Cryptos", inBasket: true, action: "wait", actionLabel: "ATTENDRE",
    up: 46.3, down: 52, thresholdUp: 49.7, thresholdDown: 58.4, baseUp: 44.7, baseDown: 53.4, buyVerdict: "unproven", sellVerdict: "insufficient",
    counts: false, time: 1, contributions: [{ id: "rsi14", label: "RSI 14", value: 40, valueText: "40", weight: -0.2, effect: "down", text: "RSI 14 : 40 (pèse contre la hausse)" }],
    text: "ATTENDRE : …", note: "Le bot n'a pas démontré d'avantage hors échantillon : il ne compte pas dans la décision.", asOf: 1, link: "/app/bot",
  };
  test("action, probabilities and whether it counts", () => {
    expect(isBotView(view)).toBe(true);
    expect(isBotView({ available: true })).toBe(false);
    expect(n(botSummary(view))).toBe("ATTENDRE · hausse 46 %, baisse 52 %");
    const h = renderToStaticMarkup(createElement(BotLine, { b: view }));
    expect(h).toContain("Bot Altim");
    expect(h).toContain("ne compte pas");
    expect(h).toContain("il ne compte pas dans la décision");
    expect(h).toContain("RSI 14 : 40");
    expect(ACTION_UI.sell.label).toBe("VENDRE");
    const na = renderToStaticMarkup(createElement(BotLine, { b: { ...view, available: false, action: null, contributions: [], text: "Bot pas encore entraîné" } }));
    expect(na).toContain("Bot pas encore entraîné");
    expect(na).not.toContain("ne compte pas<");
  });
});
