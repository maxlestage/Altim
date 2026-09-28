/**
 * Captures real market inputs once (candles from the exchanges, through the TypeScript market layer), so the Rust
 * engines are compared with the TypeScript ones on real data: bun parity/capture.ts
 * Output: tests/golden/inputs.json (committed, re-run only to refresh).
 */
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { longDaily, snapshot, type Interval } from "../../web/server/market";

const ASSETS: [string, "crypto" | "stock"][] = [["BTC", "crypto"], ["ETH", "crypto"], ["SOL", "crypto"], ["DOGE", "crypto"], ["AAPL", "stock"], ["NVDA", "stock"], ["SPY", "stock"]];
const out: { symbol: string; kind: string; interval: string; candles: unknown[] }[] = [];
for (const [symbol, kind] of ASSETS) {
  for (const interval of ["1h", "4h", "1d"] as Interval[]) {
    try {
      const s = await snapshot(symbol, kind, interval);
      out.push({ symbol, kind, interval, candles: s.candles });
      console.log(symbol, interval, s.candles.length, s.source);
    } catch (e) {
      console.log(symbol, interval, "échec", (e as Error).message);
    }
  }
  try {
    const l = await longDaily(symbol, kind);
    out.push({ symbol, kind, interval: "long", candles: l.candles });
    console.log(symbol, "long", l.candles.length, l.source);
  } catch (e) {
    console.log(symbol, "long échec", (e as Error).message);
  }
}
writeFileSync(join(import.meta.dir, "../tests/golden/inputs.json"), JSON.stringify(out));
