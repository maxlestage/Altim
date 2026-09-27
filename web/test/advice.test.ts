import { expect, test } from "bun:test";
import fixture from "./swift-fixture.json";
import { adviseAsset } from "../src/engine/advice";
import { analyze, type Candle, type Signal } from "../src/engine/signal";
import { DEFAULT_RISK } from "../src/engine/risk";
import type { LineAnalysis } from "../src/engine/holdings";

const candles: Candle[] = (fixture as unknown as { candles: number[][] }[])[0]!.candles.map(([time, open, high, low, close, volume]) => ({
  time: time!, open: open!, high: high!, low: low!, close: close!, volume: volume!,
}));
const base = analyze(candles)!;
const sig = (action: Signal["action"]): Signal => ({ ...base, action, confidence: 60 });

test("achat envisageable : plan et montant prudent calculé sur le patrimoine", () => {
  const a = adviseAsset({ signal: sig("buy"), reliability: "high", price: base.price, capital: 20_000, risk: DEFAULT_RISK });
  expect(a.tone).toBe("buy");
  expect(a.plan!.stop).toBeLessThan(a.plan!.entry);
  expect(a.amount!).toBeGreaterThan(0);
  expect(a.amount!).toBeLessThanOrEqual(20_000 * 0.2 + 1e-6); // position cap
  expect(a.points.join(" ")).toContain("n'y consacrez pas plus");
});

test("sans patrimoine renseigné : invite à remplir Mes avoirs", () => {
  const a = adviseAsset({ signal: sig("strongBuy"), reliability: "high", price: base.price, risk: DEFAULT_RISK });
  expect(a.title).toContain("signal fort");
  expect(a.amount).toBeUndefined();
  expect(a.points.join(" ")).toContain("Mes avoirs");
});

test("attendre, éviter, et aucun conseil si données peu fiables", () => {
  expect(adviseAsset({ signal: sig("hold"), reliability: "high", price: 1, risk: DEFAULT_RISK }).tone).toBe("hold");
  expect(adviseAsset({ signal: sig("strongSell"), reliability: "high", price: 1, risk: DEFAULT_RISK }).tone).toBe("sell");
  const low = adviseAsset({ signal: sig("strongBuy"), reliability: "low", price: 1, capital: 1000, risk: DEFAULT_RISK });
  expect(low.tone).toBe("unknown");
  expect(low.amount).toBeUndefined();
  expect(adviseAsset({ signal: sig("buy"), reliability: "medium", price: base.price, risk: DEFAULT_RISK }).points.join(" ")).toContain("prudent");
});

test("actif détenu : le conseil vient de l'analyse de vos avoirs", () => {
  const line = { quantity: 2, value: 5000, pnl: -300, weight: 40, reasons: ["overweight"], recommendation: "lighten", trimValue: 1200, stop: 2300, lossAtStop: 400 } as LineAnalysis;
  const a = adviseAsset({ signal: sig("buy"), reliability: "high", price: 2500, line, capital: 12_000, risk: DEFAULT_RISK });
  expect(a.title).toBe("Alléger");
  expect(a.points.join(" ")).toContain("Vous en détenez");
  expect(a.points.join(" ")).toContain("Montant à alléger");
});
