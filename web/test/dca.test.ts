import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { simulateDca } from "../src/engine/dca";
import type { Close } from "../src/engine/history";

const DAY = 86_400_000;
const d = (iso: string) => Date.parse(`${iso}T00:00:00Z`);
const sample = JSON.parse(readFileSync(join(import.meta.dir, "history-sample.json"), "utf8")) as { series: { symbol: string; closes: Close[] }[] };
const btc = sample.series.find((s) => s.symbol === "BTC")!.closes;

describe("investissement programmé", () => {
  test("calcul exact sur 3 achats", () => {
    // 100 $ on days 0, 7 and 14 at 10, 20 and 5 $; last close 10 $.
    const closes: Close[] = Array.from({ length: 21 }, (_, i) => [d("2026-09-01") + i * DAY, i < 7 ? 10 : i < 14 ? 20 : i < 20 ? 5 : 10]);
    const r = simulateDca(closes, 100, 7, 20, d("2026-09-22"))!;
    expect(r.buys).toBe(3);
    expect(r.invested).toBe(300);
    expect(r.units).toBeCloseTo(10 + 5 + 20, 9);
    expect(r.value).toBeCloseTo(350, 9);
    expect(r.gain).toBeCloseTo(50 / 3, 9); // 350 $ for 300 $
    expect(r.averagePrice).toBeCloseTo(300 / 35, 9);
    // All at once on day 0 at 10 $: 30 units → 300 $, no gain.
    expect(r.lumpSum.value).toBeCloseTo(300, 9);
    expect(r.lumpSum.gain).toBeCloseTo(0, 9);
    expect(r.path.at(-1)!.value).toBeCloseTo(350, 9);
  });

  test("action : un achat tombant un week-end se fait à la clôture suivante", () => {
    // Trading days only (Mon–Fri), purchase every 7 days starting on a Monday: always the next Monday.
    const closes: Close[] = [];
    for (let i = 0; i < 28; i++) {
      const t = d("2026-08-31") + i * DAY; // Monday 31 Aug.
      const wd = new Date(t).getUTCDay();
      if (wd !== 0 && wd !== 6) closes.push([t, 100 + i]);
    }
    const r = simulateDca(closes, 50, 7, 27, d("2026-09-28"))!;
    expect(r.buys).toBe(4);
    expect(r.invested).toBe(200);
  });

  test("vraies clôtures BTC (90 jours) : 100 $ par semaine", () => {
    const r = simulateDca(btc, 100, 7, 90)!;
    expect(r.buys).toBe(13);
    expect(r.invested).toBe(1300);
    expect(r.lastPrice).toBeCloseTo(84464.605, 3);
    expect(r.value).toBeCloseTo(r.units * 84464.605, 6);
    expect(r.averagePrice).toBeGreaterThan(60000);
    expect(r.averagePrice).toBeLessThan(84464.605);
    // BTC rose over the period: buying everything on day one did better than spreading the purchases.
    expect(r.lumpSum.gain).toBeGreaterThan(r.gain);
  });

  test("période plus longue que l'historique, montants invalides", () => {
    expect(simulateDca(btc, 100, 30, 365)).toBeNull();
    expect(simulateDca(btc, 0, 7, 90)).toBeNull();
    expect(simulateDca([], 100, 7, 90)).toBeNull();
  });
});
