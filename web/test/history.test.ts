import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { portfolioHistory, type Close } from "../src/engine/history";

// Real answer of /api/history (ETH, AAPL + Bitcoin and SPY), captured on 28 September 2026.
const sample = JSON.parse(readFileSync(join(import.meta.dir, "history-sample.json"), "utf8")) as { asOf: number; series: { symbol: string; kind: string; closes: Close[] }[] };
const real: Record<string, Close[]> = Object.fromEntries(sample.series.map((s) => [`${s.kind}:${s.symbol}`, s.closes]));

const DAY = 86_400_000;
const d = (iso: string) => Date.parse(`${iso}T00:00:00Z`);

describe("historique du portefeuille", () => {
  test("sur les vraies clôtures : dernière valeur = quantités × derniers cours, comparaison BTC et S&P 500", () => {
    const h = portfolioHistory([{ id: "crypto:ETH", quantity: 2 }, { id: "stock:AAPL", quantity: 10 }], real, 90, sample.asOf)!;
    expect(h.points.length).toBe(91);
    expect(h.shortened).toBe(false);
    expect(h.missing).toEqual([]);
    // Last closed day of ETH (27 Sept.); AAPL keeps its Friday close over the weekend.
    expect(h.points.at(-1)!.t).toBe(d("2026-09-27"));
    expect(h.points.at(-1)!.value).toBeCloseTo(2 * 2688.32 + 10 * 341.07, 1);
    // One point per day, no gap.
    for (let i = 1; i < h.points.length; i++) expect(h.points[i]!.t - h.points[i - 1]!.t).toBe(DAY);
    expect(h.change).toBeCloseTo((h.points.at(-1)!.value / h.points[0]!.value - 1) * 100, 6);
    expect(h.maxDrawdown).toBeLessThanOrEqual(0);
    expect(h.benchmarks.map((b) => b.label)).toEqual(["Bitcoin", "S&P 500 (SPY)"]);
    const btc = h.benchmarks[0]!;
    expect(btc.points.length).toBe(h.points.length);
    expect(btc.points[0]!.pct).toBe(0);
    const btcStart = real["crypto:BTC"]!.find(([t]) => t === h.points[0]!.t)![1];
    expect(btc.change).toBeCloseTo((84464.605 / btcStart - 1) * 100, 6);
    // The iPhone and Android kits check the same figures on the same file.
    expect(Math.abs(h.change - 45.5)).toBeLessThan(0.05);
    expect(Math.abs(btc.change - 40.3)).toBeLessThan(0.05);
    expect(Math.abs(h.maxDrawdown + 6.8)).toBeLessThan(0.05);
  });

  test("week-end et jours fériés : l'action garde son cours de clôture", () => {
    const series = { "stock:X": [[d("2026-09-18"), 100], [d("2026-09-21"), 110]] as Close[] };
    const h = portfolioHistory([{ id: "stock:X", quantity: 1 }], series, 3, d("2026-09-21") + 3_600_000)!;
    expect(h.points.map((p) => p.value)).toEqual([100, 100, 100, 110]);
    expect(h.best!.t).toBe(d("2026-09-21"));
    expect(h.best!.change).toBeCloseTo(10, 9);
    expect(h.worst).toBeNull();
  });

  test("un actif coté plus tard raccourcit la courbe au lieu d'inventer un gain", () => {
    const series = {
      "crypto:A": Array.from({ length: 11 }, (_, i) => [d("2026-09-01") + i * DAY, 10] as Close),
      "crypto:NEW": Array.from({ length: 4 }, (_, i) => [d("2026-09-08") + i * DAY, 5] as Close),
    };
    const h = portfolioHistory([{ id: "crypto:A", quantity: 1 }, { id: "crypto:NEW", quantity: 2 }], series, 10, d("2026-09-11") + 1)!;
    expect(h.shortened).toBe(true);
    expect(h.points[0]!.t).toBe(d("2026-09-08"));
    expect(h.points.every((p) => p.value === 20)).toBe(true);
    expect(h.change).toBe(0);
  });

  test("perte maximale depuis un sommet, actif sans historique signalé", () => {
    const closes = [100, 120, 90, 108, 60, 80].map((c, i) => [d("2026-09-01") + i * DAY, c] as Close);
    const h = portfolioHistory([{ id: "crypto:A", quantity: 1 }, { id: "crypto:GONE", quantity: 3 }], { "crypto:A": closes }, 5, d("2026-09-06") + 1)!;
    expect(h.maxDrawdown).toBeCloseTo(-50, 6); // 120 → 60
    expect(h.worst!.change).toBeCloseTo(-44.444, 2); // 108 → 60
    expect(h.missing).toEqual(["crypto:GONE"]);
    expect(h.benchmarks).toEqual([]);
    expect(portfolioHistory([{ id: "crypto:GONE", quantity: 1 }], {}, 30)).toBeNull();
  });
});
