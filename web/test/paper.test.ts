import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { FEE_RATE, SLIPPAGE, checkExits, closePosition, isPaperState, newPaper, openPosition, paperStats, valuation } from "../src/engine/paper";
import { replay } from "./paper-scenario";

const T0 = Date.UTC(2026, 8, 1, 14, 0);

describe("simulation (paper trading)", () => {
  test("achat : frais et glissement payés, quantité exacte", () => {
    const { state, error } = openPosition(newPaper(10_000, T0), { id: "a", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 64_000, amount: 3_000, stop: 60_000, target: 72_000 }, T0);
    expect(error).toBeUndefined();
    const p = state.positions[0]!;
    expect(p.entry).toBeCloseTo(64_000 * (1 + SLIPPAGE), 6);
    expect(p.quantity).toBeCloseTo((3_000 * (1 - FEE_RATE)) / (64_000 * 1.0005), 9);
    expect(state.cash).toBe(7_000);
  });

  test("objectif atteint le lendemain : vendu à l'objectif moins glissement et frais (+364,89 $)", () => {
    let s = openPosition(newPaper(10_000, T0), { id: "a", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 64_000, amount: 3_000, stop: 60_000, target: 72_000 }, T0).state;
    // The opening day (low 59 000 < stop) is not used: its low may be before the purchase.
    s = checkExits(s, { "crypto:BTC": [{ time: Date.UTC(2026, 8, 1), open: 63_000, high: 65_000, low: 59_000, close: 64_500 }] }).state;
    expect(s.positions.length).toBe(1);
    const r = checkExits(s, { "crypto:BTC": [{ time: Date.UTC(2026, 8, 3), open: 70_000, high: 73_000, low: 69_000, close: 72_500 }] });
    const t = r.closed[0]!;
    expect(t.reason).toBe("target");
    expect(t.exit).toBeCloseTo(72_000 * 0.9995, 6);
    const qty = (3_000 * 0.999) / (64_000 * 1.0005);
    expect(t.proceeds).toBeCloseTo(qty * 71_964 * 0.999, 2);
    expect(t.pnl).toBe(364.89);
  });

  test("écart sous le stop : vendu à l'ouverture ; stop et objectif le même jour : le stop", () => {
    let s = openPosition(newPaper(10_000, T0), { id: "e", symbol: "ETH", kind: "crypto", name: "Ethereum", price: 2_500, amount: 1_000, stop: 2_300, target: 3_000 }, T0).state;
    let r = checkExits(s, { "crypto:ETH": [{ time: Date.UTC(2026, 8, 2), open: 2_200, high: 2_250, low: 2_150, close: 2_210 }] });
    expect(r.closed[0]!.exit).toBeCloseTo(2_200 * 0.9995, 6);
    s = openPosition(newPaper(10_000, T0), { id: "b", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 70_000, amount: 2_000, stop: 66_000, target: 76_000 }, T0).state;
    r = checkExits(s, { "crypto:BTC": [{ time: Date.UTC(2026, 8, 2), open: 70_000, high: 77_000, low: 65_000, close: 71_000 }] });
    expect(r.closed[0]!.reason).toBe("stop");
  });

  test("refus : plus que les liquidités, prix absent, montant nul ; stop au-dessus du prix ignoré", () => {
    const s = newPaper(1_000, T0);
    const o = { id: "x", symbol: "AAPL", kind: "stock" as const, name: "Apple", price: 250, amount: 500 };
    expect(openPosition(s, { ...o, amount: 1_500 }, T0).error).toContain("insuffisantes");
    expect(openPosition(s, { ...o, price: Number.NaN }, T0).error).toBe("Prix indisponible.");
    expect(openPosition(s, { ...o, amount: -1 }, T0).error).toBe("Montant invalide.");
    expect(openPosition(s, { ...o, stop: 260 }, T0).state.positions[0]!.stop).toBeNull();
    expect(closePosition(s, "nope", 10, T0).error).toBe("Position introuvable.");
  });

  test("valorisation : comme si tout était vendu maintenant ; sans prix, au coût", () => {
    const s = openPosition(newPaper(10_000, T0), { id: "a", symbol: "SOL", kind: "crypto", name: "Solana", price: 100, amount: 1_000 }, T0).state;
    const v = valuation(s, { "crypto:SOL": 110 });
    const qty = 999 / 100.05;
    expect(v.lines[0]!.value).toBeCloseTo(qty * 110 * 0.9995 * 0.999, 2);
    expect(v.equity).toBeCloseTo(9_000 + v.lines[0]!.value!, 2);
    const none = valuation(s, {});
    expect(none.unpriced).toBe(1);
    expect(none.equity).toBe(10_000);
  });

  test("scénario de référence (partagé avec l'iPhone et Android)", () => {
    const fixture = JSON.parse(readFileSync(join(import.meta.dir, "paper-fixture.json"), "utf8"));
    expect(JSON.parse(JSON.stringify(replay()))).toEqual(fixture);
    const last = fixture.at(-1);
    expect(last.state.cash).toBe(10_509.12);
    expect(last.stats.trades).toBe(5);
    expect(last.stats.winRate).toBe(40);
    expect(last.stats.byReason).toEqual({ stop: 2, target: 2, manual: 1 });
    expect(last.stats.byVerdict.map((v: { verdict: string; trades: number }) => [v.verdict, v.trades])).toEqual([["buy", 2], ["buyZone", 1], ["none", 1], ["wait", 1]]);
  });

  test("statistiques : profit factor, pire recul, résultats par décision", () => {
    const st = paperStats({ ...newPaper(1_000, T0), trades: [] });
    expect(st.trades).toBe(0);
    expect(st.profitFactor).toBeNull();
    expect(st.maxDrawdownPct).toBe(0);
  });

  test("état enregistré contrôlé avant usage", () => {
    expect(isPaperState(newPaper(5_000, T0))).toBe(true);
    expect(isPaperState({ version: 1, startCapital: 1000, cash: 1000, positions: [{ id: 1 }], trades: [] })).toBe(false);
    expect(isPaperState(null)).toBe(false);
  });
});
