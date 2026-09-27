import { expect, test } from "bun:test";
import samples from "./samples.json";
import { aggregate, consensus, deviations, parse } from "../server/market";
import type { Candle } from "../src/engine/signal";

// Real responses captured on 27/09/2026.
test("parseurs : mêmes bougies quel que soit le format de l'exchange", () => {
  const at = (c: Candle[], t: number) => c.find((x) => x.time === t)!;
  const t = 1_790_496_000_000; // closed candle 07:00 UTC
  const okx = at(parse.okx(samples.okx), t);
  const kucoin = at(parse.kucoin(samples.kucoin), t);
  const gate = at(parse.gate(samples.gate), t);
  const coinbase = at(parse.coinbase(samples.coinbase), t);
  const kraken = at(parse.kraken(samples.kraken), t);
  expect(okx.close).toBe(84791.7);
  expect(kucoin.open).toBe(84781.9);
  expect(gate.close).toBe(84788.3);
  expect(coinbase.close).toBe(84775.68);
  expect(kraken.close).toBe(84774.7);
  for (const c of [okx, kucoin, gate, coinbase, kraken]) {
    expect(c.high).toBeGreaterThanOrEqual(Math.max(c.open, c.close));
    expect(c.low).toBeLessThanOrEqual(Math.min(c.open, c.close));
  }
  // Every exchange agrees within 0.05 %.
  const closes = [okx, kucoin, gate, coinbase, kraken].map((c) => c.close);
  expect((Math.max(...closes) / Math.min(...closes) - 1) * 100).toBeLessThan(0.05);
});

const series = (factor: number): Candle[] =>
  Array.from({ length: 100 }, (_, i) => ({ time: i * 3_600_000, open: 100 * factor, high: 102 * factor, low: 98 * factor, close: (100 + (i % 5)) * factor, volume: 1 }));

test("écart : une source fausse est détectée", () => {
  const d = deviations([series(1), series(1.0002), series(1.05)]);
  expect(d[0]!).toBeLessThan(0.1);
  expect(d[2]!).toBeGreaterThan(4);
});

test("consensus : bascule sur les sources suivantes et écarte la source divergente", async () => {
  const src = (name: string, f: number | Error) => ({
    name,
    fetch: async () => {
      if (f instanceof Error) throw f;
      return series(f);
    },
  });
  const r = await consensus("BTC", "1h", [src("Panne", new Error("451")), src("Faux", 1.05), src("A", 1), src("B", 1.0001), src("C", 0.9999)], 3);
  expect(r.source).toBe("A");
  expect(r.sources.find((s) => s.name === "Faux")!.ok).toBe(false);
  expect(r.sources.find((s) => s.name === "Panne")!.error).toBe("451");
});

test("agrégation 1 h → 4 h", () => {
  const h = Array.from({ length: 8 }, (_, i) => ({ time: i * 3_600_000, open: i, high: i + 1, low: i - 1, close: i + 0.5, volume: 1 }));
  const a = aggregate(h, 3_600_000, 14_400_000);
  expect(a.length).toBe(2);
  expect(a[0]).toEqual({ time: 0, open: 0, high: 4, low: -1, close: 3.5, volume: 4 });
});
