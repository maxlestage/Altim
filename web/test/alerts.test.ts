import { describe, expect, test } from "bun:test";
import { buyAlert, type AlertInput } from "../src/engine/alerts";

const zone = (horizon: "short" | "medium" | "long", status: string, label = "Moyen terme") => ({
  horizon, label, status: status as never, zone: { from: 68_000, to: 76_000 }, golden: { from: 68_000, to: 69_000 }, invalidation: 57_800,
});
const base: AlertInput = {
  symbol: "BTC", name: "Bitcoin", price: 72_000,
  signal: { action: "hold", confidence: 40 }, reliability: "high",
  zones: [zone("short", "above", "Court terme"), zone("medium", "above")],
  shock: "calm", trend: "up", macro: "calm",
};

describe("buyAlert — « quand puis-je acheter ? »", () => {
  test("rien à signaler : ni signal ni zone", () => {
    const a = buyAlert(base);
    expect(a.buy).toBe(false);
    expect(a.key).toBe("");
    expect(a.title).toBe("BTC : pas d'achat pour l'instant");
  });

  test("signal d'achat seul → achat possible", () => {
    const a = buyAlert({ ...base, signal: { action: "buy", confidence: 55 } });
    expect(a.buy).toBe(true);
    expect(a.strong).toBe(false);
    expect(a.key).toBe("signal");
    expect(a.title).toContain("achat possible");
    expect(a.body).toContain("Signal ACHAT en 4 h (confiance 55 %)");
  });

  test("prix dans la zone + signal → achat conseillé, zone la plus longue d'abord, invalidation donnée", () => {
    const a = buyAlert({ ...base, signal: { action: "strongBuy", confidence: 70 }, zones: [zone("short", "inZone", "Court terme"), zone("medium", "golden")] });
    expect(a.strong).toBe(true);
    expect(a.key).toBe("signal+zone:medium");
    expect(a.body).toContain("zone d'or moyen terme");
    expect(a.body).toContain("Invalidé sous 57");
    expect(a.title).toContain("achat conseillé à 72");
  });

  test("bloqué : sources en désaccord, choc, ou plus bas cassé", () => {
    const inZone = { ...base, zones: [zone("medium", "inZone")] };
    expect(buyAlert(inZone).buy).toBe(true);
    expect(buyAlert({ ...inZone, reliability: "low" }).buy).toBe(false);
    const shock = buyAlert({ ...inZone, shock: "shock" });
    expect(shock.buy).toBe(false);
    expect(shock.body).toContain("risque de choc");
    expect(buyAlert({ ...inZone, price: 50_000 }).buy).toBe(false);
  });

  test("les mises en garde ne bloquent pas mais sont écrites", () => {
    const a = buyAlert({ ...base, zones: [zone("medium", "inZone")], trend: "down", macro: "high" });
    expect(a.buy).toBe(true);
    expect(a.body).toContain("Tendance de fond baissière");
    expect(a.body).toContain("Contexte macro très tendu");
  });

  test("une zone cassée ou une tendance baissière de la zone ne comptent pas comme raison", () => {
    expect(buyAlert({ ...base, zones: [zone("medium", "broken"), zone("long", "downtrend", "Long terme")] }).buy).toBe(false);
  });
});
