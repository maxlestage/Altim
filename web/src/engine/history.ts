/**
 * History of the portfolio: what the lines held today were worth each day of the period, from the daily closes of
 * each asset, compared with Bitcoin and the S&P 500 held over the same days. Past purchases and sales are not known:
 * the curve answers "how did what I own now behave", not "how did my account do".
 */

export type Close = [time: number, close: number];

export interface HistoryLine {
  id: string;
  quantity: number;
}

export interface HistoryPoint {
  t: number;
  value: number;
}

export interface Benchmark {
  id: string;
  label: string;
  /** Change since the first day of the curve, in %. */
  change: number;
  /** Change in % since the first day, one point per day of the curve. */
  points: { t: number; pct: number }[];
}

export interface PortfolioHistory {
  points: HistoryPoint[];
  /** Change of the value between the first and the last day, in %. */
  change: number;
  /** Deepest fall from a previous high, in % (negative or 0). */
  maxDrawdown: number;
  best: { t: number; change: number } | null;
  worst: { t: number; change: number } | null;
  benchmarks: Benchmark[];
  /** Lines left out: no history on the whole period (asset too recent, source down). */
  missing: string[];
  /** The curve starts later than asked because a line has a shorter history. */
  shortened: boolean;
}

export const BENCHMARKS = [
  { id: "crypto:BTC", label: "Bitcoin" },
  { id: "stock:SPY", label: "S&P 500 (SPY)" },
] as const;

const DAY = 86_400_000;
const dayOf = (t: number) => Math.floor(t / DAY) * DAY;

/** Last close known at the end of each day of the grid (weekends and holidays keep the Friday close). */
function onGrid(closes: Close[], grid: number[]): (number | null)[] {
  const sorted = [...closes].filter(([t, c]) => Number.isFinite(t) && Number.isFinite(c) && c > 0).sort((a, b) => a[0] - b[0]);
  const out: (number | null)[] = [];
  let i = 0;
  let last: number | null = null;
  for (const d of grid) {
    while (i < sorted.length && dayOf(sorted[i]![0]) <= d) last = sorted[i++]![1];
    out.push(last);
  }
  return out;
}

export function portfolioHistory(lines: HistoryLine[], series: Record<string, Close[]>, days: number, now = Date.now()): PortfolioHistory | null {
  const held = lines.filter((l) => l.quantity > 0);
  // The curve ends on the last closed day of the held assets (only closed candles are served).
  const lastTimes = held.flatMap((l) => (series[l.id] ?? []).map((c) => c[0])).filter((t) => t <= now);
  const end = dayOf(lastTimes.length ? Math.max(...lastTimes) : now);
  const grid = Array.from({ length: days + 1 }, (_, i) => end - (days - i) * DAY);
  const missing: string[] = [];
  const valued: { q: number; closes: (number | null)[] }[] = [];
  for (const l of held) {
    const g = onGrid(series[l.id] ?? [], grid);
    if (g[g.length - 1] == null) missing.push(l.id);
    else valued.push({ q: l.quantity, closes: g });
  }
  if (!valued.length) return null;
  // The curve starts on the first day every line has a price: an asset listed later would otherwise look like a gain.
  let start = grid.findIndex((_, i) => valued.every((v) => v.closes[i] != null));
  if (start < 0 || grid.length - start < 2) return null;
  const points: HistoryPoint[] = [];
  for (let i = start; i < grid.length; i++) points.push({ t: grid[i]!, value: valued.reduce((s, v) => s + v.q * v.closes[i]!, 0) });

  const first = points[0]!.value;
  const last = points[points.length - 1]!.value;
  let peak = first;
  let maxDrawdown = 0;
  let best: PortfolioHistory["best"] = null;
  let worst: PortfolioHistory["worst"] = null;
  for (let i = 0; i < points.length; i++) {
    const v = points[i]!.value;
    peak = Math.max(peak, v);
    maxDrawdown = Math.min(maxDrawdown, (v / peak - 1) * 100);
    if (i > 0) {
      const c = (v / points[i - 1]!.value - 1) * 100;
      if (!best || c > best.change) best = { t: points[i]!.t, change: c };
      if (!worst || c < worst.change) worst = { t: points[i]!.t, change: c };
    }
  }

  const benchmarks: Benchmark[] = [];
  for (const b of BENCHMARKS) {
    const g = onGrid(series[b.id] ?? [], grid).slice(start);
    const base = g[0];
    if (base == null || g.some((c) => c == null)) continue;
    const pts = g.map((c, i) => ({ t: points[i]!.t, pct: (c! / base - 1) * 100 }));
    benchmarks.push({ id: b.id, label: b.label, change: pts[pts.length - 1]!.pct, points: pts });
  }

  return {
    points,
    change: (last / first - 1) * 100,
    maxDrawdown,
    best: best && best.change > 0 ? best : null,
    worst: worst && worst.change < 0 ? worst : null,
    benchmarks,
    missing,
    shortened: start > 0,
  };
}
