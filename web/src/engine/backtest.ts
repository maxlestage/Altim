/**
 * Backtest (long only, no look-ahead bias):
 * signal computed on the closed candle i, executed at the open of i + 1, 0.1 % fees,
 * stop assumed hit first when stop and target fall in the same candle.
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
  const candles = sanitize(raw);
  let equity = 1;
  const curve: { time: number; equity: number }[] = [];
  const trades: BacktestTrade[] = [];
  let barsInMarket = 0;
  type Position = { entryTime: number; entry: number; stop: number; target: number; units: number };
  let position: Position | null = null;
  let pendingEntry: { entry: number; stopLoss: number } | null = null;
  let pendingExit = false;

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
    return { trades: [], equity: [], totalReturnPercent: 0, buyAndHoldPercent: 0, winRatePercent: 0, maxDrawdownPercent: 0, exposurePercent: 0 };
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
  return {
    trades,
    equity: curve,
    totalReturnPercent: (equity - 1) * 100,
    buyAndHoldPercent: (lastCandle.close / candles[warmup]!.open - 1) * 100,
    winRatePercent: trades.length ? (wins.length / trades.length) * 100 : 0,
    maxDrawdownPercent: maxDD * 100,
    exposurePercent: tested > 0 ? (barsInMarket / tested) * 100 : 0,
  };
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
