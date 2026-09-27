import { expect, test } from "bun:test";
import fixture from "./swift-fixture.json";
import { assessQuality, gate, reliability } from "../src/engine/reliability";
import { analyze, type Candle } from "../src/engine/signal";

const base: Candle[] = (fixture as unknown as { candles: number[][] }[])[0]!.candles.map(([time, open, high, low, close, volume]) => ({
  time: time!, open: open!, high: high!, low: low!, close: close!, volume: volume!,
}));
const H = 3_600_000;
const now = base[base.length - 1]!.time + H;

test("qualité : série propre = 100", () => {
  expect(assessQuality(base, H, "crypto", now).score).toBe(100);
});

test("qualité : trou, pic aberrant et données périmées (comme DataQuality.swift)", () => {
  const c = [...base.slice(0, 100), ...base.slice(105)];
  const i = 200;
  const p = c[i]!;
  c[i] = { ...p, high: p.close * 1.5, close: p.close * 1.5 };
  c[i + 1] = { ...c[i + 1]!, open: p.close * 1.5, high: Math.max(p.close * 1.5, c[i + 1]!.high) };
  const q = assessQuality(c, H, "crypto", c[c.length - 1]!.time + 86_400_000);
  expect(q.gaps).toBe(1);
  expect(q.badTicks).toBe(1);
  expect(q.stale).toBe(true);
  expect(q.score).toBeLessThan(50);
});

test("fiabilité : plafonds selon les sources indépendantes", () => {
  expect(reliability(100, 1, false)).toMatchObject({ score: 40, level: "low" });
  expect(reliability(100, 2, false)).toMatchObject({ score: 60, level: "medium" });
  expect(reliability(100, 3, false)).toMatchObject({ score: 75, level: "high" });
  expect(reliability(100, 4, false)).toMatchObject({ score: 90, level: "high" });
  expect(reliability(100, 12, false)).toMatchObject({ score: 100, level: "high" });
  expect(reliability(100, 12, true)).toMatchObject({ score: 30, level: "low" });
  expect(reliability(60, 12, false)).toMatchObject({ score: 60, level: "medium" });
});

test("garde-fou : signal suspendu si fiabilité faible, rétrogradé si moyenne", () => {
  const s = { ...analyze(base)!, action: "strongBuy" as const, confidence: 80 };
  const low = gate(s, reliability(100, 2, true));
  expect(low.action).toBe("hold");
  expect(low.warnings[0]).toContain("signal suspendu");
  expect(low.confidence).toBeCloseTo(24, 9);
  expect(gate(s, reliability(100, 1, false)).action).toBe("hold");
  expect(gate(s, reliability(100, 2, false)).action).toBe("buy");
  expect(gate(s, reliability(100, 3, false)).action).toBe("strongBuy");
});
