import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import btcRaw from "../../backend/tests/samples/decision-btc.json";
import aaplRaw from "../../backend/tests/samples/decision-aapl.json";
import {
  averageCost, cacheDecision, cachedDecision, count, decisionUrl, DEFAULT_SCORE_WEIGHTS, exitText, familyTone, hashRate, longDate, NNBSP, num,
  parseDecision, pct, portfolioWeights, recentVerdict, riskRewardText, sanitizeScoreWeights, scoreWeightsParam, signedScore, sortVetoes,
  summaryFamilies, usd, usdCompact, weightsParam, type Decision, type Structure,
} from "../src/webapp/decision";
import { DecisionView } from "../src/webapp/DecisionCard";

const N = NNBSP;
const clone = <T>(x: T): T => JSON.parse(JSON.stringify(x));
const btc = () => parseDecision(clone(btcRaw));
const aapl = () => parseDecision(clone(aaplRaw));

describe("contract samples decode through the card's own types", () => {
  test("BTC: informational, not held, ATTENDRE", () => {
    const d = btc();
    expect(d.verdict).toBe("wait");
    expect(d.level).toBe("waiting");
    expect(d.mode).toBe("informational");
    expect(d.position).toBeNull();
    expect(d.fundamentals?.kind).toBe("crypto");
    expect(d.plan?.acceptable).toBe(false);
  });
  test("AAPL: personal, held, ALLÉGER, progressive exits and exposure", () => {
    const d = aapl();
    expect(d.verdict).toBe("trim");
    expect(d.mode).toBe("personal");
    expect(d.position?.exits.filter((e) => e.now).length).toBe(1);
    expect(d.exposure?.warning).toContain("62");
    const f = d.fundamentals;
    if (f?.kind !== "stock") throw new Error("stock fundamentals expected");
    expect(f.nextEarnings?.estimated).toBe(true);
  });
  test("an answer outside the contract is refused with the field named", () => {
    expect(() => parseDecision({ ...clone(btcRaw), verdict: "hodl" })).toThrow("verdict");
    expect(() => parseDecision({ ...clone(btcRaw), level: "red" })).toThrow("level");
    expect(() => parseDecision({ ...clone(btcRaw), fundamentals: { kind: "bond" } })).toThrow("fundamentals.kind");
    expect(() => parseDecision({ ...clone(btcRaw), disclaimer: "" })).toThrow("disclaimer");
    expect(() => parseDecision(null)).toThrow();
  });
});

describe("French formatting", () => {
  test("prices, amounts, percentages with narrow no-break spaces", () => {
    expect(usd(80100)).toBe(`80${N}100${N}$`);
    expect(usd(340.99)).toBe(`340,99${N}$`);
    expect(usd(null)).toBe("—");
    expect(usdCompact(421e9)).toBe(`421${N}Md$`);
    expect(usdCompact(3.16e9)).toBe(`3,16${N}Md$`);
    expect(usdCompact(1654e9)).toBe(`1${N}654${N}Md$`);
    expect(pct(-2.6, 1, true)).toBe(`−2,6${N}%`);
    expect(pct(7.9, 1, true)).toBe(`+7,9${N}%`);
    expect(pct(0.0069, 4, true)).toBe(`+0,0069${N}%`);
    expect(num(1.2, 1)).toBe("1,2");
    expect(count(19_930_000)).toBe("19,93 millions");
    expect(hashRate(1.069e21)).toBe(`1${N}069${N}EH/s`);
    expect(longDate(Date.UTC(2026, 8, 28, 12))).toBe("28 septembre 2026");
  });
  test("decision texts", () => {
    const d = aapl();
    expect(exitText(d.position!.exits[0]!)).toBe(`Vendre 20${N}% si objectif 1 atteint (338 $)`);
    expect(riskRewardText(btc().plan!)).toBe("1,2 (minimum 2)");
  });
  test("levels and families: an icon and a word, never colour alone", () => {
    expect(familyTone({ status: "negative", score: -55 })).toEqual({ tone: "exit", icon: "🔴", text: "très défavorable" });
    expect(familyTone({ status: "negative", score: -20 }).text).toBe("défavorable");
    expect(familyTone({ status: "unavailable", score: null }).text).toBe("non disponible");
    const names = summaryFamilies(btc().families).map((x) => x.name);
    expect(names).toEqual(["Tendance", "Momentum", "Volume", "Fondamentaux", "Macro", "Risque", "Valorisation"]);
  });
  test("vetoes: active first, then passed checks, then unverifiable", () => {
    const v = sortVetoes(btc().vetoes);
    expect(v[0]!.code).toBe("riskReward");
    expect(v[v.length - 1]!.verifiable).toBe(false);
  });
});

describe("personal mode inputs", () => {
  const holdings = [
    { symbol: "BTC", kind: "crypto" as const, quantity: 0.5, averagePrice: 60_000 },
    { symbol: "AAPL", kind: "stock" as const, quantity: 100, averagePrice: 275 },
    { symbol: "AAPL", kind: "stock" as const, quantity: 100, averagePrice: 325 },
    { symbol: "XYZ", kind: "stock" as const, quantity: 10, averagePrice: 5 },
  ];
  const prices = { "crypto:BTC": 80_000, "stock:AAPL": 150 };
  test("weights of the total value (cash included), merged lines, unpriced lines left out", () => {
    const w = portfolioWeights(holdings, prices, 30_000);
    // BTC 40 000, AAPL 200 × 150 = 30 000, cash 30 000 → total 100 000.
    expect(w).toEqual([
      { symbol: "BTC", kind: "crypto", weight: 40 },
      { symbol: "AAPL", kind: "stock", weight: 30 },
    ]);
    expect(weightsParam(w)).toBe("BTC:crypto:40,AAPL:stock:30");
  });
  test("at most 20 weights, the largest ones", () => {
    const many = Array.from({ length: 25 }, (_, i) => ({ symbol: `S${i}`, kind: "stock" as const, quantity: i + 1 }));
    const p = Object.fromEntries(many.map((h) => [`stock:${h.symbol}`, 10]));
    const w = portfolioWeights(many, p);
    expect(w.length).toBe(20);
    expect(w[0]!.symbol).toBe("S24");
    expect(portfolioWeights([], {})).toEqual([]);
  });
  test("average cost over every line of the asset", () => {
    expect(averageCost(holdings, "AAPL", "stock")).toBe(300);
    expect(averageCost(holdings, "ETH", "crypto")).toBeNull();
  });
  test("URL carries the cost and weights only, never quantities nor amounts", () => {
    expect(decisionUrl("BTC", "crypto")).toBe("/api/decision?symbol=BTC&kind=crypto");
    const u = decisionUrl("AAPL", "stock", { cost: 275.456, weights: [{ symbol: "AAPL", kind: "stock", weight: 10 }, { symbol: "BTC", kind: "crypto", weight: 35.2 }] });
    expect(u).toBe(`/api/decision?symbol=AAPL&kind=stock&cost=275.46&weights=${encodeURIComponent("AAPL:stock:10,BTC:crypto:35.2")}`);
  });
});

describe("offline cache", () => {
  const mem = () => {
    const m = new Map<string, string>();
    return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v) };
  };
  test("last decision kept per asset, recent verdict for the Radar", () => {
    const s = mem();
    const t = 1_790_612_000_000;
    cacheDecision(btc(), false, t, s);
    expect(cachedDecision("crypto", "BTC", s)?.decision.label).toBe("ATTENDRE");
    expect(recentVerdict("crypto", "BTC", t + 3_600_000, undefined, s)?.label).toBe("ATTENDRE");
    expect(recentVerdict("crypto", "BTC", t + 13 * 3_600_000, undefined, s)).toBeNull();
    expect(cachedDecision("stock", "AAPL", s)).toBeNull();
  });
});

describe("card render (static markup)", () => {
  const html = (d: Decision) => renderToStaticMarkup(createElement(DecisionView, { d }));
  test("BTC card", () => {
    const h = html(btc());
    for (const s of ["ATTENDRE", "Confiance du modèle", "67/100", "Mode informationnel", "Pourquoi attendre ?", "Pour passer en ACHAT", "Pour passer en VENTE",
      "Interdictions d&#x27;achat", "non vérifiable", "ACTIVE", "non disponible", "Fondamentaux du réseau", "Profit factor"]) {
      expect(h).toContain(s);
    }
    expect(h).toContain("Altim ne passe aucun ordre");
    expect(h.indexOf("dec-disclaimer")).toBeGreaterThan(h.indexOf("Sources"));
  });
  test("AAPL card", () => {
    const h = html(aapl());
    for (const s of ["ALLÉGER", "Mode personnel", "Votre position", "maintenant", "date estimée", "62", "PEG", "EV/EBITDA", "Fondamentaux de l&#x27;entreprise"]) {
      expect(h).toContain(s);
    }
    expect(h).not.toContain("Pourquoi attendre ?");
  });
});

describe("composite score weights", () => {
  test("defaults are not sent; custom weights are, as whole numbers 0 – 100", () => {
    expect(scoreWeightsParam(DEFAULT_SCORE_WEIGHTS)).toBeNull();
    expect(scoreWeightsParam(null)).toBeNull();
    const w = { ...DEFAULT_SCORE_WEIGHTS, tech: 50, macro: 0 };
    expect(scoreWeightsParam(w)).toBe("tech:50,mom:18,fund:20,sent:10,news:10,macro:0");
    expect(decisionUrl("BTC", "crypto", null, w)).toBe(`/api/decision?symbol=BTC&kind=crypto&w=${encodeURIComponent("tech:50,mom:18,fund:20,sent:10,news:10,macro:0")}`);
    expect(decisionUrl("BTC", "crypto", null, DEFAULT_SCORE_WEIGHTS)).toBe("/api/decision?symbol=BTC&kind=crypto");
  });
  test("stored values are cleaned; all at 0 falls back on the defaults", () => {
    expect(sanitizeScoreWeights({ tech: 140, mom: -3, fund: 12.6, sent: "x" })).toEqual({ tech: 100, mom: 0, fund: 13, sent: 10, news: 10, macro: 10 });
    expect(sanitizeScoreWeights(undefined)).toEqual(DEFAULT_SCORE_WEIGHTS);
    expect(sanitizeScoreWeights({ tech: 0, mom: 0, fund: 0, sent: 0, news: 0, macro: 0 })).toEqual(DEFAULT_SCORE_WEIGHTS);
    expect(signedScore(34.4)).toBe("+34");
    expect(signedScore(-12)).toBe("−12");
  });
});

describe("rating, score, degraded signal and structure", () => {
  const structure: Structure = {
    timeframe: "Bougies journalières clôturées", score: 40,
    ichimoku: { tenkan: 173, kijun: 164.5, senkouA: 142.75, senkouB: 125.5, futureA: 168.75, futureB: 150, position: "above", tkCross: null, tkCrossBars: null, bias: "bullish", reading: "Prix au-dessus du nuage Ichimoku, Tenkan au-dessus de la Kijun" },
    supertrend: { direction: "up", level: 160, bars: 6, bias: "bullish", reading: "Supertrend haussier depuis 6 bougies (niveau 160,00 $)" },
    donchian: null,
    vwap: { value: 170, bars: 20, deviationPct: 2, bias: "bullish", reading: "Prix au-dessus du VWAP glissant sur 20 bougies" },
    volumeProfile: { poc: 150, valueAreaHigh: 160, valueAreaLow: 140, bars: 120, bins: 24, bias: "bullish", reading: "Prix au-dessus de la zone de valeur", note: "Approximation à partir des bougies : …" },
    pivots: { pivot: 170, r1: 175, r2: 180, s1: 165, s2: 160, from: 0, reading: "Prix entre le pivot et R1" },
    levels: [{ price: 180, touches: 3, kind: "resistance", distancePct: 3.2 }], nearestSupport: null,
    nearestResistance: { price: 180, touches: 3, kind: "resistance", distancePct: 3.2 }, levelsReading: "Résistance la plus proche 180,00 $ (3 contacts, +3,2 %)",
    breakout: { kind: "fake", side: "up", level: 180, volumeRatio: 1.1, bias: "bearish", reading: "Fausse cassure : la résistance 180,00 $ a été dépassée puis le prix a refermé en dessous" },
    marketStructure: null,
    relative: [{ benchmark: "S&P 500 (SPY)", symbol: "SPY", periods: [{ label: "1 mois", days: 30, assetPct: 5, benchmarkPct: 2, diff: 3 }], correlation: 0.4, bias: "neutral", reading: "En ligne avec le S&P 500 (SPY)" }],
    relativeNote: null,
  };
  const full = (): Decision => parseDecision({
    ...clone(btcRaw),
    rating: "strongBuy", ratingLabel: "ACHAT FORT",
    score: {
      value: 34, label: "Plutôt favorable", missing: ["Fondamentaux"], custom: false, text: "Score composite +34 sur 5 facteur(s) mesuré(s).",
      factors: [
        { key: "tech", label: "Technique", weight: 32, applied: 40, value: 20, contribution: 8, sources: ["trend", "structure"] },
        { key: "fund", label: "Fondamentaux", weight: 20, applied: 0, value: null, contribution: null, sources: [] },
      ],
    },
    degraded: { active: true, headline: "⚠️ Signal dégradé — Le modèle détecte des signaux contradictoires. Aucune entrée privilégiée actuellement.", reasons: ["Familles contradictoires"] },
    marketRegime: { kind: "riskOn", label: "Risk-on", benchmark: "S&P 500", reasons: ["Stress macro 10/100 (calme)"] },
    horizon: { kind: "swing", label: "Swing", atrDistance: 3.7, detail: "Plan sur bougies journalières, objectif 1 à 3,7 ATR de l'entrée : swing" },
    plan: { ...clone(btcRaw).plan, target3: 123456, reward3Pct: 42, target3Source: "Niveau touché 3 fois au-dessus de l'objectif 2" },
    structure,
  });
  test("added fields decode, and are checked when present", () => {
    expect(full().rating).toBe("strongBuy");
    expect(() => parseDecision({ ...clone(btcRaw), rating: "moon" })).toThrow("rating");
    expect(() => parseDecision({ ...clone(btcRaw), structure: { levels: null } })).toThrow("structure");
    // Older answers (no added field) still decode.
    expect(btc().rating).toBeUndefined();
  });
  test("card: rating first, degraded banner, score bars, target 3, horizon, structure section", () => {
    const h = renderToStaticMarkup(createElement(DecisionView, { d: full() }));
    for (const s of ["ACHAT FORT", "Verdict du plan", "Signal dégradé", "Familles contradictoires", "Score composite", "+34", "non mesuré", "Objectif 3",
      "123", "Swing", "Structure technique", "Ichimoku (9, 26, 52)", "Supertrend haussier depuis 6 bougies", "Canal de Donchian (20)",
      "Non disponible : historique trop court", "Profil de volume (approximation)", "Approximation à partir des bougies", "Fausse cassure",
      "Force relative contre S&amp;P 500 (SPY)", "Risk-on", "haussier"]) {
      expect(h).toContain(s);
    }
    expect(h.indexOf("ACHAT FORT")).toBeLessThan(h.indexOf("Verdict du plan"));
  });
});

describe("« Preuve du modèle »", () => {
  const evidence = {
    available: true, assetClass: "stock", classLabel: "Actions et ETF américains", classVerdict: "edge",
    classVerdictLabel: "Gain moyen positif (t ≥ 2), à confirmer", assets: 22, beatHoldCount: 2, beatHold: "2/22", trades: 650, tStat: 2.18,
    regime: "range", regimeLabel: "marché sans tendance", regimeVerdict: "edge", regimeTrades: 88, regimeTStat: 2.69, weak: true,
    text: "Sur les actions et ETF testés (22), gain moyen positif par trade (t = 2,2) mais la simple détention a fait mieux dans 20 cas sur 22.",
    asOf: Date.UTC(2026, 8, 29, 8), link: "/app/validation",
  } as const;
  test("shown next to the confidence, with the link to the validation screen", () => {
    const d = { ...btc(), modelEvidence: { ...evidence }, ratingReason: "ACHAT plutôt que ACHAT FORT : la validation du modèle…" };
    const h = renderToStaticMarkup(createElement(DecisionView, { d: parseDecision(clone(d)) }));
    expect(h).toContain("Preuve du modèle");
    expect(h).toContain("dec-proof weak");
    expect(h).toContain("bat la détention : 2/22");
    expect(h).toContain("la simple détention a fait mieux dans 20 cas sur 22");
    expect(h).toContain('href="/app/validation"');
    expect(h).toContain("ACHAT plutôt que ACHAT FORT");
    expect(h.indexOf("Preuve du modèle")).toBeGreaterThan(h.indexOf("Confiance du modèle"));
  });
  test("not computed yet: said plainly; older answers: nothing", () => {
    const na = { ...evidence, available: false, classVerdict: null, classVerdictLabel: null, beatHold: null, weak: false, asOf: null, text: "Validation pas encore calculée : …" };
    const h = renderToStaticMarkup(createElement(DecisionView, { d: parseDecision(clone({ ...btc(), modelEvidence: na })) }));
    expect(h).toContain("dec-proof na");
    expect(h).toContain("Validation pas encore calculée");
    expect(h).not.toContain("bat la détention");
    expect(renderToStaticMarkup(createElement(DecisionView, { d: btc() }))).not.toContain("Preuve du modèle");
    expect(() => parseDecision({ ...clone(btcRaw), modelEvidence: { available: "yes" } })).toThrow("modelEvidence");
  });
});
