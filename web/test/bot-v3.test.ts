import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import raw from "../../backend/tests/samples/bot.json";
import rawV2 from "../../backend/tests/samples/bot-v2.json";
import {
  computeText, confirmed, pendingText, forwardSignals, forwardText, frIso, headlineConfigs, judged, proven, tLine, tVsRequired, v3AnyEdge, v3SideText, v3VerdictLabel, volRows,
  type BotReport, type ConfigStats, type SideStats,
} from "../src/webapp/model-bot";
import { BotReportView } from "../src/webapp/Bot";
import { BotSection } from "../src/components/BotSection";
import { BotRadarView, firstSentence } from "../src/webapp/BotRadarCard";

const report = raw as unknown as BotReport;
const v2 = rawV2 as unknown as BotReport;
const v3 = report.v3!;
/** Narrow no-break spaces (before %) read as plain spaces in the expectations. */
const n = (s: string) => s.replace(/ /g, " ");
const esc = (s: string) => s.replace(/'/g, "&#x27;").replace(/"/g, "&quot;");

describe("v3 contract sample (real /api/bot answer)", () => {
  test("pre-registration, corrected threshold, groups × horizons × configurations", () => {
    expect(report.version).toBe(3);
    expect(v3.preregDate).toBe("2026-09-30");
    expect(v3.k).toEqual({ v1: 4, v2: 20, v3: 90, total: 114 });
    expect(Math.abs(v3.tRequired - 3.5157)).toBeLessThan(0.001);
    expect(v3.parameters.horizons).toEqual([20, 60]);
    expect(v3.groups.map((g) => g.id)).toEqual(["stock", "crypto"]);
    for (const g of v3.groups) {
      expect(g.horizons.map((h) => h.horizon)).toEqual([20, 60]);
      for (const h of g.horizons) {
        expect(h.configs.map((c) => c.id)).toEqual(["absolute", "peers", "v2", "trend", "v1", "logit", "trees", "treesLong", "xsMomentum", "xsLogit", "xsTrees"]);
        expect(h.selection.length).toBe(h.blocks);
        for (const c of h.configs) {
          // Family B also judged against the group's mean (control listed as added after the pre-registration).
          expect(c.main.buyVsMean != null && c.main.sellVsMean != null).toBe(c.family === "peers");
          if (proven(c.main, "buy")) expect(judged(c.main, "buy").t!).toBeGreaterThanOrEqual(v3.tRequired);
          // No verdict "edge" under the corrected threshold (and never with fewer than 30 signals).
          for (const s of [c.main.buy, c.main.sell]) {
            if (s.verdict === "edge") expect(s.t!).toBeGreaterThanOrEqual(v3.tRequired);
            if (s.signals < 30) expect(s.verdict).toBe("insufficient");
          }
        }
      }
      expect(headlineConfigs(g).length).toBe(4);
    }
    // The headline and the tiles come from the numbers.
    expect(report.headline).toBe(v3.headline);
    expect(v3.afterPrereg.length).toBe(2);
    expect(v3.afterPrereg[0]).toContain("moyenne à parts égales");
    expect(v3.afterPrereg[1]).toContain("au moins 30 signaux");
    // The one side proven on the past (stocks, peers, 20 days, buys) waits for the forward test: shown, not counted.
    const peers20 = v3.groups[0]!.horizons[0]!.configs.find((c) => c.id === "peers")!;
    expect(proven(peers20.main, "buy")).toBe(true);
    expect(confirmed(peers20, "buy")).toBe(false);
    expect(n(pendingText(peers20, "buy", v3.tRequired)!)).toBe(
      "Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé) mais fragile : il ne comptera qu'après 30 signaux sur l'avenir qui le confirment (0 à ce jour).",
    );
    expect(v3.headline).toContain("mais fragile");
    expect(v3.headline).toContain("le bot ne pèse pas dans les décisions");
    expect(v3.headline).toStartWith("Bot v3 : ");
    expect(v3.headline.includes("aucun avantage démontré")).toBe(!v3AnyEdge(v3));
    expect(forwardSignals(v3)).toBeGreaterThanOrEqual(0);
    // Today's actions of the basket assets under the 4 headline configurations.
    expect(report.assets.every((a) => a.v3 != null)).toBe(true);
  });

  test("the screen renders v3 first, then v2's selection as the reference", () => {
    const h = n(renderToStaticMarkup(createElement(BotReportView, { report })));
    for (const t of [
      "Bot v3 · pré-enregistré le 30/09/2026", "Seuil corrigé", "Résultats v3 hors échantillon", "Depuis le 30/09/2026 (test sur l&#x27;avenir)",
      "Chaque configuration seule", "Tendance à volatilité gérée", "Comment la v3 est jugée", "Référence : sélection v2 à 20 jours", "Classement entre pairs",
      "Hausse ou baisse de l&#x27;actif", "requis 3,52", "mais fragile : il ne comptera qu&#x27;après 30 signaux",
    ]) expect(h).toContain(t);
    expect(h).toContain(esc(n(v3.forwardHeadline)));
    expect(h).not.toContain("Ce qui change avec la v2");
  });

  test("a v2 answer still renders without the v3 parts", () => {
    const h = renderToStaticMarkup(createElement(BotReportView, { report: v2 }));
    expect(v2.v3).toBeUndefined();
    expect(h).toContain("Ce qui change avec la v2");
    expect(h).not.toContain("Bot v3");
  });

  test("the site section loads live (nothing hard-coded) and links to the screen", () => {
    const h = renderToStaticMarkup(createElement(BotSection));
    expect(h).toContain('id="bot"');
    expect(h).toContain("Chargement des résultats");
    expect(h).toContain('href="/app/bot"');
    expect(h).toContain("Voir le détail");
    expect(h).not.toContain("t ≥ 3,52");
  });
});

describe("v3 texts", () => {
  const side = (o: Partial<SideStats>): SideStats => ({
    signals: 120, mean: 1, baseline: 0.8, excess: 0.2, t: 1.24, dates: 100, tByAsset: 0.9, assets: 20, tPerSignal: 1.5, beatShare: 52, verdict: "unproven", rawVerdict: "unproven", ...o,
  });
  test("raw vs required t, verdicts, sides", () => {
    expect(tVsRequired(1.24, 3.5157)).toBe("t = 1,2 (requis 3,52)");
    expect(tVsRequired(null, 3.5157)).toBe("t = — (requis 3,52)");
    expect(v3VerdictLabel(side({}), 3.52)).toBe("Non démontré");
    expect(v3VerdictLabel(side({ t: 2.4, rawVerdict: "edge" }), 3.52)).toBe("t ≥ 2 atteint, pas le seuil corrigé : non démontré");
    expect(v3VerdictLabel(side({ verdict: "edge", t: 4 }), 3.5157)).toBe("Avantage au seuil corrigé (t ≥ 3,52), à confirmer");
    expect(v3VerdictLabel(side({ verdict: "insufficient", signals: 3 }), 3.52)).toBe("Trop peu de signaux pour conclure");
    expect(n(v3SideText("peers", "buy", side({}), 3.5157))).toBe("120 achats : +0,2 point face à la médiane du groupe (t par jour 1,2 (requis 3,52) · par actif 0,9).");
    expect(n(v3SideText("absolute", "sell", side({ excess: -2.5, t: -4.51, tByAsset: null }), 3.5157))).toBe("120 ventes : baisse évitée −2,5 points (t par jour −4,5 (requis 3,52) · par actif —).");
    expect(tLine(side({ t: 3.65, tByAsset: 0.62 }), 3.5157)).toBe("t par jour 3,7 (requis 3,52) · par actif 0,6");
    expect(v3SideText("peers", "sell", side({ signals: 0 }), 3.5)).toBe("Aucune vente.");
  });
  test("forward test, managed trend, compute, dates", () => {
    const empty = { buy: side({ signals: 0 }), sell: side({ signals: 0 }) } as ConfigStats;
    expect(forwardText(empty, 3.5)).toStartWith("Aucun signal jugé pour l'instant");
    expect(frIso("2026-09-30")).toBe("30/09/2026");
    const g = v3.groups[0]!;
    expect(volRows(g.volManaged!).map((r) => r.label)).toEqual(["Ratio de Sharpe", "Pire baisse", "Rendement annuel", "Volatilité annuelle"]);
    expect(computeText({ fetchMs: 15_400, computeMs: 432_000, threads: 1, peakRssMb: 243.4 })).toBe("7 min 12 s de calcul sur 1 cœur, pic mémoire 243 Mo (téléchargement 15 s)");
    expect(computeText(null)).toBeNull();
  });
});

describe("Radar card", () => {
  test("first sentence of the live headline, forward counter, link; honest states", () => {
    const h = n(renderToStaticMarkup(createElement(BotRadarView, { state: "ready", report })));
    expect(h).toContain(esc(firstSentence(v3.headline)));
    expect(firstSentence(v3.headline).endsWith("le bot ne pèse pas dans les décisions.")).toBe(true);
    expect(h).toContain("Test sur l&#x27;avenir : 0 signal sur 30");
    expect(h).toContain('href="/app/bot"');
    expect(renderToStaticMarkup(createElement(BotRadarView, { state: "error", report: null }))).toContain("Résultats indisponibles");
    expect(renderToStaticMarkup(createElement(BotRadarView, { state: "pending", report: null }))).toContain("Entraînement et test en cours");
    expect(firstSentence("A. B. C")).toBe("A.");
  });
});
