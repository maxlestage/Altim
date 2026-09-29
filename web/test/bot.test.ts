import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import raw from "../../backend/tests/samples/bot.json";
import rawV1 from "../../backend/tests/samples/bot-v1.json";
import {
  ACTION_UI, CANDIDATE_SHORT, anyEdge, botSummary, botUrl, botViewsUrl, buyText, calibrationRows, clusteredText, dataText, exitText, isBotView, points,
  selectionRuns, sellText, skillText, waitText, type BlockOut, type BotReport, type BotView, type SellStats,
} from "../src/webapp/model-bot";
import { AssetRow, BotReportView, Calibration, CandidatesCard, GroupCard } from "../src/webapp/Bot";
import { BotLine } from "../src/webapp/DecisionCard";

const report = raw as unknown as BotReport;
const v1 = rawV1 as unknown as BotReport;
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
      const m = g.model!;
      if (m.candidate === "v1" || m.candidate === "logit") expect(m.up.weights.length).toBe(m.candidate === "v1" ? 16 : report.features.length);
      expect(g.candidates!.map((c) => c.id)).toEqual(["v1", "logit", "trees", "trend"]);
      expect(g.candidates!.reduce((s, c) => s + c.chosenBlocks, 0)).toBe(g.trainedBlocks);
      expect(g.selection!.length).toBe(g.blocks);
      expect(g.buy.tStat).toBe(g.buy.clustered!.byDate);
      expect(g.universe!.extra).toBeGreaterThan(0);
      if (g.buy.meanNet != null && g.buy.baselineNet != null && g.buy.excess != null) expect(Math.abs(g.buy.meanNet - g.buy.baselineNet - g.buy.excess)).toBeLessThan(0.02);
    }
    expect(report.parameters.horizonDays).toBe(20);
    expect(report.version).toBe(2);
    expect(report.features.length).toBe(24);
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
    expect(renderToStaticMarkup(createElement(GroupCard, { g: report.groups[0]! }))).toContain(report.groups[0]!.buy.verdictLabel.replace(/'/g, "&#x27;"));
    const cal = renderToStaticMarkup(createElement(Calibration, { title: "t", buckets: report.groups[0]!.calibrationUp, skill: report.groups[0]!.brierSkillUp }));
    expect(cal).toContain("prévu");
    for (const t of ["Ce qui change avec la v2", "Chaque modèle seul", "non utilisé pour choisir", "12 derniers mois", "hors panier", "Modèle retenu à chaque réentraînement", "t par jour"]) {
      expect(h).toContain(t);
    }
    const cands = renderToStaticMarkup(createElement(CandidatesCard, { g: report.groups[0]! }));
    for (const c of report.groups[0]!.candidates!) expect(cands).toContain(c.label);
  });

  test("a v1 answer still renders (v2 parts left out)", () => {
    const h = renderToStaticMarkup(createElement(BotReportView, { report: v1 }));
    expect(h).toContain("Résultats hors échantillon");
    expect(h).not.toContain("Chaque modèle seul");
    expect(dataText(v1.groups[0]!)).toBeNull();
    expect(exitText(v1.groups[0]!.sell.exit)).toBeNull();
    expect(n(buyText(v1.groups[0]!.buy))).toContain("(−0,15 point, t = −0,1)");
  });
});

describe("v2 texts", () => {
  test("clustered t, exits, data, selection runs", () => {
    expect(clusteredText({ byDate: -2.4, dates: 1067, byAsset: -4.46, assets: 22, perSignal: -2.84 }, -2.4)).toBe("t par jour −2,4 (1067 jours) · par actif −4,5 · par signal −2,8");
    expect(clusteredText(undefined, 1.25)).toBe("t = 1,3");
    const e = exitText({ assets: 22, outShare: 15.5, medianHoldMaxDrawdown: -46.13, medianBotMaxDrawdown: -47.93, medianDrawdownAvoided: -0.15, medianHoldReturn: 647.46, medianBotReturn: 289.42, beatHold: 1 });
    expect(n(e!)).toBe("En sortant 20 jours à chaque VENDRE : hors marché 16 % du temps ; pire baisse médiane −47,9 % contre −46,1 % en gardant ; rendement médian +289 % contre +647 % (mieux que garder : 1 actif sur 22).");
    expect(exitText({ assets: 0, outShare: null, medianHoldMaxDrawdown: null, medianBotMaxDrawdown: null, medianDrawdownAvoided: null, medianHoldReturn: null, medianBotReturn: null, beatHold: 0 })).toBeNull();
    const g = report.groups[0]!;
    expect(dataText(g)).toContain(`${g.universe!.basket} actifs du panier et ${g.universe!.extra} de plus à l'entraînement`);
    const b = (start: number, end: number | null, chosen: BlockOut["chosen"]): BlockOut => ({ start, end, trainRows: 1, chosen, scores: [] });
    expect(selectionRuns([b(1, 2, "trend"), b(2, 3, "trend"), b(3, null, "v1"), b(4, null, null)])).toEqual([
      { from: 1, to: 2, chosen: "trend", count: 2 },
      { from: 3, to: 3, chosen: "v1", count: 1 },
      { from: 4, to: 4, chosen: null, count: 1 },
    ]);
    expect(CANDIDATE_SHORT.trees).toBe("Arbres");
    expect(n(buyText(g.buy))).toContain("t par jour =");
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
