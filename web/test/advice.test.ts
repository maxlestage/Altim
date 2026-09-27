import { expect, test } from "bun:test";
import fixture from "./swift-fixture.json";
import { adviceQuantity, adviseAsset, px, quantityText } from "../src/engine/advice";
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

test("un signal d'achat qui a surtout échoué sur cet actif devient « Attendre »", () => {
  const poor = adviseAsset({ signal: sig("strongBuy"), reliability: "high", price: base.price, capital: 20_000, risk: DEFAULT_RISK, track: { trades: 12, winRate: 25, avgReturn: -0.8 } });
  expect(poor.tone).toBe("hold");
  expect(poor.title).toContain("peu fiable");
  expect(poor.amount).toBeUndefined();
  expect(poor.points[0]).toContain("25 %");
  // Negative average even with a decent win rate.
  expect(adviseAsset({ signal: sig("buy"), reliability: "high", price: base.price, risk: DEFAULT_RISK, track: { trades: 8, winRate: 55, avgReturn: -0.1 } }).tone).toBe("hold");
  // Good track record: the buy stays, with its figures.
  const good = adviseAsset({ signal: sig("buy"), reliability: "high", price: base.price, risk: DEFAULT_RISK, track: { trades: 9, winRate: 56, avgReturn: 1.4 } });
  expect(good.tone).toBe("buy");
  expect(good.points.join(" ")).toContain("9 signaux d'achat, 56 % gagnants, +1,4 %");
  // Too few trades: the buy stays but prudence is stated.
  const few = adviseAsset({ signal: sig("buy"), reliability: "high", price: base.price, risk: DEFAULT_RISK, track: { trades: 2, winRate: 0, avgReturn: -2 } });
  expect(few.tone).toBe("buy");
  expect(few.points.join(" ")).toContain("pas assez pour juger");
});

test("montants précis : quantité exacte à acheter, actions entières, petits prix lisibles", () => {
  expect(adviceQuantity(1000, 84_871.45, "crypto")).toBeCloseTo(0.0117825, 7);
  expect(adviceQuantity(1000, 84_871.45, "crypto") * 84_871.45).toBeLessThanOrEqual(1000);
  expect(adviceQuantity(1000, 341.07, "stock")).toBe(2);
  expect(adviceQuantity(100, 341.07, "stock")).toBe(0);
  expect(adviceQuantity(50, 0.000009312, "crypto")).toBe(5_369_410);
  expect(quantityText(2, "stock", "AAPL")).toBe("2 actions AAPL");
  expect(quantityText(0.0117825, "crypto", "BTC")).toBe("0,0117825 BTC");
  expect(px(0.000009312)).toBe("0,000009312 $");
  expect(px(84_871.456)).toBe("84\u202f871,46 $");
  const a = adviseAsset({ signal: sig("buy"), reliability: "high", price: base.price, capital: 20_000, risk: DEFAULT_RISK, symbol: "BTC", kind: "crypto" });
  expect(a.quantity! * base.price).toBeLessThanOrEqual(a.amount! + 1e-9);
  expect(a.points.join(" ")).toContain(" BTC :");
});

test("actions entières : le montant et la perte au stop sont ceux de la quantité arrondie", () => {
  const a = adviseAsset({ signal: sig("buy"), reliability: "high", price: 341.07, capital: 20_000, risk: DEFAULT_RISK, symbol: "AAPL", kind: "stock" });
  expect(Number.isInteger(a.quantity!)).toBe(true);
  expect(a.amount!).toBeCloseTo(a.quantity! * 341.07, 6);
  expect(a.points.join(" ")).toContain(`soit ${a.quantity} actions AAPL`);
});

test("garde-fou : pas d'achat en plein choc ni sur un retournement probable, montant divisé par deux si agité", () => {
  const base_ = { signal: sig("strongBuy"), reliability: "high" as const, price: base.price, capital: 20_000, risk: DEFAULT_RISK, symbol: "BTC", kind: "crypto" as const };
  const chocked = adviseAsset({ ...base_, guard: { shock: "shock", reversalScore: 0, reversalDirection: null } });
  expect(chocked.tone).toBe("hold");
  expect(chocked.title).toContain("choc");
  const rev = adviseAsset({ ...base_, guard: { shock: "calm", reversalScore: 60, reversalDirection: "down" } });
  expect(rev.title).toContain("retournement");
  // A reversal UPWARD does not block a buy.
  expect(adviseAsset({ ...base_, guard: { shock: "calm", reversalScore: 80, reversalDirection: "up" } }).tone).toBe("buy");
  const calm = adviseAsset({ ...base_, guard: { shock: "calm", reversalScore: 0, reversalDirection: null } });
  const agitated = adviseAsset({ ...base_, guard: { shock: "agitated", reversalScore: 0, reversalDirection: null } });
  expect(agitated.tone).toBe("buy");
  expect(agitated.amount!).toBeCloseTo(calm.amount! / 2, 0);
  expect(agitated.quantity! * base.price).toBeLessThanOrEqual(agitated.amount! + 1e-9);
  expect(agitated.points.join(" ")).toContain("divisé par deux");
});

test("garde-fou sur un actif détenu : resserrer le stop si un retournement à la baisse est probable", () => {
  const line = { quantity: 2, value: 5000, pnl: 800, weight: 20, reasons: ["momentum"], recommendation: "strengthen", trimValue: 0, stop: 2300, lossAtStop: 400, price: 2500, kind: "crypto", symbol: "ETH" } as unknown as LineAnalysis;
  const a = adviseAsset({ signal: sig("buy"), reliability: "high", price: 2500, line, risk: DEFAULT_RISK, guard: { shock: "calm", reversalScore: 55, reversalDirection: "down" } });
  expect(a.tone).toBe("hold");
  expect(a.points.join(" ")).toContain("resserrez votre stop");
});
