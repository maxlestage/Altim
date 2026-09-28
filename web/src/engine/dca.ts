/**
 * "What if I had invested X $ every week / month": regular purchases at the daily close, over real past prices,
 * compared with the same total invested at once on the first day. No fees, no taxes; the past does not predict the
 * future. Same computation on the iPhone and Android.
 */
import type { Close } from "./history";

export interface DcaResult {
  /** Number of purchases made. */
  buys: number;
  invested: number;
  units: number;
  /** Value of the units at the last close. */
  value: number;
  /** Gain in % of the invested amount. */
  gain: number;
  /** Average price paid per unit. */
  averagePrice: number;
  lastPrice: number;
  /** The same total invested at once at the first purchase. */
  lumpSum: { value: number; gain: number };
  /** Value of the regular plan after each purchase day, for a small chart. */
  path: { t: number; invested: number; value: number }[];
  first: number;
  last: number;
}

const DAY = 86_400_000;

export function simulateDca(closes: Close[], amount: number, everyDays: number, days: number, now = Date.now()): DcaResult | null {
  if (!(amount > 0) || !(everyDays > 0)) return null;
  const sorted = closes.filter(([t, c]) => Number.isFinite(t) && c > 0 && t <= now).sort((a, b) => a[0] - b[0]);
  if (sorted.length < 2) return null;
  const end = sorted[sorted.length - 1]![0];
  const start = end - days * DAY;
  const inRange = sorted.filter(([t]) => t >= start);
  // The asset must have existed for the whole period, otherwise the comparison is meaningless.
  if (inRange.length < 2 || sorted[0]![0] > start + 7 * DAY) return null;

  let units = 0;
  let invested = 0;
  let next = inRange[0]![0];
  const path: DcaResult["path"] = [];
  for (const [t, c] of inRange) {
    if (t >= next) {
      units += amount / c;
      invested += amount;
      next += everyDays * DAY;
      // Weekends and holidays: the next purchase happens at the next close available.
      while (next <= t) next += everyDays * DAY;
    }
    path.push({ t, invested, value: units * c });
  }
  const lastPrice = inRange[inRange.length - 1]![1];
  const value = units * lastPrice;
  const firstPrice = inRange[0]![1];
  const lump = (invested / firstPrice) * lastPrice;
  return {
    buys: Math.round(invested / amount),
    invested,
    units,
    value,
    gain: (value / invested - 1) * 100,
    averagePrice: invested / units,
    lastPrice,
    lumpSum: { value: lump, gain: (lump / invested - 1) * 100 },
    path,
    first: inRange[0]![0],
    last: end,
  };
}
