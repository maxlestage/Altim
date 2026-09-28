/**
 * Backtest (long only, no look-ahead bias):
 * signal computed on the closed candle i, executed at the open of i + 1, 0.1 % fees,
 * stop assumed hit first when stop and target fall in the same candle.
 * Also the trade statistics (expectancy, R multiples) and the split of the results by market regime.
 */
import { analyze, isBuy, isSell, sanitize, type Candle } from "./signal";

export interface BacktestTrade {
  entryTime: number;
  exitTime: number;
  entryPrice: number;
  exitPrice: number;
  returnPercent: number;
  exitReason: string;
}

export interface BacktestResult {
  trades: BacktestTrade[];
  equity: { time: number; equity: number }[];
  totalReturnPercent: number;
  buyAndHoldPercent: number;
  winRatePercent: number;
  maxDrawdownPercent: number;
  exposurePercent: number;
}

export function backtest(raw: Candle[], feeRate = 0.001, lookback = 250, rewardRisk = 2, warmup = 60): BacktestResult {
  return backtestWithRisk(raw, feeRate, lookback, rewardRisk, warmup).result;
}

/**
 * Same backtest, plus the initial risk of each trade: distance entry − stop in % of the entry price (one per trade,
 * same order), for the R multiples.
 */
export function backtestWithRisk(raw: Candle[], feeRate = 0.001, lookback = 250, rewardRisk = 2, warmup = 60): { result: BacktestResult; risks: number[] } {
  const candles = sanitize(raw);
  let equity = 1;
  const curve: { time: number; equity: number }[] = [];
  const trades: BacktestTrade[] = [];
  let barsInMarket = 0;
  type Position = { entryTime: number; entry: number; stop: number; target: number; units: number };
  let position: Position | null = null;
  let pendingEntry: { entry: number; stopLoss: number } | null = null;
  let pendingExit = false;
  const risks: number[] = [];

  const close = (price: number, time: number, reason: string) => {
    if (!position) return;
    const proceeds = position.units * price * (1 - feeRate);
    const cost = position.units * position.entry;
    equity = proceeds;
    trades.push({
      entryTime: position.entryTime, exitTime: time, entryPrice: position.entry, exitPrice: price,
      returnPercent: (proceeds / (cost / (1 - feeRate)) - 1) * 100, exitReason: reason,
    });
    position = null;
  };

  if (candles.length <= warmup + 1) {
    return { result: { trades: [], equity: [], totalReturnPercent: 0, buyAndHoldPercent: 0, winRatePercent: 0, maxDrawdownPercent: 0, exposurePercent: 0 }, risks };
  }

  for (let i = warmup; i < candles.length; i++) {
    const bar = candles[i]!;
    if (pendingExit) {
      close(bar.open, bar.time, "Signal de vente");
      pendingExit = false;
    }
    if (pendingEntry && !position) {
      const entry = bar.open;
      const distance = pendingEntry.entry - pendingEntry.stopLoss;
      risks.push((distance / entry) * 100);
      position = { entryTime: bar.time, entry, stop: entry - distance, target: entry + distance * rewardRisk, units: (equity * (1 - feeRate)) / entry };
      pendingEntry = null;
    }
    const p = position as Position | null;
    if (p) {
      barsInMarket++;
      if (bar.low <= p.stop) close(Math.min(bar.open, p.stop), bar.time, "Stop");
      else if (bar.high >= p.target) close(Math.max(bar.open, p.target), bar.time, "Objectif");
    }
    const held = position as Position | null;
    curve.push({ time: bar.time, equity: held ? held.units * bar.close : equity });

    if (i >= candles.length - 1) break;
    const signal = analyze(candles.slice(Math.max(0, i - lookback + 1), i + 1));
    if (!signal) continue;
    if (!position && isBuy(signal.action) && signal.hasPlan) pendingEntry = { entry: signal.price, stopLoss: signal.stopLoss };
    else if (position && isSell(signal.action)) pendingExit = true;
  }

  const lastCandle = candles[candles.length - 1]!;
  if (position) {
    close(lastCandle.close, lastCandle.time, "Fin du test");
    if (curve.length) curve[curve.length - 1] = { time: lastCandle.time, equity };
  }

  let peak = 0;
  let maxDD = 0;
  for (const pt of curve) {
    peak = Math.max(peak, pt.equity);
    if (peak > 0) maxDD = Math.max(maxDD, (peak - pt.equity) / peak);
  }
  const wins = trades.filter((t) => t.returnPercent > 0);
  const tested = candles.length - warmup;
  const result: BacktestResult = {
    trades,
    equity: curve,
    totalReturnPercent: (equity - 1) * 100,
    buyAndHoldPercent: (lastCandle.close / candles[warmup]!.open - 1) * 100,
    winRatePercent: trades.length ? (wins.length / trades.length) * 100 : 0,
    maxDrawdownPercent: maxDD * 100,
    exposurePercent: tested > 0 ? (barsInMarket / tested) * 100 : 0,
  };
  return { result, risks };
}

/** Track record of the buy signals on one asset (trades closed in the backtest, fees included). */
export interface TrackRecord { trades: number; winRate: number; avgReturn: number }

export function trackRecord(r: BacktestResult): TrackRecord {
  const n = r.trades.length;
  return {
    trades: n,
    winRate: n ? (r.trades.filter((t) => t.returnPercent > 0).length / n) * 100 : 0,
    avgReturn: n ? r.trades.reduce((a, t) => a + t.returnPercent, 0) / n : 0,
  };
}

// ---------- Trade statistics and market regimes (backend/src/engine/backtest.rs) ----------

/**
 * Expectancy per trade (%) = average win × win rate − |average loss| × loss rate, and the average R multiple
 * (return ÷ initial risk, trades whose risk is known and positive). null without trades.
 */
export interface TradeStats { expectancy: number | null; avgR: number | null }

export function tradeStats(returns: number[], risks: number[]): TradeStats {
  const n = returns.length;
  if (!n) return { expectancy: null, avgR: null };
  const wins = returns.filter((x) => x > 0);
  const losses = returns.filter((x) => x <= 0);
  const avg = (v: number[]) => (v.length ? v.reduce((a, b) => a + b, 0) / v.length : 0);
  const winRate = wins.length / n;
  const expectancy = avg(wins) * winRate - Math.abs(avg(losses)) * (1 - winRate);
  const rs = returns.flatMap((x, i) => {
    const r = risks[i];
    return i < risks.length && Number.isFinite(r) && r! > 0 ? [x / r!] : [];
  });
  return { expectancy, avgR: rs.length ? avg(rs) : null };
}

/** Market regime at one bar, from the candles up to it only (no look-ahead). */
export type Regime = "bull" | "bear" | "range" | "crisis" | "unknown";

export const REGIME_SMA = 200;
/** The SMA is "rising" when above its value this many bars earlier. */
export const REGIME_SLOPE = 20;
/** Drawdown from the 1-year high beyond which the market is in crisis (fraction). */
export const CRISIS_DRAWDOWN = 0.3;
const YEAR_MS = 365 * 86_400_000;
/** Below it, a regime is shown as "échantillon trop faible". */
export const REGIME_MIN_TRADES = 5;

/**
 * Crisis (drawdown > 30 % from the highest high of the last 365 days), else bull (close above a rising 200-bar
 * SMA), bear (below a falling one) or range; unknown with fewer than 220 candles up to `i`.
 */
export function regimeAt(candles: Candle[], i: number): Regime {
  if (i >= candles.length || i + 1 < REGIME_SMA + REGIME_SLOPE) return "unknown";
  const bar = candles[i]!;
  let high = 0;
  for (let k = i; k >= 0 && candles[k]!.time > bar.time - YEAR_MS; k--) high = Math.max(high, candles[k]!.high);
  if (high > 0 && 1 - bar.close / high > CRISIS_DRAWDOWN) return "crisis";
  const sma = (end: number) => {
    let s = 0;
    for (let k = end + 1 - REGIME_SMA; k <= end; k++) s += candles[k]!.close;
    return s / REGIME_SMA;
  };
  const now = sma(i);
  const before = sma(i - REGIME_SLOPE);
  if (bar.close > now && now > before) return "bull";
  if (bar.close < now && now < before) return "bear";
  return "range";
}

export interface RegimeStat {
  regime: Regime; label: string; trades: number; winRate: number | null; avgReturn: number | null;
  /** Fewer than 5 trades: "échantillon trop faible". */
  lowSample: boolean;
}

export const REGIME_LABEL: Record<Regime, string> = {
  bull: "Marché haussier",
  bear: "Marché baissier",
  range: "Marché sans tendance",
  crisis: "Crise (−30 % depuis le plus haut sur 1 an)",
  unknown: "Historique trop court pour classer",
};

/**
 * Results by regime of the market on the signal candle (the closed candle before each entry). `returns` has one
 * value per trade (e.g. net of costs). Bull, bear, range and crisis always listed; unknown only when it has trades.
 */
export function regimeSplit(raw: Candle[], trades: BacktestTrade[], returns: number[]): RegimeStat[] {
  const candles = sanitize(raw);
  const order: Regime[] = ["bull", "bear", "range", "crisis", "unknown"];
  const by: number[][] = order.map(() => []);
  trades.forEach((t, k) => {
    if (k >= returns.length) return;
    const e = candles.findIndex((c) => c.time === t.entryTime);
    const regime = e > 0 ? regimeAt(candles, e - 1) : "unknown";
    by[order.indexOf(regime)]!.push(returns[k]!);
  });
  return order.flatMap((regime, k) => {
    const v = by[k]!;
    const n = v.length;
    if (regime === "unknown" && !n) return [];
    return [{
      regime, label: REGIME_LABEL[regime], trades: n,
      winRate: n ? (v.filter((x) => x > 0).length / n) * 100 : null,
      avgReturn: n ? v.reduce((a, b) => a + b, 0) / n : null,
      lowSample: n < REGIME_MIN_TRADES,
    }];
  });
}
