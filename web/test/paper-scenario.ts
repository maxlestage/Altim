/**
 * Reference scenario of paper trading, replayed identically by the iPhone (PaperTests.swift) and Android
 * (PaperTest.kt) ports: bun test/paper-scenario.ts writes test/paper-fixture.json (checked by paper.test.ts).
 */
import { checkExits, closePosition, newPaper, openPosition, paperStats, valuation, type DailyCandle, type PaperState } from "../src/engine/paper";

const DAY = 86_400_000;
const T0 = Date.UTC(2026, 8, 1, 14, 0);
const c = (d: number, open: number, high: number, low: number, close: number): DailyCandle => ({ time: Date.UTC(2026, 8, 1 + d), open, high, low, close });

export type Step =
  | { op: "new"; capital: number; now: number }
  | { op: "open"; order: Parameters<typeof openPosition>[1]; now: number }
  | { op: "close"; id: string; price: number; now: number }
  | { op: "exits"; candles: Record<string, DailyCandle[]> }
  | { op: "value"; prices: Record<string, number | null> };

const buy = { verdict: "buy", label: "ACHETER", confidence: 70, asOf: T0 };
const zone = { verdict: "buyZone", label: "ZONE D'ACHAT", confidence: 55, asOf: T0 };
const wait = { verdict: "wait", label: "ATTENDRE", confidence: 50, asOf: T0 };

export const STEPS: Step[] = [
  { op: "new", capital: 10_000, now: T0 },
  // BTC: stop 60 000, target 72 000. Refused orders first (more than the cash, no price, zero amount).
  { op: "open", order: { id: "p1", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 64_000, amount: 20_000, stop: 60_000, target: 72_000, decision: buy }, now: T0 },
  { op: "open", order: { id: "p1", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 0, amount: 1_000, decision: buy }, now: T0 },
  { op: "open", order: { id: "p1", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 64_000, amount: 0, decision: buy }, now: T0 },
  { op: "open", order: { id: "p1", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 64_000, amount: 3_000, stop: 60_000, target: 72_000, decision: buy }, now: T0 },
  // AAPL: stop above the price (ignored), target 300.
  { op: "open", order: { id: "p2", symbol: "AAPL", kind: "stock", name: "Apple", price: 250, amount: 2_500, stop: 260, target: 300, decision: zone }, now: T0 + 1_000 },
  // SOL: no stop nor target (only a manual exit), opened under "ATTENDRE".
  { op: "open", order: { id: "p3", symbol: "SOL", kind: "crypto", name: "Solana", price: 150, amount: 1_500, decision: wait }, now: T0 + 2_000 },
  // ETH: no decision, gap below its stop.
  { op: "open", order: { id: "p4", symbol: "ETH", kind: "crypto", name: "Ethereum", price: 2_500, amount: 1_000, stop: 2_300, target: 3_000, decision: null }, now: T0 + 3_000 },
  { op: "value", prices: { "crypto:BTC": 66_000, "stock:AAPL": 240, "crypto:SOL": 160, "crypto:ETH": null } },
  {
    op: "exits",
    candles: {
      // Opening day ignored (its low 59 000 may be before the purchase); day 2: target reached.
      "crypto:BTC": [c(0, 63_000, 65_000, 59_000, 64_500), c(1, 64_500, 71_000, 63_000, 70_000), c(2, 70_000, 73_000, 69_000, 72_500)],
      // Both stop-free (260 ignored) and target 300 reached on day 3: target.
      "stock:AAPL": [c(1, 250, 280, 245, 270), c(3, 270, 305, 268, 301)],
      // Gap down under the stop: filled at the open 2 200.
      "crypto:ETH": [c(1, 2_200, 2_250, 2_150, 2_210)],
      "crypto:SOL": [c(1, 150, 400, 10, 160)],
    },
  },
  { op: "close", id: "p3", price: 140, now: T0 + 5 * DAY },
  { op: "close", id: "missing", price: 140, now: T0 + 5 * DAY },
  // A candle reaching both the stop and the target: the stop (worst case).
  { op: "open", order: { id: "p5", symbol: "BTC", kind: "crypto", name: "Bitcoin", price: 70_000, amount: 2_000, stop: 66_000, target: 76_000, decision: buy }, now: T0 + 6 * DAY },
  { op: "exits", candles: { "crypto:BTC": [c(7, 70_000, 77_000, 65_000, 71_000)] } },
  { op: "value", prices: {} },
];

export function replay(): { step: Step; error?: string; closed?: string[]; state: PaperState; valuation?: unknown; stats: unknown }[] {
  let s = newPaper(10_000, T0);
  return STEPS.map((step) => {
    let error: string | undefined;
    let closed: string[] | undefined;
    let val: unknown;
    if (step.op === "new") s = newPaper(step.capital, step.now);
    else if (step.op === "open") ({ state: s, error } = openPosition(s, step.order, step.now));
    else if (step.op === "close") ({ state: s, error } = closePosition(s, step.id, step.price, step.now));
    else if (step.op === "exits") {
      const r = checkExits(s, step.candles);
      s = r.state;
      closed = r.closed.map((t) => t.id);
    } else val = valuation(s, step.prices);
    return { step, ...(error ? { error } : {}), ...(closed ? { closed } : {}), state: s, ...(val ? { valuation: val } : {}), stats: paperStats(s) };
  });
}

if (import.meta.main) {
  await Bun.write(new URL("./paper-fixture.json", import.meta.url), JSON.stringify(replay(), null, 1));
  console.log("test/paper-fixture.json écrit");
}
