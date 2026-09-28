//! Backtest (long only, no look-ahead bias), `web/src/engine/backtest.ts`:
//! signal computed on the closed candle i, executed at the open of i + 1, 0.1 % fees,
//! stop assumed hit first when stop and target fall in the same candle. Also the trade statistics (expectancy, R
//! multiples) and the split of the results by market regime.
use serde::{Deserialize, Serialize};

use super::signal::{AnalyzeOptions, Candle, analyze, is_buy, is_sell, sanitize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestTrade {
    pub entry_time: i64,
    pub exit_time: i64,
    pub entry_price: f64,
    pub exit_price: f64,
    pub return_percent: f64,
    pub exit_reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EquityPoint {
    pub time: i64,
    pub equity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BacktestResult {
    pub trades: Vec<BacktestTrade>,
    pub equity: Vec<EquityPoint>,
    pub total_return_percent: f64,
    pub buy_and_hold_percent: f64,
    pub win_rate_percent: f64,
    pub max_drawdown_percent: f64,
    pub exposure_percent: f64,
}

/// Defaults of the TypeScript signature.
pub const FEE_RATE: f64 = 0.001;
pub const LOOKBACK: usize = 250;
pub const REWARD_RISK: f64 = 2.0;
pub const WARMUP: usize = 60;

struct Position {
    entry_time: i64,
    entry: f64,
    stop: f64,
    target: f64,
    units: f64,
}

/// `backtest(raw)` with the default parameters.
pub fn backtest_default(raw: &[Candle]) -> BacktestResult {
    backtest(raw, FEE_RATE, LOOKBACK, REWARD_RISK, WARMUP)
}

pub fn backtest(raw: &[Candle], fee_rate: f64, lookback: usize, reward_risk: f64, warmup: usize) -> BacktestResult {
    backtest_with_risk(raw, fee_rate, lookback, reward_risk, warmup).0
}

/// Same backtest, plus the initial risk of each trade: distance entry − stop in % of the entry price (one per trade,
/// same order), for the R multiples.
pub fn backtest_with_risk(raw: &[Candle], fee_rate: f64, lookback: usize, reward_risk: f64, warmup: usize) -> (BacktestResult, Vec<f64>) {
    let candles = sanitize(raw);
    let mut equity = 1.0;
    let mut curve: Vec<EquityPoint> = Vec::new();
    let mut trades: Vec<BacktestTrade> = Vec::new();
    let mut bars_in_market = 0usize;
    let mut position: Option<Position> = None;
    let mut pending_entry: Option<(f64, f64)> = None; // (entry, stopLoss)
    let mut pending_exit = false;
    let mut risks: Vec<f64> = Vec::new();

    let close = |position: &mut Option<Position>, equity: &mut f64, trades: &mut Vec<BacktestTrade>, price: f64, time: i64, reason: &str| {
        let Some(p) = position.take() else { return };
        let proceeds = p.units * price * (1.0 - fee_rate);
        let cost = p.units * p.entry;
        *equity = proceeds;
        trades.push(BacktestTrade {
            entry_time: p.entry_time,
            exit_time: time,
            entry_price: p.entry,
            exit_price: price,
            return_percent: (proceeds / (cost / (1.0 - fee_rate)) - 1.0) * 100.0,
            exit_reason: reason.into(),
        });
    };

    if candles.len() <= warmup + 1 {
        let empty = BacktestResult {
            trades: vec![],
            equity: vec![],
            total_return_percent: 0.0,
            buy_and_hold_percent: 0.0,
            win_rate_percent: 0.0,
            max_drawdown_percent: 0.0,
            exposure_percent: 0.0,
        };
        return (empty, risks);
    }

    for i in warmup..candles.len() {
        let bar = candles[i];
        if pending_exit {
            close(&mut position, &mut equity, &mut trades, bar.open, bar.time, "Signal de vente");
            pending_exit = false;
        }
        if let Some((pe, psl)) = pending_entry {
            if position.is_none() {
                let entry = bar.open;
                let distance = pe - psl;
                risks.push(distance / entry * 100.0);
                position = Some(Position {
                    entry_time: bar.time,
                    entry,
                    stop: entry - distance,
                    target: entry + distance * reward_risk,
                    units: (equity * (1.0 - fee_rate)) / entry,
                });
                pending_entry = None;
            }
        }
        if let Some((stop, target)) = position.as_ref().map(|p| (p.stop, p.target)) {
            bars_in_market += 1;
            if bar.low <= stop {
                close(&mut position, &mut equity, &mut trades, bar.open.min(stop), bar.time, "Stop");
            } else if bar.high >= target {
                close(&mut position, &mut equity, &mut trades, bar.open.max(target), bar.time, "Objectif");
            }
        }
        curve.push(EquityPoint { time: bar.time, equity: position.as_ref().map(|h| h.units * bar.close).unwrap_or(equity) });

        if i >= candles.len() - 1 {
            break;
        }
        let from = (i + 1).saturating_sub(lookback);
        let Some(signal) = analyze(&candles[from..i + 1], &AnalyzeOptions::default()) else { continue };
        if position.is_none() && is_buy(signal.action) && signal.has_plan {
            pending_entry = Some((signal.price, signal.stop_loss));
        } else if position.is_some() && is_sell(signal.action) {
            pending_exit = true;
        }
    }

    let last_candle = candles[candles.len() - 1];
    if position.is_some() {
        close(&mut position, &mut equity, &mut trades, last_candle.close, last_candle.time, "Fin du test");
        if let Some(l) = curve.last_mut() {
            *l = EquityPoint { time: last_candle.time, equity };
        }
    }

    let mut peak: f64 = 0.0;
    let mut max_dd: f64 = 0.0;
    for pt in &curve {
        peak = peak.max(pt.equity);
        if peak > 0.0 {
            max_dd = max_dd.max((peak - pt.equity) / peak);
        }
    }
    let wins = trades.iter().filter(|t| t.return_percent > 0.0).count();
    let tested = candles.len() - warmup;
    let result = BacktestResult {
        total_return_percent: (equity - 1.0) * 100.0,
        buy_and_hold_percent: (last_candle.close / candles[warmup].open - 1.0) * 100.0,
        win_rate_percent: if trades.is_empty() { 0.0 } else { wins as f64 / trades.len() as f64 * 100.0 },
        max_drawdown_percent: max_dd * 100.0,
        exposure_percent: if tested > 0 { bars_in_market as f64 / tested as f64 * 100.0 } else { 0.0 },
        trades,
        equity: curve,
    };
    (result, risks)
}

/// Track record of the buy signals on one asset (trades closed in the backtest, fees included).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRecord {
    pub trades: usize,
    pub win_rate: f64,
    pub avg_return: f64,
}

pub fn track_record(r: &BacktestResult) -> TrackRecord {
    let n = r.trades.len();
    TrackRecord {
        trades: n,
        win_rate: if n > 0 { r.trades.iter().filter(|t| t.return_percent > 0.0).count() as f64 / n as f64 * 100.0 } else { 0.0 },
        avg_return: if n > 0 { r.trades.iter().fold(0.0, |a, t| a + t.return_percent) / n as f64 } else { 0.0 },
    }
}

// ---------- Trade statistics and market regimes (`web/src/engine/backtest.ts`) ----------

/// Expectancy per trade (%) = average win × win rate − |average loss| × loss rate, and the average R multiple
/// (return ÷ initial risk, trades whose risk is known and positive). None without trades.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeStats {
    pub expectancy: Option<f64>,
    pub avg_r: Option<f64>,
}

pub fn trade_stats(returns: &[f64], risks: &[f64]) -> TradeStats {
    let n = returns.len();
    if n == 0 {
        return TradeStats { expectancy: None, avg_r: None };
    }
    let wins: Vec<f64> = returns.iter().copied().filter(|x| *x > 0.0).collect();
    let losses: Vec<f64> = returns.iter().copied().filter(|x| *x <= 0.0).collect();
    let avg = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    let win_rate = wins.len() as f64 / n as f64;
    let expectancy = avg(&wins) * win_rate - avg(&losses).abs() * (1.0 - win_rate);
    let rs: Vec<f64> = returns.iter().zip(risks).filter(|(_, r)| r.is_finite() && **r > 0.0).map(|(x, r)| x / r).collect();
    TradeStats { expectancy: Some(expectancy), avg_r: (!rs.is_empty()).then(|| avg(&rs)) }
}

/// Market regime at one bar, from the candles up to it only (no look-ahead).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Regime {
    Bull,
    Bear,
    Range,
    Crisis,
    Unknown,
}

pub const REGIME_SMA: usize = 200;
/// The SMA is "rising" when above its value this many bars earlier.
pub const REGIME_SLOPE: usize = 20;
/// Drawdown from the 1-year high beyond which the market is in crisis (fraction).
pub const CRISIS_DRAWDOWN: f64 = 0.30;
const YEAR_MS: i64 = 365 * 86_400_000;
/// Below it, a regime is shown as "échantillon trop faible".
pub const REGIME_MIN_TRADES: usize = 5;

/// Crisis (drawdown > 30 % from the highest high of the last 365 days), else bull (close above a rising 200-bar
/// SMA), bear (below a falling one) or range; unknown with fewer than 220 candles up to `i`.
pub fn regime_at(candles: &[Candle], i: usize) -> Regime {
    if i >= candles.len() || i + 1 < REGIME_SMA + REGIME_SLOPE {
        return Regime::Unknown;
    }
    let bar = candles[i];
    let high = candles[..=i].iter().rev().take_while(|c| c.time > bar.time - YEAR_MS).fold(0.0f64, |a, c| a.max(c.high));
    if high > 0.0 && 1.0 - bar.close / high > CRISIS_DRAWDOWN {
        return Regime::Crisis;
    }
    let sma = |end: usize| candles[end + 1 - REGIME_SMA..=end].iter().map(|c| c.close).sum::<f64>() / REGIME_SMA as f64;
    let (now, before) = (sma(i), sma(i - REGIME_SLOPE));
    if bar.close > now && now > before {
        Regime::Bull
    } else if bar.close < now && now < before {
        Regime::Bear
    } else {
        Regime::Range
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegimeStat {
    pub regime: Regime,
    pub label: String,
    pub trades: usize,
    pub win_rate: Option<f64>,
    pub avg_return: Option<f64>,
    /// Fewer than 5 trades: "échantillon trop faible".
    pub low_sample: bool,
}

pub fn regime_label(r: Regime) -> &'static str {
    match r {
        Regime::Bull => "Marché haussier",
        Regime::Bear => "Marché baissier",
        Regime::Range => "Marché sans tendance",
        Regime::Crisis => "Crise (−30 % depuis le plus haut sur 1 an)",
        Regime::Unknown => "Historique trop court pour classer",
    }
}

/// Results by regime of the market on the signal candle (the closed candle before each entry). `returns` has one
/// value per trade (e.g. net of costs). Bull, bear, range and crisis always listed; unknown only when it has trades.
pub fn regime_split(raw: &[Candle], trades: &[BacktestTrade], returns: &[f64]) -> Vec<RegimeStat> {
    let candles = sanitize(raw);
    let order = [Regime::Bull, Regime::Bear, Regime::Range, Regime::Crisis, Regime::Unknown];
    let mut by: Vec<Vec<f64>> = vec![Vec::new(); order.len()];
    for (t, x) in trades.iter().zip(returns) {
        let regime = match candles.iter().position(|c| c.time == t.entry_time) {
            Some(e) if e > 0 => regime_at(&candles, e - 1),
            _ => Regime::Unknown,
        };
        by[order.iter().position(|r| *r == regime).unwrap_or(4)].push(*x);
    }
    order
        .iter()
        .zip(by)
        .filter(|(r, v)| **r != Regime::Unknown || !v.is_empty())
        .map(|(r, v)| {
            let n = v.len();
            RegimeStat {
                regime: *r,
                label: regime_label(*r).into(),
                trades: n,
                win_rate: (n > 0).then(|| v.iter().filter(|x| **x > 0.0).count() as f64 / n as f64 * 100.0),
                avg_return: (n > 0).then(|| v.iter().sum::<f64>() / n as f64),
                low_sample: n < REGIME_MIN_TRADES,
            }
        })
        .collect()
}
