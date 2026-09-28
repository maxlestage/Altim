import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import btcRaw from "../../backend/tests/samples/decision-btc.json";
import { cacheDecision, NNBSP, parseDecision, type Decision, type Track } from "../src/webapp/decision";
import {
  applyDecision, explainChange, latestChange, MAX_TRANSITIONS, parseState, readConfigState, recordConfiguration, snapshotOf, transitionTitle,
  type ConfigState, type SignalMetrics,
} from "../src/webapp/config-changes";
import { parseDangers, readDangers, saveDangers } from "../src/webapp/danger-store";
import { TrackDetails } from "../src/webapp/TrackDetails";

const btc = (): Decision => parseDecision(JSON.parse(JSON.stringify(btcRaw)));
const memory = () => {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), m };
};
const empty = (): ConfigState => ({ version: 2, last: {}, transitions: [] });

describe("configuration changes", () => {
  test("the first sighting is a baseline, a new verdict is a transition with the missing conditions", () => {
    const d = btc();
    const first = applyDecision(empty(), d, false, 1000);
    expect(first.transition).toBeNull();
    const next = { ...btc(), verdict: "buyZone", label: "ZONE D'ACHAT", level: "moderate", levelLabel: "Signal modéré" } as Decision;
    const second = applyDecision(first.state, next, false, 2000);
    const t = second.transition!;
    expect(t.from.verdict).toBe(d.verdict);
    expect(t.to.verdict).toBe("buyZone");
    expect(t.since).toBe(1000);
    expect(transitionTitle(t)).toBe(`🚨 BTC — changement de configuration : ${d.label} → ZONE D'ACHAT`);
    expect(t.missing).toEqual(snapshotOf(next, 0).missing);
    expect(t.missing.length).toBe(d.setup.steps.filter((s) => s.state !== "ok").length);
    expect(t.triggers.length).toBe(d.toBuy.length);
    // Same configuration again: nothing new.
    expect(applyDecision(second.state, next, false, 3000).transition).toBeNull();
    // Personal mode has its own baseline.
    expect(applyDecision(second.state, d, true, 3000).transition).toBeNull();
  });
  test("a level change alone is told with the levels", () => {
    const s = applyDecision(empty(), btc(), false, 1).state;
    const t = applyDecision(s, { ...btc(), level: "highRisk", levelLabel: "Risque élevé" } as Decision, false, 2).transition!;
    expect(transitionTitle(t)).toContain("→ Risque élevé");
  });
  test("at most 50 transitions are kept, newest first", () => {
    let s = empty();
    for (let i = 0; i < 60; i++) s = applyDecision(s, { ...btc(), verdict: i % 2 ? "wait" : "buy" } as Decision, false, i).state;
    expect(s.transitions.length).toBe(MAX_TRANSITIONS);
    expect(s.transitions[0]!.at).toBe(59);
  });
  test("damaged storage is dropped, bad entries filtered", () => {
    expect(parseState("{nope")).toEqual(empty());
    expect(parseState(JSON.stringify({ version: 3, last: {}, transitions: [] }))).toEqual(empty());
    const good = applyDecision(applyDecision(empty(), btc(), false, 1).state, { ...btc(), verdict: "sell" } as Decision, false, 2).state;
    const raw = JSON.stringify({ ...good, last: { ...good.last, bad: { verdict: "hodl" } }, transitions: [...good.transitions, { symbol: 1 }] });
    const p = parseState(raw);
    expect(Object.keys(p.last)).toEqual(Object.keys(good.last));
    expect(p.transitions.length).toBe(1);
  });
  test("every cached decision goes through the diff", () => {
    const s = memory();
    cacheDecision(btc(), false, 1, s);
    cacheDecision({ ...btc(), verdict: "buy", label: "ACHETER" } as Decision, false, 2, s);
    expect(readConfigState(s).transitions.map((t) => t.to.verdict)).toEqual(["buy"]);
    expect(recordConfiguration(btc(), false, 3, s)?.to.verdict).toBe(btc().verdict);
  });
});

describe("why the signal changed", () => {
  const m = (over: Partial<SignalMetrics> = {}): SignalMetrics => ({
    price: 342, composite: 24,
    families: [{ key: "trend", label: "Tendance", score: 50 }, { key: "momentum", label: "Momentum", score: 40 }, { key: "news", label: "Actualités", score: 0 }],
    relVolume: 1.4, rsi: 62, support: { price: 335.61, touches: 2 }, resistance: { price: 344.95, touches: 2 }, newsScore: 0, topNews: null, ...over,
  });
  test("every measured change is told, most telling first", () => {
    const next = m({
      composite: 6,
      families: [{ key: "trend", label: "Tendance", score: 45 }, { key: "momentum", label: "Momentum", score: 22 }, { key: "news", label: "Actualités", score: null }],
      relVolume: 0.7, rsi: 48, resistance: { price: 344.9, touches: 3 },
      topNews: { title: "Apple cuts iPhone orders", tone: "negative" },
    });
    expect(explainChange(m(), next)).toEqual([
      "Score composite +24 → +6",
      "Momentum −18 pts (+40 → +22)",
      "Actualités : plus mesuré(e) (données indisponibles)",
      "Volume en baisse (1,4× → 0,7× la moyenne)",
      "RSI 62 → 48",
      `Résistance 344,90${NNBSP}$ confirmée (touchée 3 fois)`,
      "Actualité négative : « Apple cuts iPhone orders »",
    ]);
  });
  test("levels broken or crossed, small moves ignored", () => {
    expect(explainChange(m(), m({ price: 330, composite: 22, rsi: 58 }))).toEqual([`Support 335,61${NNBSP}$ cassé (prix 330${NNBSP}$)`]);
    expect(explainChange(m(), m({ price: 346, resistance: { price: 360, touches: 2 } }))).toEqual([`Résistance 344,95${NNBSP}$ franchie (prix 346${NNBSP}$)`]);
    expect(explainChange(m(), m())).toEqual([]);
  });
  test("the transition carries the explanation; a rating change alone is a transition", () => {
    const d = btc();
    const score = (value: number) => ({ value, label: "", factors: [], missing: [], custom: false, text: "" });
    const s = applyDecision(empty(), { ...d, rating: "buy", ratingLabel: "ACHAT", score: score(24) } as Decision, false, 1).state;
    expect(s.last[Object.keys(s.last)[0]!]!.metrics?.families.length).toBe(d.families.length);
    const t = applyDecision(s, { ...d, rating: "hold", ratingLabel: "ATTENDRE", score: score(-30) } as Decision, false, 2).transition!;
    expect(transitionTitle(t)).toBe("🚨 BTC — changement de configuration : note ACHAT → note ATTENDRE");
    // Older decision without `snapshot`: the measurements are read from the decision itself.
    expect(t.changes).toEqual(["Score composite +24 → −30"]);
  });
  test("version 1 storage is migrated, damaged measurements are dropped", () => {
    const v1 = {
      version: 1,
      last: { "crypto:BTC:i": { verdict: "wait", level: "waiting", label: "ATTENDRE", levelLabel: "Attente", missing: [], triggers: [], at: 1 } },
      transitions: [{ symbol: "BTC", kind: "crypto", name: "Bitcoin", personal: false, at: 2, since: 1, from: { verdict: "buy", level: "moderate", label: "ACHETER", levelLabel: "Signal modéré" }, to: { verdict: "wait", level: "waiting", label: "ATTENDRE", levelLabel: "Attente" }, missing: [], triggers: [] }],
    };
    const p = parseState(JSON.stringify(v1));
    expect(p.version).toBe(2);
    expect(p.last["crypto:BTC:i"]?.metrics).toBeUndefined();
    expect(p.transitions[0]?.changes).toEqual([]);
    const bad = { ...v1, version: 2, last: { "crypto:BTC:i": { ...v1.last["crypto:BTC:i"], metrics: { price: "x" } } } };
    const q = parseState(JSON.stringify(bad));
    expect(Object.keys(q.last)).toEqual(["crypto:BTC:i"]);
    expect(q.last["crypto:BTC:i"]?.metrics).toBeUndefined();
    // A migrated baseline (no measurements) gives a transition without explanation, never a made-up one.
    const t = applyDecision(p, { ...btc(), verdict: "buy", label: "ACHETER" } as Decision, false, 3).transition!;
    expect(t.changes).toEqual([]);
  });
  test("the decision card finds the change that led to what it shows", () => {
    const d = btc();
    const s = applyDecision(empty(), d, false, 1).state;
    const next = { ...btc(), verdict: "buyZone", level: "moderate" } as Decision;
    const { state } = applyDecision(s, next, false, 2);
    expect(latestChange(state.transitions, next)?.since).toBe(1);
    expect(latestChange(state.transitions, d)).toBeNull();
    expect(latestChange(state.transitions, { ...next, mode: "personal" })).toBeNull();
  });
});

describe("dangerous positions kept for the Radar", () => {
  test("saved, read back, validated", () => {
    const s = memory();
    saveDangers([{ id: "1", symbol: "BTC", kind: "crypto", name: "Bitcoin", reasons: [{ code: "stop_broken", text: "Stop cassé" }] }], 42, s);
    expect(readDangers(s)?.items[0]?.symbol).toBe("BTC");
    expect(readDangers(s)?.at).toBe(42);
    expect(parseDangers(JSON.stringify({ version: 1, at: 1, items: [{ id: "x", reasons: [{ code: "boom" }] }] }))?.items).toEqual([]);
    expect(parseDangers("[]")).toBeNull();
  });
});

describe("track details", () => {
  const base: Track = {
    period: "p", trades: 12, winRate: 50, avgWin: 4, avgLoss: -2, profitFactor: 2, sharpe: 1, sortino: 1, maxDrawdown: -10, totalReturn: 20, buyAndHold: 30,
    feesPct: 0.1, slippagePct: 0.05, losingStreak: 2, note: "n",
  };
  test("older answers show nothing more", () => {
    expect(renderToStaticMarkup(createElement(TrackDetails, { track: base }))).toBe("");
  });
  test("expectancy, regimes with small samples, anti-bias notes and the tax assumption", () => {
    const html = renderToStaticMarkup(createElement(TrackDetails, {
      track: {
        ...base, spreadPct: 0.02, spreadMeasured: false, spreadNote: "Hypothèse : écart", expectancy: 1, avgR: 0.4, testedBars: 900,
        biasNotes: ["Paramètres fixes, ceux du signal en direct : aucune optimisation sur la période testée."],
        regimes: [
          { regime: "bull", label: "Marché haussier", trades: 9, winRate: 55.6, avgReturn: 1.2, lowSample: false },
          { regime: "crisis", label: "Crise", trades: 3, winRate: 33.3, avgReturn: -2, lowSample: true },
        ],
      },
    }));
    expect(html).toContain("Espérance par trade");
    expect(html).toContain("0,4 R");
    expect(html).toContain("échantillon trop faible");
    expect(html).toContain("aucune optimisation");
    expect(html).toContain("Hypothèse : écart");
    expect(html).toContain("flat tax de 30");
    expect(html).toContain("+14"); // 20 % × (1 − 30 %)
  });
});
