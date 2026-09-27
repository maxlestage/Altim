import { expect, test } from "bun:test";
import { floorQty } from "../src/webapp/TradeSheet";
import { positionSize, preTradeIssues, DEFAULT_RISK } from "../src/engine/risk";

test("taille suggérée arrondie vers le bas", () => {
  expect(floorQty(0.0235294)).toBe("0.02352");
  expect(floorQty(1.23456)).toBe("1.234");
  expect(floorQty(19.2307)).toBe("19.23");
  expect(floorQty(1234.56)).toBe("1234");
  expect(floorQty(0)).toBe("");
});

test("la taille suggérée plafonnée passe toujours les contrôles de risque", () => {
  for (const price of [84_988.2, 85_016, 2716.52, 124.52, 0.2555, 341.07]) {
    const plan = { entry: price, stopLoss: price * 0.99, takeProfit: price * 1.02 };
    const s = positionSize(DEFAULT_RISK, 10_000, plan)!;
    const qty = Number(floorQty(s.quantity));
    expect(qty * price).toBeLessThanOrEqual(2000.0001);
    expect(preTradeIssues(DEFAULT_RISK, "buy", qty * price, 10_000, plan, 0)).toEqual([]);
  }
});
