import { describe, expect, test } from "bun:test";
import {
  addEntry, coherenceChecks, createEntry, DAY, emptyJournal, holdingChange, isJournalState, journalProfile, marketFromCandles, patchEntry, removeEntry,
  reviewEntry, snapshotDecision, type JournalEntry,
} from "../src/engine/journal";
import type { Candle } from "../src/engine/signal";
import type { Decision } from "../src/webapp/decision";

const T0 = Date.UTC(2026, 5, 1);
const AT = T0 + 10 * 3_600_000; // entry at 10:00 UTC on day 0

/** A decision with only what the journal reads; `over` replaces fields. */
function decision(over: Partial<Decision> = {}): Decision {
  return {
    symbol: "BTC", kind: "crypto", name: "Bitcoin", asOf: AT - 3_600_000, price: 100, mode: "informational",
    verdict: "buy", label: "Acheter", level: "strong", levelLabel: "Signal fort", confidence: 72, confidenceText: "", headline: "Rebond sur support",
    families: [], vetoes: [{ code: "x", label: "Volatilité extrême", active: false, verifiable: true, detail: "" }], blocked: false,
    setup: { name: "Rebond", steps: [{ label: "Support", state: "ok", detail: "" }, { label: "Volume", state: "no", detail: "" }], met: 1, total: 2 },
    plan: { zoneFrom: 98, zoneTo: 102, entry: 100, stop: 95, target1: 110, target2: 120, riskPct: 5, reward1Pct: 10, reward2Pct: 20, riskReward: 2, minRiskReward: 2, acceptable: true, horizon: "swing" },
    whyWait: [], toBuy: [], toSell: [], scenarios: [], pros: ["a", "b", "c", "d"], cons: ["x"], whyNot: { risks: [], uncertainty: "low", invalidation: [] },
    fundamentals: null, liquidity: { spreadPct: null, dailyValue: null, relativeVolume: 1.4, source: "" }, track: null, position: null, exposure: null, sources: [], disclaimer: "d",
    rating: "buy", ratingLabel: "Achat", score: { value: 64, label: "", factors: [], missing: [], custom: false, text: "" },
    degraded: { active: false, headline: "", reasons: [] }, marketRegime: { kind: "riskOn", label: "Risk-on", benchmark: null, reasons: [] },
    horizon: { kind: "swing", label: "Swing", atrDistance: 1, detail: "" }, events: [],
    ...over,
  };
}

const entry = (over: Partial<Parameters<typeof createEntry>[0]> = {}): JournalEntry =>
  createEntry({ id: "e1", now: AT, source: "paper", side: "buy", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 100, stop: 95, targets: [110], decision: decision(), ...over });

/** Daily candles from day 0 (the entry day) with [low, high, close] per day. */
const days = (rows: [number, number, number][]): Candle[] =>
  rows.map(([low, high, close], i) => ({ time: T0 + i * DAY, open: i ? rows[i - 1]![2] : 100, high, low, close, volume: 10 }));

describe("writing entries", () => {
  test("snapshot keeps the reasons, the active vetoes, the setup and the plan; market conditions come from the decision", () => {
    const e = entry({ note: "  rebond  " });
    expect(e.decision!.pros).toEqual(["a", "b", "c"]);
    expect(e.decision!.vetoes).toEqual([]);
    expect(e.decision!.setup.steps).toEqual([{ label: "Support", state: "ok" }, { label: "Volume", state: "no" }]);
    expect(e.decision!.score).toBe(64);
    expect(e.market).toMatchObject({ regime: "riskOn", relativeVolume: 1.4, events: 0, macroScore: null, atrPct: null });
    expect(e.signal).toBe("Achat (Signal fort) · configuration « Rebond » 1/2 · horizon Swing");
    expect(e.note).toBe("rebond");
  });

  test("a stop above the price and targets below it are dropped; a sale keeps neither", () => {
    expect(entry({ stop: 105, targets: [90, 130, null] })).toMatchObject({ stop: null, targets: [130] });
    expect(entry({ side: "sell" })).toMatchObject({ stop: null, targets: [] });
  });

  test("older decisions without the added fields still give an entry", () => {
    const d = decision();
    for (const k of ["rating", "ratingLabel", "score", "degraded", "marketRegime", "horizon", "events"] as const) delete d[k];
    const e = createEntry({ id: "x", now: AT, source: "real", side: "buy", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 100, decision: d });
    expect(e.decision).toMatchObject({ rating: null, score: null, degraded: false, horizon: "swing" });
    expect(e.market.regime).toBeNull();
    expect(e.market.events).toBeNull();
  });

  test("state helpers: add (dedup by id), patch the market and the note, remove, validate", () => {
    let s = addEntry(emptyJournal(), entry());
    s = addEntry(s, entry());
    expect(s.entries).toHaveLength(1);
    s = patchEntry(s, "e1", { market: { macroScore: 31, macroLevel: "tense" }, note: "x" });
    expect(s.entries[0]!.market).toMatchObject({ macroScore: 31, macroLevel: "tense", regime: "riskOn" });
    expect(s.entries[0]!.note).toBe("x");
    expect(isJournalState(JSON.parse(JSON.stringify(s)))).toBe(true);
    expect(isJournalState({ version: 1, entries: [{ id: 1 }] })).toBe(false);
    expect(removeEntry(s, "e1").entries).toHaveLength(0);
  });

  test("ATR % and relative volume use only the days closed before the entry", () => {
    const c: Candle[] = Array.from({ length: 30 }, (_, i) => ({ time: T0 - (30 - i) * DAY, open: 100, high: 102, low: 98, close: 100, volume: i === 29 ? 30 : 10 }));
    // The entry day's own candle (huge range, huge volume) must not count.
    c.push({ time: T0, open: 100, high: 200, low: 50, close: 150, volume: 1000 });
    const m = marketFromCandles(c, AT);
    expect(m.atrPct).toBeCloseTo(4, 5);
    expect(m.relativeVolume).toBeCloseTo(3, 5);
  });

  test("a holding edit becomes a purchase (price implied by the new average cost) or a sale at the live price", () => {
    expect(holdingChange({ quantity: 1, averagePrice: 100 }, { quantity: 2, averagePrice: 110 }, 130)).toEqual({ side: "buy", quantity: 1, price: 120, implied: true });
    expect(holdingChange({ quantity: 1, averagePrice: 100 }, { quantity: 3, averagePrice: 100 }, 130)).toEqual({ side: "buy", quantity: 2, price: 130, implied: false });
    expect(holdingChange({ quantity: 2, averagePrice: 100 }, { quantity: 0.5, averagePrice: 100 }, 90)).toEqual({ side: "sell", quantity: 1.5, price: 90, implied: false });
    expect(holdingChange({ quantity: 2, averagePrice: 100 }, { quantity: 1, averagePrice: 100 }, null)).toBeNull();
    expect(holdingChange({ quantity: 2, averagePrice: 100 }, { quantity: 2, averagePrice: 90 }, 90)).toBeNull();
  });
});

describe("review", () => {
  test("target 1 reached on day 6: result at the target in R, the entry day's candle is ignored", () => {
    // Day 0 (entry day) touches the stop before the purchase: ignored.
    const c = days([[90, 101, 100], [99, 103, 102], [98, 104, 103], [99, 105, 104], [100, 106, 105], [101, 107, 106], [104, 111, 109], [105, 108, 107], [104, 108, 106], [104, 107, 106], [103, 106, 105]]);
    const r = reviewEntry(entry(), c, AT + 40 * DAY);
    const h10 = r.horizons.find((h) => h.days === 10)!;
    expect(h10.status).toBe("ready");
    expect(h10.first).toBe("target1");
    expect(h10.firstDays).toBe(6);
    expect(h10.resultR).toBe(2);
    expect(h10.mfePct).toBeCloseTo(11, 6);
    expect(h10.maeR).toBeCloseTo(-0.4, 6);
    expect(r.planR).toBe(2);
    expect(r.worked).toContain("L'objectif 1 a été atteint en 6 jours (+2 R).");
    expect(r.horizons.find((h) => h.days === 30)!.status).toBe("ready");
  });

  test("stop touched while the signal was degraded: −1 R, fact names the cause; a candle reaching both counts as the stop", () => {
    const e = entry({ decision: decision({ degraded: { active: true, headline: "Données incomplètes", reasons: [] } }) });
    const c = days([[99, 101, 100], [99, 102, 101], [94, 112, 96], [95, 97, 96]]);
    const r = reviewEntry(e, c, AT + 5 * DAY);
    const h3 = r.horizons[0]!;
    expect(h3.first).toBe("stop");
    expect(h3.firstDays).toBe(2);
    expect(h3.resultR).toBe(-1);
    expect(r.failed[0]).toBe("Le stop a été touché en 2 jours (−1 R) alors que le signal était dégradé à l'entrée.");
    expect(r.coherent).toBe(false);
    expect(r.horizons[1]!.status).toBe("pending");
  });

  test("a gap below the stop is counted at the open (worse than −1 R)", () => {
    const c = days([[99, 101, 100], [88, 92, 90]]);
    c[1]!.open = 90;
    const h = reviewEntry(entry(), c, AT + 4 * DAY).horizons[0]!;
    expect(h.resultR).toBe(-2);
  });

  test("coherence: vetoes, R/R under 2, outside the buy zone, against a risk-off regime, decision against the purchase", () => {
    const d = decision({
      verdict: "wait", label: "Attendre",
      vetoes: [{ code: "v", label: "Annonce macro dans les 48 h", active: true, verifiable: true, detail: "" }],
      plan: { ...decision().plan!, riskReward: 1.2 },
      marketRegime: { kind: "riskOff", label: "Risk-off", benchmark: null, reasons: [] },
    });
    const e = entry({ price: 104.08, decision: d });
    const checks = Object.fromEntries(coherenceChecks(e).map((c) => [c.code, c]));
    expect(checks.vetoes!.ok).toBe(false);
    expect(checks.vetoes!.detail).toContain("Annonce macro dans les 48 h");
    expect(checks.rr!.ok).toBe(false);
    expect(checks.zone!.detail).toBe("Entrée hors zone d'achat : +2 % au-dessus du haut de la zone.");
    expect(checks.regime!.ok).toBe(false);
    expect(checks.verdict!.ok).toBe(false);
    expect(checks.degraded!.ok).toBe(true);
  });

  test("without a decision nothing is verifiable; without a stop R is not computed and the plan's stop is used when there is one", () => {
    const bare = entry({ decision: null, stop: null, targets: [] });
    const r = reviewEntry(bare, days([[99, 101, 100], [100, 104, 103], [101, 105, 104], [102, 106, 105]]), AT + 4 * DAY);
    expect(r.coherent).toBeNull();
    expect(r.risk).toBeNull();
    expect(r.horizons[0]!.resultR).toBeNull();
    expect(r.worked[0]).toBe("+5 % en 3 jours (sans stop, résultat en R non mesurable).");
    const fromPlan = reviewEntry(entry({ stop: null, targets: [] }), [], AT);
    expect(fromPlan.levels).toEqual({ stop: 95, stopSource: "plan", targets: [110, 120], targetsSource: "plan" });
  });

  test("a history that starts after the entry gives no review (no hole at the start)", () => {
    const c = days([[99, 101, 100], [99, 101, 100], [99, 101, 100], [99, 101, 100]]).slice(2);
    expect(reviewEntry(entry(), c, AT + 5 * DAY).horizons[0]!.status).toBe("noData");
  });

  test("a closed paper trade adds its realized R", () => {
    const r = reviewEntry(entry(), [], AT + DAY, { at: AT + DAY, price: 105, reason: "vente manuelle" });
    expect(r.realizedR).toBe(1);
  });

  test("a sale: the price after the sale, checked against the decision", () => {
    const e = entry({ side: "sell", decision: decision({ verdict: "trim", label: "Alléger" }) });
    const r = reviewEntry(e, days([[99, 101, 100], [95, 99, 96], [94, 97, 95], [93, 96, 94]]), AT + 4 * DAY);
    expect(r.worked[0]).toContain("3 jours après la vente, le cours est à −6 %");
    expect(r.planRespected).toBeNull();
    expect(r.coherent).toBe(true);
  });
});

describe("profile", () => {
  test("groups by rating, plan and regime with sample sizes; under 5 the group is flagged", () => {
    const win = days([[99, 101, 100], [100, 111, 110], [104, 108, 107], [104, 108, 107]]);
    const loss = days([[99, 101, 100], [94, 100, 95], [95, 97, 96], [95, 97, 96]]);
    const reviews = [
      ...[0, 1, 2, 3, 4].map((i) => reviewEntry(entry({ id: `w${i}` }), win, AT + 5 * DAY)),
      reviewEntry(entry({ id: "l", decision: decision({ marketRegime: { kind: "riskOff", label: "Risk-off", benchmark: null, reasons: [] }, rating: "hold", ratingLabel: "Conserver" }) }), loss, AT + 5 * DAY),
    ];
    const p = journalProfile(reviews, 3);
    expect(p.reviewed).toBe(6);
    expect(p.all).toMatchObject({ n: 6, avgR: 1.5, lowSample: false });
    expect(p.byRating.map((g) => [g.key, g.n, g.lowSample])).toEqual([["buy", 5, false], ["hold", 1, true]]);
    expect(p.byPlan.map((g) => [g.key, g.n])).toEqual([["yes", 5], ["no", 1]]);
    expect(p.byRegime.map((g) => [g.key, g.avgR])).toEqual([["riskOn", 2], ["riskOff", -1]]);
    expect(p.byRegime[1]!.worstMaeR).toBe(-1.2);
    expect(journalProfile(reviews, 10).reviewed).toBe(0);
  });
});

test("saved journal: empty, valid, unreadable (ignored with a message, never a crash)", async () => {
  const { parseSavedJournal } = await import("../src/webapp/journal-store");
  expect(parseSavedJournal(null)).toEqual({ state: emptyJournal(), error: null });
  const s = addEntry(emptyJournal(), entry());
  expect(parseSavedJournal(JSON.stringify(s)).state.entries).toHaveLength(1);
  expect(parseSavedJournal("{oops").error).toContain("illisible");
  expect(parseSavedJournal(JSON.stringify({ version: 2, entries: [] })).error).toContain("illisible");
});

test("snapshotDecision without a plan", () => {
  expect(snapshotDecision(decision({ plan: null })).plan).toBeNull();
});
