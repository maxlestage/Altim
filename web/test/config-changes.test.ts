import { describe, expect, test } from "bun:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import btcRaw from "../../backend/tests/samples/decision-btc.json";
import { cacheDecision, parseDecision, type Decision, type Track } from "../src/webapp/decision";
import {
  applyDecision, MAX_TRANSITIONS, parseState, readConfigState, recordConfiguration, snapshotOf, transitionTitle, type ConfigState,
} from "../src/webapp/config-changes";
import { parseDangers, readDangers, saveDangers } from "../src/webapp/danger-store";
import { TrackDetails } from "../src/webapp/TrackDetails";

const btc = (): Decision => parseDecision(JSON.parse(JSON.stringify(btcRaw)));
const memory = () => {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), m };
};
const empty = (): ConfigState => ({ version: 1, last: {}, transitions: [] });

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
    expect(parseState(JSON.stringify({ version: 2, last: {}, transitions: [] }))).toEqual(empty());
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
