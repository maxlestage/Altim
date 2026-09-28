import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { compareAssets, positionSize, projection, rebalance, saleAfterTax, saleTotal } from "../src/engine/tools";
import type { Close } from "../src/engine/history";

const DAY = 86_400_000;
const d = (iso: string) => Date.parse(`${iso}T00:00:00Z`);
const sample = JSON.parse(readFileSync(join(import.meta.dir, "history-sample.json"), "utf8")) as { asOf: number; series: { symbol: string; kind: string; closes: Close[] }[] };
const real: Record<string, Close[]> = Object.fromEntries(sample.series.map((s) => [`${s.kind}:${s.symbol}`, s.closes]));

describe("comparateur", () => {
  test("sur les vraies clôtures : mêmes jours, variation, volatilité, corrélation", () => {
    const c = compareAssets(real, ["crypto:BTC", "crypto:ETH", "stock:AAPL"], 90, sample.asOf)!;
    expect(c.stats.map((s) => s.id)).toEqual(["crypto:BTC", "crypto:ETH", "stock:AAPL"]);
    for (const s of c.stats) {
      expect(s.pct.length).toBe(c.days.length);
      expect(s.pct[0]).toBe(0);
      expect(s.change).toBeCloseTo(s.pct.at(-1)!, 9);
      expect(s.maxDrawdown).toBeLessThanOrEqual(0);
      expect(s.volatility).toBeGreaterThan(5);
    }
    // Ether moves more than Bitcoin, which moves more than Apple; the two cryptos move together.
    const [btc, eth, aapl] = c.stats;
    expect(eth!.volatility).toBeGreaterThan(btc!.volatility);
    expect(btc!.volatility).toBeGreaterThan(aapl!.volatility);
    expect(c.correlation[0]![1]!).toBeGreaterThan(0.5);
    expect(c.correlation[1]![0]).toBe(c.correlation[0]![1]);
    expect(c.correlation[2]![2]).toBe(1);
  });

  test("calcul exact et actif plus jeune", () => {
    const a: Close[] = [100, 110, 99, 120].map((v, i) => [d("2026-09-01") + i * DAY, v]);
    const b: Close[] = [50, 55].map((v, i) => [d("2026-09-03") + i * DAY, v]);
    const c = compareAssets({ "crypto:A": a, "crypto:B": b }, ["crypto:A", "crypto:B", "crypto:GONE"], 30, d("2026-09-05"))!;
    expect(c.missing).toEqual(["crypto:GONE"]);
    // The common period starts on 3 Sept., the first day of B.
    expect(c.days[0]).toBe(d("2026-09-03"));
    expect(c.stats[0]!.pct.map((v) => Math.round(v * 100) / 100)).toEqual([0, 21.21]);
    expect(c.stats[0]!.maxDrawdown).toBe(0);
    expect(c.stats[1]!.change).toBeCloseTo(10, 9);
    expect(c.correlation[0]![1]).toBeNull(); // fewer than 20 common days
  });
});

describe("taille de position", () => {
  test("1 % de risque, stop à 5 % : 20 % du capital", () => {
    const p = positionSize({ capital: 10_000, riskPct: 1, entry: 100, stop: 95, target: 110 })!;
    expect(p.quantity).toBeCloseTo(20, 9);
    expect(p.amount).toBeCloseTo(2000, 9);
    expect(p.capitalShare).toBeCloseTo(20, 9);
    expect(p.risk).toBeCloseTo(100, 9);
    expect(p.stopDistance).toBeCloseTo(5, 9);
    expect(p.reward).toBeCloseTo(200, 9);
    expect(p.ratio).toBeCloseTo(2, 9);
    expect(p.capped).toBe(false);
  });

  test("stop très proche : la taille est plafonnée au capital", () => {
    const p = positionSize({ capital: 1000, riskPct: 2, entry: 100, stop: 99.5 })!;
    expect(p.capped).toBe(true);
    expect(p.amount).toBeCloseTo(1000, 9);
    expect(p.risk).toBeCloseTo(5, 9);
    expect(p.reward).toBeNull();
  });

  test("entrées impossibles", () => {
    expect(positionSize({ capital: 1000, riskPct: 1, entry: 100, stop: 100 })).toBeNull();
    expect(positionSize({ capital: 1000, riskPct: 1, entry: 100, stop: 120 })).toBeNull();
    expect(positionSize({ capital: 0, riskPct: 1, entry: 100, stop: 90 })).toBeNull();
  });
});

describe("si je vendais : frais et flat tax", () => {
  test("gain, perte, coût inconnu", () => {
    // 10 000 $ sold, bought 6 000 $, fees 0,1 % = 10 $: gain 3 990 $, tax 30 % = 1 197 $.
    expect(saleAfterTax({ id: "a", value: 10_000, cost: 6000 })).toEqual({ gross: 10_000, fees: 10, gain: 3990, tax: 1197, net: 8793 });
    // A loss pays no tax.
    expect(saleAfterTax({ id: "b", value: 5000, cost: 7000 })).toMatchObject({ tax: 0, net: 4995, gain: -2005 });
    expect(saleAfterTax({ id: "c", value: 1000, cost: null })).toMatchObject({ gain: null, tax: 0, net: 999 });
  });

  test("tout vendre : les pertes compensent les gains de l'année", () => {
    const t = saleTotal([{ id: "a", value: 10_000, cost: 6000 }, { id: "b", value: 5000, cost: 7000 }, { id: "c", value: 1000, cost: null }]);
    expect(t.gross).toBe(16_000);
    expect(t.fees).toBeCloseTo(16, 9);
    expect(t.gain).toBeCloseTo(3990 - 2005, 9);
    expect(t.tax).toBeCloseTo((3990 - 2005) * 0.3, 9);
    expect(t.net).toBeCloseTo(16_000 - 16 - (3990 - 2005) * 0.3, 9);
    expect(t.unknownCost).toBe(1);
    // Net loss overall: no tax at all.
    expect(saleTotal([{ id: "b", value: 5000, cost: 7000 }]).tax).toBe(0);
  });
});

describe("projection", () => {
  test("sans rendement, on retrouve ce qu'on a versé", () => {
    const p = projection(1000, 100, 10, 0);
    expect(p.length).toBe(11);
    expect(p.at(-1)!.value).toBeCloseTo(1000 + 100 * 120, 9);
    expect(p.at(-1)!.paid).toBeCloseTo(13_000, 9);
  });

  test("capital seul à 8 % par an : ×1,08 chaque année", () => {
    const p = projection(10_000, 0, 5, 8);
    expect(p[1]!.value).toBeCloseTo(10_800, 6);
    expect(p[5]!.value).toBeCloseTo(10_000 * 1.08 ** 5, 6);
  });

  test("versements mensuels : formule de la rente", () => {
    const r = 1.04 ** (1 / 12) - 1;
    const p = projection(0, 200, 20, 4);
    expect(p.at(-1)!.value).toBeCloseTo((200 * ((1 + r) ** 240 - 1)) / r, 6);
  });
});

describe("rééquilibrage", () => {
  test("vers 50 % crypto, 40 % actions, 10 % liquidités", () => {
    const r = rebalance(
      [
        { id: "crypto:BTC", kind: "crypto", value: 6000 },
        { id: "crypto:ETH", kind: "crypto", value: 2000 },
        { id: "stock:AAPL", kind: "stock", value: 1500 },
      ],
      500,
      { crypto: 50, stock: 40, cash: 10 },
    )!;
    expect(r.total).toBe(10_000);
    expect(r.current.crypto).toBeCloseTo(80, 9);
    expect(r.moves).toEqual({ crypto: -3000, stock: 2500, cash: 500 });
    expect(r.lines).toEqual([
      { id: "crypto:BTC", amount: -2250 },
      { id: "crypto:ETH", amount: -750 },
      { id: "stock:AAPL", amount: 2500 },
    ]);
    // Buying and selling balance out.
    expect(r.moves.crypto + r.moves.stock + r.moves.cash).toBeCloseTo(0, 9);
  });

  test("cible qui ne fait pas 100 %", () => {
    expect(rebalance([{ id: "crypto:BTC", kind: "crypto", value: 1 }], 0, { crypto: 60, stock: 30, cash: 0 })).toBeNull();
  });
});
