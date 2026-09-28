/**
 * Golden outputs of the TypeScript engines on real inputs (tests/golden/inputs.json), compared field by field by
 * the Rust tests (tests/parity.rs): bun parity/golden.ts
 * Each section writes tests/golden/<name>.json = [{ args, output }].
 */
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Candle } from "../../web/src/engine/signal";
import { analyze } from "../../web/src/engine/signal";

export type Input = { symbol: string; kind: "crypto" | "stock"; interval: "1h" | "4h" | "1d" | "long"; candles: Candle[] };
export const DIR = join(import.meta.dir, "../tests/golden");
/** The TypeScript server (web/server) was replaced by the Rust one: the golden files recorded from it are frozen. */
export const SERVER = existsSync(join(import.meta.dir, "../../web/server"));
export const inputs = JSON.parse(readFileSync(join(DIR, "inputs.json"), "utf8")) as Input[];
export const find = (symbol: string, interval: string) => inputs.find((i) => i.symbol === symbol && i.interval === interval);
/** A fixed "now" just after the capture, so time-dependent outputs are reproducible. */
export const NOW = Math.max(...inputs.map((i) => i.candles.at(-1)!.time)) + 3_600_000;
export function write(name: string, cases: { args: unknown; output: unknown }[]) {
  writeFileSync(join(DIR, `${name}.json`), JSON.stringify(cases));
  console.log(name, cases.length);
}

const HIGHER: Record<string, string | null> = { "1h": "4h", "4h": "1d", "1d": null };

// ---------- signal ----------
{
  const cases = [];
  for (const i of inputs.filter((x) => x.interval !== "long")) {
    const h = HIGHER[i.interval] ? find(i.symbol, HIGHER[i.interval]!) : undefined;
    for (const cut of [0, 7, 40]) {
      const candles = i.candles.slice(0, i.candles.length - cut);
      const higher = h?.candles.filter((c) => c.time <= candles.at(-1)!.time);
      const args = { symbol: i.symbol, interval: i.interval, cut, withHigher: !!higher };
      cases.push({ args, output: analyze(candles, { higher, intervalMs: 3_600_000, now: NOW }) });
    }
  }
  write("signal", cases);
}

// ---------- reliability ----------
{
  const { assessQuality, reliability, gate } = await import("../../web/src/engine/reliability");
  const STEP: Record<string, number> = { "1h": 3_600_000, "4h": 14_400_000, "1d": 86_400_000, long: 86_400_000 };
  const cases = [];
  for (const i of inputs) {
    for (const cut of [0, 250]) {
      const candles = i.candles.slice(cut);
      const q = assessQuality(candles, STEP[i.interval]!, i.kind, NOW);
      for (const [n, conflict] of [[1, false], [2, false], [3, false], [5, false], [4, true]] as const) {
        const rel = reliability(q.score, n, conflict);
        const s = analyze(candles);
        cases.push({ args: { symbol: i.symbol, interval: i.interval, cut, n, conflict }, output: { q, rel, gated: s ? gate(s, rel, q.issues) : null } });
      }
    }
  }
  write("reliability", cases);
}

// Other engines: one file per module, imported here (parity/golden-*.ts).
for (const m of ["engines-a", "engines-b", "engines-b1", ...(SERVER ? ["server"] : [])]) {
  try {
    await import(`./golden-${m}.ts`);
  } catch (e) {
    if (!String(e).includes("Cannot find module")) throw e;
  }
}

