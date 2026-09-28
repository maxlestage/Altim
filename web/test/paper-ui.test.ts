import { describe, expect, test } from "bun:test";
import { checkExits, newPaper, openPosition, paperStats, type PaperTrade } from "../src/engine/paper";
import {
  againstDecision, againstText, assetsToCheck, BAD_DATA_MESSAGE, defaultAmount, exitNotice, frDate, inputPrice, journal, levelWarnings, newId, parseAmount,
  parseSavedPaper, signedPct, signedUsd, startCapital, toDaily, usd, verdictRows,
} from "../src/webapp/paper-ui";

const T0 = Date.UTC(2026, 8, 1, 14, 0);
const NB = " ";

describe("parseSavedPaper", () => {
  test("nothing saved", () => {
    expect(parseSavedPaper(null)).toEqual({ state: null, error: null });
    expect(parseSavedPaper("")).toEqual({ state: null, error: null });
  });
  test("valid state", () => {
    const s = newPaper(5_000, T0);
    expect(parseSavedPaper(JSON.stringify(s))).toEqual({ state: s, error: null });
  });
  test("bad data is ignored with a message, never throws", () => {
    for (const raw of ["{", "null", "42", '{"version":2}', JSON.stringify({ ...newPaper(1000, T0), positions: [{ id: 1 }] }), JSON.stringify({ ...newPaper(1000, T0), cash: "x" })]) {
      expect(parseSavedPaper(raw)).toEqual({ state: null, error: BAD_DATA_MESSAGE });
    }
  });
});

describe("amounts", () => {
  test("parseAmount reads French input", () => {
    expect(parseAmount("1 234,5")).toBe(1234.5);
    expect(parseAmount("10 000 $")).toBe(10_000);
    expect(parseAmount("0.25")).toBe(0.25);
    expect(parseAmount("")).toBeNull();
    expect(parseAmount("  ")).toBeNull();
    expect(Number.isNaN(parseAmount("abc")!)).toBe(true);
    expect(Number.isNaN(parseAmount("1,2,3")!)).toBe(true);
    expect(Number.isNaN(parseAmount("-5")!)).toBe(true);
  });
  test("defaultAmount = min(10 % of equity, cash), cents rounded down", () => {
    expect(defaultAmount(10_000, 10_000)).toBe(1_000);
    expect(defaultAmount(10_000, 400)).toBe(400);
    expect(defaultAmount(12_345.678, 9_000)).toBe(1_234.56);
    expect(defaultAmount(10_000, 0)).toBe(0);
    expect(defaultAmount(NaN, 100)).toBe(0);
  });
  test("inputPrice prefills readable numbers that parseAmount reads back", () => {
    expect(inputPrice(63575.19787210)).toBe("63575,2");
    expect(inputPrice(1000)).toBe("1000");
    expect(inputPrice(0.000012345678)).toBe("0,0000123457");
    expect(inputPrice(0.5)).toBe("0,5");
    expect(inputPrice(null)).toBe("");
    expect(inputPrice(0)).toBe("");
    for (const v of [63575.2, 0.0000123457, 12.34]) expect(parseAmount(inputPrice(v))).toBeCloseTo(v, 10);
  });
  test("startCapital falls back to 10 000", () => {
    expect(startCapital("25 000")).toBe(25_000);
    expect(startCapital("0")).toBe(10_000);
    expect(startCapital("abc")).toBe(10_000);
    expect(startCapital("")).toBe(10_000);
  });
});

describe("decision and levels", () => {
  test("simulating against the decision", () => {
    expect(againstDecision("buy")).toBe(false);
    expect(againstDecision("buyZone")).toBe(false);
    for (const v of ["wait", "noPosition", "trim", "sell"]) expect(againstDecision(v)).toBe(true);
    expect(againstText("ATTENDRE")).toBe("La décision actuelle est « ATTENDRE » : vous simulez contre elle.");
  });
  test("levelWarnings mirror what openPosition drops", () => {
    const fill = 100 * 1.0005;
    expect(levelWarnings(fill, 90, 120)).toEqual([]);
    expect(levelWarnings(fill, null, null)).toEqual([]);
    expect(levelWarnings(fill, 101, 100)).toHaveLength(2);
    // Same verdict as the engine: a stop at or above the fill is dropped, a target at or below too.
    const { state } = openPosition(newPaper(10_000, T0), { id: "a", symbol: "X", kind: "crypto", name: "X", price: 100, amount: 100, stop: 101, target: 100 }, T0);
    expect(state.positions[0]!.stop).toBeNull();
    expect(state.positions[0]!.target).toBeNull();
  });
});

describe("exits and journal", () => {
  const base = openPosition(
    openPosition(newPaper(10_000, T0), { id: "p1", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 64_000, amount: 1_000, stop: 60_000, target: 72_000 }, T0).state,
    { id: "p2", symbol: "SOL", kind: "crypto", name: "Solana", price: 150, amount: 500 },
    T0,
  ).state;

  test("assetsToCheck keeps only positions with a stop or a target, once per asset", () => {
    expect(assetsToCheck(base.positions)).toEqual([{ symbol: "BTC", kind: "crypto" }]);
  });

  test("toDaily drops bad rows and extra fields", () => {
    expect(toDaily([{ time: 1, open: 1, high: 2, low: 0.5, close: 1.5, volume: 9 } as never, { time: 2, open: NaN, high: 1, low: 1, close: 1 }])).toEqual([
      { time: 1, open: 1, high: 2, low: 0.5, close: 1.5 },
    ]);
  });

  test("exitNotice names the reason and the candle day", () => {
    const day = Date.UTC(2026, 8, 3);
    const { closed } = checkExits(base, { "crypto:BTC": [{ time: day, open: 63_000, high: 63_500, low: 59_000, close: 61_000 }] });
    expect(closed).toHaveLength(1);
    const text = exitNotice(closed[0]!);
    expect(text.startsWith("Stop touché le 3 septembre 2026 : Bitcoin (BTC), −")).toBe(true);
    expect(text).toContain(`${NB}%`);
  });

  test("journal: most recent exit first", () => {
    const t = (id: string, closedAt: number) => ({ id, closedAt, openedAt: 0 }) as PaperTrade;
    expect(journal([t("a", 1), t("b", 3), t("c", 2)]).map((x) => x.id)).toEqual(["b", "c", "a"]);
  });

  test("verdictRows follow the decision scale, 'Sans décision' last", () => {
    const row = (verdict: string) => ({ verdict, label: verdict, trades: 1, wins: 0, winRate: 0, avgPnlPct: 0 });
    expect(verdictRows([row("none"), row("wait"), row("sell"), row("buy"), row("buyZone")]).map((r) => r.verdict)).toEqual(["buy", "buyZone", "wait", "sell", "none"]);
    // With the engine's own grouping.
    const s = paperStats(base);
    expect(verdictRows(s.byVerdict)).toEqual([]);
  });
});

describe("formatting", () => {
  test("French numbers with signs (never colour alone)", () => {
    expect(usd(1234.5)).toBe(`1${NB}234,50${NB}$`);
    expect(signedUsd(-12.3)).toBe(`−12,30${NB}$`);
    expect(signedUsd(5)).toBe(`+5,00${NB}$`);
    expect(signedUsd(0)).toBe(`0,00${NB}$`);
    expect(signedPct(-1.234)).toBe(`−1,23${NB}%`);
  });
  test("frDate: candle days in UTC", () => {
    expect(frDate(Date.UTC(2026, 8, 3))).toBe("3 septembre 2026");
  });
  test("newId is unique", () => {
    expect(newId()).not.toBe(newId());
  });
});
