import { afterEach, describe, expect, test } from "bun:test";
import { convert, displayCurrency, fromDisplay, fxLine, money, moneyCompact, moneyPrice, setMoneyDisplay, toDisplay, type FxRate } from "../src/money";
import { parseFx, savedFx } from "../src/webapp/fx";
import { mergeLine, toUsdHoldings, type HoldingsState } from "../src/webapp/store";
import { parseBudget } from "../src/webapp/Selection";
import { usd, usdCompact } from "../src/webapp/decision";

const NNBSP = "\u202f";
const FX: FxRate = { rate: 0.88, usdPerEur: 1 / 0.88, time: Date.UTC(2026, 8, 29, 12, 5), source: "Yahoo Finance", fetchedAt: Date.now(), stale: false };

afterEach(() => setMoneyDisplay("USD", null));

describe("display currency", () => {
  test("dollars by default and without a rate: never a made-up conversion", () => {
    expect(displayCurrency()).toBe("USD");
    expect(money(212.4)).toBe("212,40\u00a0$");
    setMoneyDisplay("EUR", null);
    expect(displayCurrency()).toBe("USD");
    expect(moneyPrice(212.4)).toBe("212,40\u00a0$");
    expect(convert(100, "EUR", "USD")).toBeNaN();
    expect(fxLine(null)).toBeNull();
  });

  test("euros with a rate: every formatter converts", () => {
    setMoneyDisplay("EUR", FX);
    expect(displayCurrency()).toBe("EUR");
    expect(money(241.3636)).toBe("212,40\u00a0€");
    expect(moneyPrice(0.5)).toBe("0,4400\u00a0€");
    expect(moneyCompact(1e10)).toBe("8,8\u00a0Md€");
    expect(usd(100)).toBe(`88${NNBSP}€`);
    expect(usdCompact(2e9)).toBe(`1,76${NNBSP}Md€`);
    expect(toDisplay(100)).toBeCloseTo(88, 10);
    expect(fromDisplay(88)).toBeCloseTo(100, 10);
    expect(fxLine(FX, FX.time)).toMatch(/^1 \$ = 0,880 € · Yahoo Finance, \d{2}:\d{2}$/);
  });

  test("dollars chosen: no conversion even with a rate", () => {
    setMoneyDisplay("USD", FX);
    expect(money(100)).toBe("100,00\u00a0$");
    expect(fromDisplay(100)).toBe(100);
  });
});

describe("/api/fx", () => {
  test("a valid rate is kept, a missing one never replaced by a default", () => {
    const body = { base: "USD", quote: "EUR", rate: 0.88, usdPerEur: 1.136, time: 1, source: "BCE", fetchedAt: 2, stale: false };
    expect(parseFx(body)?.rate).toBe(0.88);
    expect(parseFx({ ...body, rate: null, error: "taux indisponible" })).toBeNull();
    expect(parseFx({ ...body, rate: 88 })).toBeNull();
    expect(savedFx(JSON.stringify(body), 2 + 3_600_000)?.stale).toBe(true);
    expect(savedFx(JSON.stringify(body), 2 + 8 * 86_400_000)).toBeNull();
    expect(savedFx("{", 0)).toBeNull();
  });
});

describe("saved amounts and their currency", () => {
  const state: HoldingsState = {
    version: 1, cash: 1000, updatedAt: 0,
    holdings: [
      // Old line: no tag = dollars.
      { id: "a", symbol: "AAPL", kind: "stock", name: "Apple", quantity: 2, averagePrice: 150, stop: 140 },
      { id: "b", symbol: "BTC", kind: "crypto", name: "Bitcoin", quantity: 0.1, averagePrice: 44_000, costCurrency: "EUR", stop: 40_000, stopCurrency: "EUR" },
    ],
    cashCurrency: "EUR",
  };

  test("euro costs go to the engines in dollars at the current rate; P&L back in euros is price € − cost €", () => {
    setMoneyDisplay("EUR", FX);
    const u = toUsdHoldings(state);
    expect(u.unconverted).toEqual([]);
    expect(u.holdings[0]!.averagePrice).toBe(150);
    expect(u.holdings[1]!.averagePrice).toBeCloseTo(50_000, 6);
    expect(u.holdings[1]!.stop).toBeCloseTo(40_000 / 0.88, 6);
    expect(u.cash).toBeCloseTo(1000 / 0.88, 6);
    // BTC at 60 000 $ = 52 800 €: gain 0,1 × (52 800 − 44 000) = 880 €.
    const pnlUsd = 0.1 * (60_000 - u.holdings[1]!.averagePrice);
    expect(toDisplay(pnlUsd)).toBeCloseTo(880, 6);
  });

  test("without a rate, euro amounts are flagged, never read as dollars", () => {
    setMoneyDisplay("EUR", null);
    const u = toUsdHoldings(state);
    expect(u.unconverted).toEqual(["BTC"]);
    expect(u.holdings[1]!.averagePrice).toBe(0);
    expect(u.holdings[1]!.stop).toBeUndefined();
    expect(u.cashUnconverted).toBe(true);
    expect(u.holdings[0]!.averagePrice).toBe(150);
  });

  test("a new purchase in euros merges a dollar cost converted at the current rate", () => {
    setMoneyDisplay("EUR", FX);
    const m = mergeLine(state.holdings[0]!, { quantity: 2, averagePrice: 200, costCurrency: "EUR" })!;
    expect(m.costCurrency).toBe("EUR");
    expect(m.quantity).toBe(4);
    expect(m.averagePrice).toBeCloseTo((2 * 150 * 0.88 + 2 * 200) / 4, 9);
    setMoneyDisplay("EUR", null);
    expect(mergeLine(state.holdings[0]!, { quantity: 1, averagePrice: 200, costCurrency: "EUR" })).toBeNull();
  });

  test("Selection budget: a bare number is dollars, the new format carries its currency", () => {
    expect(parseBudget("5000")).toEqual({ amount: 5000, currency: "USD" });
    expect(parseBudget('{"amount":4000,"currency":"EUR"}')).toEqual({ amount: 4000, currency: "EUR" });
    expect(parseBudget("-3")).toBeNull();
    expect(parseBudget(null)).toBeNull();
  });
});
