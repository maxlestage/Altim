import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import btcRaw from "../../backend/tests/samples/decision-btc.json";
import aaplRaw from "../../backend/tests/samples/decision-aapl.json";
import {
  averageCost, cacheDecision, cachedDecision, count, decisionUrl, exitText, familyTone, hashRate, longDate, NNBSP, num, parseDecision, pct,
  portfolioWeights, recentVerdict, riskRewardText, sortVetoes, summaryFamilies, usd, usdCompact, weightsParam, type Decision,
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
