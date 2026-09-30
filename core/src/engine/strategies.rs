//! Strategy comparator (`/api/strategies`): how textbook strategies, with their FIXED published parameters (no
//! optimisation on the tested period), would have behaved on one asset's daily history. Same assumptions as the
//! signal's backtest (`backtest.rs`, `metrics.rs`): long only, decision on the closed candle i, execution at the open
//! of i + 1, fees 0.1 %, slippage 0.05 % and half the default bid/ask spread paid on each side, stop assumed hit
//! first when stop and target fall in the same candle, results split by market regime on the signal candle.
//!
//! Every strategy is tested on the same bars: from the 201st candle (the 200-bar moving average exists) to the last
//! closed one. Indicators are causal (value at i only uses candles ≤ i), and the simulator only asks a strategy
//! about bar i once bar i is closed.
use serde::{Deserialize, Serialize};

use super::backtest::{BacktestTrade, FEE_RATE, RegimeStat, regime_split, trade_stats};
use super::metrics::{SLIPPAGE, SPREAD_CRYPTO, SPREAD_STOCK, ratios};
use super::signal::{Series, ema, rsi, sanitize, sma};
use crate::js::fr;
use crate::types::{Candle, Kind};

/// First tested bar (index): the 200-bar SMA needs 200 closes, the decision taken on bar 199 is executed at bar 200.
pub const START: usize = 200;
/// Fewer closed trades: "échantillon trop faible".
pub const MIN_TRADES: usize = 10;
/// Points of each equity curve sent to the client (at most).
pub const MAX_POINTS: usize = 200;
/// Tested bars needed to say anything.
pub const MIN_TESTED: usize = 60;
const DAY_MS: i64 = 86_400_000;
const YEAR_MS: f64 = 365.25 * 86_400_000.0;

// Fixed parameters (textbook values).
pub const TREND_FAST: usize = 50;
pub const TREND_SLOW: usize = 200;
pub const MOMENTUM_ROC: usize = 90;
pub const BREAKOUT_ENTRY: usize = 55;
pub const BREAKOUT_EXIT: usize = 20;
pub const MR_RSI: usize = 2;
pub const MR_ENTRY: f64 = 10.0;
pub const MR_EXIT: f64 = 70.0;
pub const MR_MAX_BARS: usize = 5;
pub const SWING_EMA: usize = 20;
pub const SWING_TREND: usize = 50;
pub const SWING_PULLBACK_BARS: usize = 3;
pub const SWING_LOW_BARS: usize = 5;
pub const SWING_REWARD_RISK: f64 = 2.0;
pub const VALUE_MIN_DAYS: usize = 250;
pub const VALUE_HISTORY_DAYS: i64 = 5 * 365;
pub const VALUE_HIGH_PERCENTILE: f64 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StrategyId {
    Trend,
    Momentum,
    Breakout,
    MeanReversion,
    Swing,
    Dca,
    BuyHold,
    Value,
}

/// Fixed order (the client gives each one its colour in this order).
pub const ALL: [StrategyId; 8] = [
    StrategyId::Trend,
    StrategyId::Momentum,
    StrategyId::Breakout,
    StrategyId::MeanReversion,
    StrategyId::Swing,
    StrategyId::Dca,
    StrategyId::BuyHold,
    StrategyId::Value,
];

impl StrategyId {
    pub fn name(self) -> &'static str {
        match self {
            StrategyId::Trend => "Suivi de tendance",
            StrategyId::Momentum => "Momentum",
            StrategyId::Breakout => "Cassure (Turtle)",
            StrategyId::MeanReversion => "Retour à la moyenne",
            StrategyId::Swing => "Swing (repli sur EMA 20)",
            StrategyId::Dca => "Achats programmés (DCA)",
            StrategyId::BuyHold => "Achat-conservation",
            StrategyId::Value => "Valeur (PER)",
        }
    }

    /// One French sentence: the exact rule.
    pub fn rule(self) -> &'static str {
        match self {
            StrategyId::Trend => {
                "Achète quand la clôture est au-dessus de la moyenne 200 jours et que la moyenne 50 jours est au-dessus de la 200 jours ; revend dès qu'une clôture passe sous la moyenne 200 jours."
            }
            StrategyId::Momentum => {
                "Le premier jour de cotation de chaque mois, reste investi si le cours a monté sur les 90 dernières séances (ROC 90 > 0), sinon passe en liquidités jusqu'au mois suivant."
            }
            StrategyId::Breakout => {
                "Achète quand la clôture dépasse le plus haut des 55 séances précédentes ; revend quand elle passe sous le plus bas des 20 séances précédentes (règle des Turtles, sur clôture)."
            }
            StrategyId::MeanReversion => {
                "Au-dessus de la moyenne 200 jours, achète quand le RSI 2 jours passe sous 10 ; revend quand il repasse au-dessus de 70 ou au bout de 5 séances (façon Connors)."
            }
            StrategyId::Swing => {
                "En tendance haussière (EMA 20 au-dessus de la moyenne 50 jours, clôture au-dessus de celle-ci), après un repli qui a touché l'EMA 20 dans les 3 dernières séances, achète quand la clôture dépasse le plus haut de la veille ; stop sous le plus bas des 5 dernières séances, objectif à 2 fois le risque."
            }
            StrategyId::Dca => {
                "Investit la même somme le premier jour de cotation de chaque mois, sans jamais vendre ; comparé à tout investir au départ."
            }
            StrategyId::BuyHold => "Achète au début de la période et garde jusqu'au bout : la référence.",
            StrategyId::Value => {
                "Achète quand le PER du jour est sous sa médiane des 5 années précédentes ; revend quand il dépasse son 80e centile sur la même fenêtre."
            }
        }
    }

    pub fn params(self) -> &'static str {
        match self {
            StrategyId::Trend => "SMA 50 / SMA 200, sortie sous SMA 200",
            StrategyId::Momentum => "ROC 90 séances, rééquilibrage mensuel",
            StrategyId::Breakout => "Donchian 55 entrée / Donchian 20 sortie",
            StrategyId::MeanReversion => "RSI(2) < 10 au-dessus de SMA 200 ; sortie RSI(2) > 70 ou 5 séances",
            StrategyId::Swing => "EMA 20, SMA 50, repli ≤ 3 séances, stop plus bas 5 séances, objectif 2 R",
            StrategyId::Dca => "Même montant chaque mois, sans vente",
            StrategyId::BuyHold => "Un achat au départ",
            StrategyId::Value => "PER < médiane 5 ans ; vente > 80e centile 5 ans (au moins 250 jours de PER)",
        }
    }
}

/// What the strategy wants after the close of bar i.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    Hold,
    /// Buy at the next open; `stop` (price) and a target at `reward_risk` × the risk when given.
    Enter {
        stop: Option<f64>,
        reward_risk: Option<f64>,
    },
    /// Sell at the next open.
    Exit,
}

/// Open position seen by a strategy: bars held including the current one, entry price.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
    pub bars: usize,
    pub entry: f64,
}

/// P/E thresholds of one bar: (P/E of the day, median of the previous 5 years, 80th percentile).
type ValueBar = Option<(f64, f64, f64)>;

/// Causal indicators of a candle series.
pub struct Indicators {
    candles: Vec<Candle>,
    sma50: Series,
    sma200: Series,
    ema20: Series,
    rsi2: Series,
    value: Vec<ValueBar>,
}

fn at(s: &Series, i: usize) -> Option<f64> {
    s.get(i).copied().flatten()
}

fn is_month_start(c: &[Candle], i: usize) -> bool {
    use chrono::Datelike;
    let m = |t: i64| chrono::DateTime::from_timestamp_millis(t).map(|d| (d.year(), d.month()));
    i > 0 && m(c[i].time) != m(c[i - 1].time)
}

/// P/E of each bar against the P/E of the 5 years up to that day (the day included): None when the day has no P/E
/// or fewer than 250 days of history. `per`: (ms, P/E) by date.
pub fn value_bars(candles: &[Candle], per: &[(i64, f64)]) -> Vec<ValueBar> {
    let day = |t: i64| t.div_euclid(DAY_MS);
    candles
        .iter()
        .map(|c| {
            let d = day(c.time);
            let (_, current) = per.iter().rev().find(|(t, _)| day(*t) == d)?;
            let mut past: Vec<f64> = per.iter().filter(|(t, _)| day(*t) <= d && d - day(*t) < VALUE_HISTORY_DAYS).map(|x| x.1).collect();
            if past.len() < VALUE_MIN_DAYS {
                return None;
            }
            past.sort_by(f64::total_cmp);
            let n = past.len();
            let median = if n % 2 == 1 { past[n / 2] } else { (past[n / 2 - 1] + past[n / 2]) / 2.0 };
            let high = past[((n - 1) as f64 * VALUE_HIGH_PERCENTILE).round() as usize];
            Some((*current, median, high))
        })
        .collect()
}

impl Indicators {
    /// `candles` sanitized, oldest first; `per` only for the value strategy (stocks).
    pub fn new(candles: &[Candle], per: Option<&[(i64, f64)]>) -> Indicators {
        let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
        Indicators {
            sma50: sma(&closes, TREND_FAST),
            sma200: sma(&closes, TREND_SLOW),
            ema20: ema(&closes, SWING_EMA),
            rsi2: rsi(&closes, MR_RSI),
            value: per.map(|p| value_bars(candles, p)).unwrap_or_else(|| vec![None; candles.len()]),
            candles: candles.to_vec(),
        }
    }

    /// Decision of a signal strategy at the close of bar i (DCA is simulated apart).
    pub fn decide(&self, id: StrategyId, i: usize, held: Option<Held>) -> Decision {
        let c = &self.candles;
        let close = c[i].close;
        match id {
            StrategyId::Trend => {
                let (Some(fast), Some(slow)) = (at(&self.sma50, i), at(&self.sma200, i)) else { return Decision::Hold };
                match held {
                    None if close > slow && fast > slow => Decision::Enter { stop: None, reward_risk: None },
                    Some(_) if close < slow => Decision::Exit,
                    _ => Decision::Hold,
                }
            }
            StrategyId::Momentum => {
                if i < MOMENTUM_ROC || !is_month_start(c, i) {
                    return Decision::Hold;
                }
                let up = close > c[i - MOMENTUM_ROC].close;
                match held {
                    None if up => Decision::Enter { stop: None, reward_risk: None },
                    Some(_) if !up => Decision::Exit,
                    _ => Decision::Hold,
                }
            }
            StrategyId::Breakout => match held {
                None if i >= BREAKOUT_ENTRY => {
                    let high = c[i - BREAKOUT_ENTRY..i].iter().fold(f64::MIN, |a, x| a.max(x.high));
                    if close > high { Decision::Enter { stop: None, reward_risk: None } } else { Decision::Hold }
                }
                Some(_) if i >= BREAKOUT_EXIT => {
                    let low = c[i - BREAKOUT_EXIT..i].iter().fold(f64::MAX, |a, x| a.min(x.low));
                    if close < low { Decision::Exit } else { Decision::Hold }
                }
                _ => Decision::Hold,
            },
            StrategyId::MeanReversion => {
                let Some(r) = at(&self.rsi2, i) else { return Decision::Hold };
                match held {
                    None if at(&self.sma200, i).is_some_and(|s| close > s) && r < MR_ENTRY => Decision::Enter { stop: None, reward_risk: None },
                    Some(h) if r > MR_EXIT || h.bars >= MR_MAX_BARS => Decision::Exit,
                    _ => Decision::Hold,
                }
            }
            StrategyId::Swing => {
                if held.is_some() || i < SWING_LOW_BARS {
                    return Decision::Hold;
                }
                let (Some(e), Some(trend)) = (at(&self.ema20, i), at(&self.sma50, i)) else { return Decision::Hold };
                let uptrend = e > trend && close > trend;
                let touched = c[i + 1 - SWING_PULLBACK_BARS..=i].iter().any(|x| x.low <= e);
                if uptrend && touched && close > c[i - 1].high && close > e {
                    let stop = c[i + 1 - SWING_LOW_BARS..=i].iter().fold(f64::MAX, |a, x| a.min(x.low));
                    Decision::Enter { stop: Some(stop), reward_risk: Some(SWING_REWARD_RISK) }
                } else {
                    Decision::Hold
                }
            }
            StrategyId::BuyHold => {
                if held.is_none() {
                    Decision::Enter { stop: None, reward_risk: None }
                } else {
                    Decision::Hold
                }
            }
            StrategyId::Value => {
                let Some((pe, median, high)) = self.value[i] else { return Decision::Hold };
                match held {
                    None if pe < median => Decision::Enter { stop: None, reward_risk: None },
                    Some(_) if pe > high => Decision::Exit,
                    _ => Decision::Hold,
                }
            }
            StrategyId::Dca => Decision::Hold,
        }
    }
}

/// Result of one simulation, before the metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// (time, equity) at each tested close, 1 = capital at the start.
    pub curve: Vec<(i64, f64)>,
    pub trades: Vec<BacktestTrade>,
    /// Initial risk (entry − stop, % of the entry) per trade, NaN without a stop.
    pub risks: Vec<f64>,
    pub bars_in_market: usize,
}

struct Position {
    index: usize,
    time: i64,
    entry: f64,
    units: f64,
    stop: Option<f64>,
    target: Option<f64>,
    /// Equity just before the buy (the trade's return is measured against it).
    capital: f64,
}

/// Walk-forward simulation of a signal strategy. `side`: slippage + half spread paid on each side (fraction), on
/// top of the fees. The decision of bar i is asked with bar i closed and executed at the open of bar i + 1.
pub fn simulate(candles: &[Candle], side: f64, mut decide: impl FnMut(usize, Option<Held>) -> Decision) -> Run {
    let n = candles.len();
    let mut run = Run { curve: vec![], trades: vec![], risks: vec![], bars_in_market: 0 };
    if n <= START {
        return run;
    }
    let mut equity = 1.0;
    let mut position: Option<Position> = None;
    let mut pending = Decision::Hold;
    let close = |p: Position, price: f64, time: i64, reason: &str, trades: &mut Vec<BacktestTrade>| -> f64 {
        let proceeds = p.units * price * (1.0 - side) * (1.0 - FEE_RATE);
        trades.push(BacktestTrade {
            entry_time: p.time,
            exit_time: time,
            entry_price: p.entry,
            exit_price: price,
            return_percent: (proceeds / p.capital - 1.0) * 100.0,
            exit_reason: reason.into(),
        });
        proceeds
    };
    for (i, &bar) in candles.iter().enumerate().skip(START - 1) {
        if i >= START {
            match pending {
                Decision::Exit => {
                    if let Some(p) = position.take() {
                        equity = close(p, bar.open, bar.time, "Signal de sortie", &mut run.trades);
                    }
                }
                Decision::Enter { stop, reward_risk } if position.is_none() => {
                    let entry = bar.open;
                    // An open already under the stop: no trade (the setup is broken before it starts).
                    if stop.is_none_or(|s| entry > s) {
                        run.risks.push(stop.map_or(f64::NAN, |s| (entry - s) / entry * 100.0));
                        position = Some(Position {
                            index: i,
                            time: bar.time,
                            entry,
                            units: equity * (1.0 - FEE_RATE) / (entry * (1.0 + side)),
                            stop,
                            target: stop.zip(reward_risk).map(|(s, r)| entry + (entry - s) * r),
                            capital: equity,
                        });
                    }
                }
                _ => {}
            }
            pending = Decision::Hold;
            if let Some(p) = position.as_ref() {
                run.bars_in_market += 1;
                let (stop, target) = (p.stop, p.target);
                if let Some(s) = stop.filter(|s| bar.low <= *s) {
                    equity = close(position.take().unwrap(), bar.open.min(s), bar.time, "Stop", &mut run.trades);
                } else if let Some(t) = target.filter(|t| bar.high >= *t) {
                    equity = close(position.take().unwrap(), bar.open.max(t), bar.time, "Objectif", &mut run.trades);
                }
            }
            run.curve.push((bar.time, position.as_ref().map_or(equity, |p| p.units * bar.close)));
        }
        if i + 1 < n {
            pending = decide(i, position.as_ref().map(|p| Held { bars: i + 1 - p.index, entry: p.entry }));
        }
    }
    if let Some(p) = position.take() {
        let last = candles[n - 1];
        equity = close(p, last.close, last.time, "Fin du test", &mut run.trades);
        if let Some(l) = run.curve.last_mut() {
            l.1 = equity;
        }
    }
    run
}

/// DCA: the same amount (1) invested at the open of the first tested bar, then at the open of the first bar of
/// each month, never sold. Curve: value ÷ amount invested so far, the last point net of the selling costs. The
/// purchases are returned as trades running to the end (for the split by regime).
pub fn simulate_dca(candles: &[Candle], side: f64) -> (Run, Vec<(i64, f64)>, f64) {
    let n = candles.len();
    let mut run = Run { curve: vec![], trades: vec![], risks: vec![], bars_in_market: 0 };
    let mut flows: Vec<(i64, f64)> = vec![];
    if n <= START {
        return (run, flows, 0.0);
    }
    let mut units = 0.0;
    let mut buys: Vec<(i64, f64, f64)> = vec![]; // (time, price, units)
    for i in START..n {
        let bar = candles[i];
        if i == START || is_month_start(candles, i) {
            let u = (1.0 - FEE_RATE) / (bar.open * (1.0 + side));
            units += u;
            buys.push((bar.time, bar.open, u));
            flows.push((bar.time, 1.0));
        }
        run.bars_in_market += 1;
        run.curve.push((bar.time, units * bar.close / flows.len() as f64));
    }
    let last = candles[n - 1];
    let net = (1.0 - side) * (1.0 - FEE_RATE);
    let value = units * last.close * net;
    if let Some(l) = run.curve.last_mut() {
        l.1 = value / flows.len() as f64;
    }
    for (t, price, u) in buys {
        run.trades.push(BacktestTrade {
            entry_time: t,
            exit_time: last.time,
            entry_price: price,
            exit_price: last.close,
            return_percent: (u * last.close * net - 1.0) * 100.0,
            exit_reason: "Fin du test".into(),
        });
        run.risks.push(f64::NAN);
    }
    (run, flows, value)
}

/// Annual money-weighted return (internal rate of return) of contributions `flows` (time, amount) worth `value` at
/// `end`, by bisection; None when it cannot be bracketed.
pub fn irr(flows: &[(i64, f64)], value: f64, end: i64) -> Option<f64> {
    let t0 = flows.first()?.0;
    let npv = |r: f64| {
        let d = |t: i64| (1.0 + r).powf(-((t - t0) as f64 / YEAR_MS));
        value * d(end) - flows.iter().map(|(t, a)| a * d(*t)).sum::<f64>()
    };
    let (mut lo, mut hi) = (-0.99, 100.0);
    if npv(lo) < 0.0 || npv(hi) > 0.0 {
        return None;
    }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        if npv(mid) > 0.0 { lo = mid } else { hi = mid }
    }
    Some((lo + hi) / 2.0)
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyMetrics {
    /// % over the period, costs included (DCA: gain ÷ total invested).
    pub total_return: f64,
    /// % per year; None under one year. DCA: annual money-weighted return (IRR).
    pub cagr: Option<f64>,
    /// % (negative), worst fall from a peak of the equity curve.
    pub max_drawdown: f64,
    /// Annualised, daily returns of the equity curve; None for DCA (its time-weighted return is the asset's).
    pub sharpe: Option<f64>,
    pub sortino: Option<f64>,
    /// % of winning trades; None without closed trades (or DCA).
    pub win_rate: Option<f64>,
    /// Gross gains ÷ gross losses; None without a loss.
    pub profit_factor: Option<f64>,
    /// % per trade: average win × win rate − |average loss| × loss rate.
    pub expectancy: Option<f64>,
    /// Average R multiple (swing only: the other strategies have no stop).
    pub avg_r: Option<f64>,
    /// Closed trades (DCA: purchases).
    pub trades: usize,
    /// % of the tested bars spent invested.
    pub exposure: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyResult {
    pub id: StrategyId,
    pub name: String,
    /// The rule in one French sentence.
    pub rule: String,
    /// Fixed parameters.
    pub params: String,
    /// false: not applicable or not covered (`unavailable` says why).
    pub available: bool,
    pub unavailable: Option<String>,
    pub metrics: Option<StrategyMetrics>,
    /// By market regime on the signal candle (bull, bear, range, crisis, unknown when it has trades).
    pub regimes: Vec<RegimeStat>,
    /// [ms, value of 100 invested], at most 200 points, the same dates for every strategy.
    pub equity: Vec<(i64, f64)>,
    /// Fewer than 10 closed trades (signal strategies only).
    pub low_sample: bool,
    /// Extra sentence (DCA against a lump sum, source of the P/E…).
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategiesReport {
    pub symbol: String,
    pub kind: Kind,
    pub as_of: i64,
    /// First and last tested bars (ms), number of tested daily bars.
    pub from: i64,
    pub to: i64,
    pub bars: usize,
    /// "janv. 2024 – sept. 2026 (680 bougies journalières)".
    pub period: String,
    /// Where the candles come from.
    pub source: String,
    pub fees_pct: f64,
    pub slippage_pct: f64,
    /// Full bid/ask spread assumed (%), half paid on each side.
    pub spread_pct: f64,
    /// How the comparison avoids flattering itself (French sentences).
    pub notes: Vec<String>,
    /// In the fixed order of `ALL`.
    pub strategies: Vec<StrategyResult>,
}

fn round_to(x: f64, d: i32) -> f64 {
    let p = 10f64.powi(d);
    (x * p).round() / p
}

const MONTHS: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];

fn month_year(ms: i64) -> String {
    use chrono::Datelike;
    match chrono::DateTime::from_timestamp_millis(ms) {
        Some(d) => format!("{} {}", MONTHS[d.month0() as usize], d.year()),
        None => "?".into(),
    }
}

fn pct_fr(x: f64) -> String {
    format!("{}{}\u{202f}%", if x < 0.0 { "−" } else { "+" }, fr(x.abs(), 0, 1))
}

/// Indices kept for a curve of `len` points: all of them up to 200, else 200 evenly spaced, first and last kept.
pub fn sample_indices(len: usize) -> Vec<usize> {
    if len <= MAX_POINTS {
        return (0..len).collect();
    }
    (0..MAX_POINTS).map(|k| k * (len - 1) / (MAX_POINTS - 1)).collect()
}

fn max_drawdown(values: &[f64]) -> f64 {
    let mut peak = 0.0f64;
    let mut dd = 0.0f64;
    for v in values {
        peak = peak.max(*v);
        if peak > 0.0 {
            dd = dd.max((peak - v) / peak);
        }
    }
    dd
}

/// Metrics of a signal strategy's run (1 = starting capital).
pub fn metrics(run: &Run, tested: usize, kind: Kind) -> StrategyMetrics {
    let values: Vec<f64> = std::iter::once(1.0).chain(run.curve.iter().map(|p| p.1)).collect();
    let last = *values.last().unwrap_or(&1.0);
    let years = match (run.curve.first(), run.curve.last()) {
        (Some(a), Some(b)) => (b.0 - a.0) as f64 / YEAR_MS,
        _ => 0.0,
    };
    let returns: Vec<f64> = run.trades.iter().map(|t| t.return_percent).collect();
    let wins: f64 = returns.iter().filter(|x| **x > 0.0).sum();
    let losses: f64 = -returns.iter().filter(|x| **x <= 0.0).sum::<f64>();
    let stats = trade_stats(&returns, &run.risks);
    let (sharpe, sortino) = ratios(&values, if kind == Kind::Crypto { 365.0 } else { 252.0 });
    let n = returns.len();
    StrategyMetrics {
        total_return: round_to((last - 1.0) * 100.0, 2),
        cagr: (years >= 1.0 && last > 0.0).then(|| round_to((last.powf(1.0 / years) - 1.0) * 100.0, 2)),
        max_drawdown: round_to(-max_drawdown(&values) * 100.0, 2),
        sharpe,
        sortino,
        win_rate: (n > 0).then(|| round_to(returns.iter().filter(|x| **x > 0.0).count() as f64 / n as f64 * 100.0, 1)),
        profit_factor: (losses > 0.0).then(|| round_to(wins / losses, 2)),
        expectancy: stats.expectancy.map(|x| round_to(x, 2)),
        avg_r: stats.avg_r.map(|x| round_to(x, 2)),
        trades: n,
        exposure: if tested > 0 { round_to(run.bars_in_market as f64 / tested as f64 * 100.0, 1) } else { 0.0 },
    }
}

fn rounded_regimes(candles: &[Candle], run: &Run) -> Vec<RegimeStat> {
    let returns: Vec<f64> = run.trades.iter().map(|t| t.return_percent).collect();
    regime_split(candles, &run.trades, &returns)
        .into_iter()
        .map(|g| RegimeStat { win_rate: g.win_rate.map(|x| round_to(x, 1)), avg_return: g.avg_return.map(|x| round_to(x, 2)), ..g })
        .collect()
}

fn sampled(run: &Run, idx: &[usize]) -> Vec<(i64, f64)> {
    idx.iter().filter_map(|i| run.curve.get(*i)).map(|(t, v)| (*t, round_to(v * 100.0, 2))).collect()
}

/// P/E input of the value strategy: the daily series with its source, or why it is missing.
pub enum PerInput<'a> {
    NotApplicable,
    Missing(String),
    Series(&'a [(i64, f64)], String),
}

/// The whole comparison on daily candles (any order, invalid ones dropped). None under 260 candles.
pub fn compare(symbol: &str, kind: Kind, raw: &[Candle], source: &str, per: PerInput, as_of: i64) -> Option<StrategiesReport> {
    let candles = sanitize(raw);
    if candles.len() < START + MIN_TESTED {
        return None;
    }
    let spread = if kind == Kind::Crypto { SPREAD_CRYPTO } else { SPREAD_STOCK };
    let side = SLIPPAGE + spread / 100.0 / 2.0;
    let per_series = match &per {
        PerInput::Series(s, _) => Some(*s),
        _ => None,
    };
    let ind = Indicators::new(&candles, per_series);
    let tested = candles.len() - START;
    let idx = sample_indices(tested);
    let buy_hold = simulate(&candles, side, |i, h| ind.decide(StrategyId::BuyHold, i, h));
    let buy_hold_return = (buy_hold.curve.last().map_or(1.0, |p| p.1) - 1.0) * 100.0;

    let strategies = ALL
        .iter()
        .map(|&id| {
            let base = StrategyResult {
                id,
                name: id.name().into(),
                rule: id.rule().into(),
                params: id.params().into(),
                available: true,
                unavailable: None,
                metrics: None,
                regimes: vec![],
                equity: vec![],
                low_sample: false,
                note: None,
            };
            match id {
                StrategyId::Dca => {
                    let (run, flows, value) = simulate_dca(&candles, side);
                    let mut m = metrics(&run, tested, kind);
                    let invested = flows.len() as f64;
                    let end = candles[candles.len() - 1].time;
                    let years = (end - candles[START].time) as f64 / YEAR_MS;
                    m.total_return = round_to((value / invested - 1.0) * 100.0, 2);
                    m.cagr = if years >= 1.0 { irr(&flows, value, end).map(|r| round_to(r * 100.0, 2)) } else { None };
                    m.sharpe = None;
                    m.sortino = None;
                    m.win_rate = None;
                    m.profit_factor = None;
                    m.expectancy = None;
                    m.avg_r = None;
                    let note = format!(
                        "{} versements mensuels : {} sur les sommes versées ; tout investir au départ (achat-conservation) : {}.{}",
                        flows.len(),
                        pct_fr(m.total_return),
                        pct_fr(buy_hold_return),
                        " Rendement annuel : taux de rendement interne (argent investi progressivement) ; Sharpe et Sortino non pertinents ici."
                    );
                    StrategyResult { metrics: Some(m), regimes: rounded_regimes(&candles, &run), equity: sampled(&run, &idx), note: Some(note), ..base }
                }
                StrategyId::Value if !matches!(per, PerInput::Series(..)) => {
                    let why = match &per {
                        PerInput::Missing(e) => format!("Non couvert : {e}."),
                        _ => "Non applicable : une crypto ne dégage pas de bénéfices, donc pas de PER.".into(),
                    };
                    StrategyResult { available: false, unavailable: Some(why), ..base }
                }
                _ => {
                    let run = if id == StrategyId::BuyHold { buy_hold.clone() } else { simulate(&candles, side, |i, h| ind.decide(id, i, h)) };
                    let m = metrics(&run, tested, kind);
                    let signal = id != StrategyId::BuyHold;
                    let mut note = None;
                    if let (StrategyId::Value, PerInput::Series(_, src)) = (id, &per) {
                        let days = ind.value[START..].iter().filter(|v| v.is_some()).count();
                        note = Some(format!(
                            "PER : {src} ; chiffres retenus 45 jours après la fin de chaque trimestre. Seuils calculables {days} jours sur {tested} (il faut 250 jours de PER passés)."
                        ));
                    }
                    StrategyResult {
                        low_sample: signal && m.trades < MIN_TRADES,
                        // One purchase at the start: a split by regime would say nothing.
                        regimes: if signal { rounded_regimes(&candles, &run) } else { vec![] },
                        equity: sampled(&run, &idx),
                        metrics: Some(m),
                        note,
                        ..base
                    }
                }
            }
        })
        .collect();

    let from = candles[START].time;
    let to = candles[candles.len() - 1].time;
    let notes = vec![
        "Paramètres fixes, ceux des manuels : aucune optimisation sur la période testée.".to_string(),
        "Chaque décision ne voit que les bougies journalières déjà clôturées ; l'ordre est exécuté à l'ouverture suivante.".to_string(),
        format!(
            "Coûts à chaque achat et vente : frais {} %, glissement {} %, demi-écart achat/vente {} % (hypothèse).",
            fr(FEE_RATE * 100.0, 0, 3),
            fr(SLIPPAGE * 100.0, 0, 3),
            fr(spread / 2.0, 0, 3)
        ),
        format!("Même période pour toutes : à partir du jour où la moyenne 200 jours existe ({tested} séances testées)."),
        "Une position encore ouverte est vendue à la dernière clôture («\u{a0}Fin du test\u{a0}»).".to_string(),
        "Régime de marché lu sur la bougie du signal, avec les seules données connues ce jour-là.".to_string(),
        "Moins de 10 trades : échantillon trop faible pour conclure. Le passé ne garantit pas l'avenir.".to_string(),
    ];
    Some(StrategiesReport {
        symbol: symbol.into(),
        kind,
        as_of,
        from,
        to,
        bars: tested,
        period: format!("{} – {} ({tested} bougies journalières)", month_year(from), month_year(to)),
        source: source.into(),
        fees_pct: round_to(FEE_RATE * 100.0, 4),
        slippage_pct: round_to(SLIPPAGE * 100.0, 4),
        spread_pct: spread,
        notes,
        strategies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_600_000_000_000; // 13 sept. 2020

    /// Daily candles from closes: open = previous close, high/low 0.5 % around.
    fn series(closes: &[f64]) -> Vec<Candle> {
        closes
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let open = if i == 0 { *c } else { closes[i - 1] };
                Candle { time: T0 + i as i64 * DAY_MS, open, high: open.max(*c) * 1.005, low: open.min(*c) * 0.995, close: *c, volume: 1.0 }
            })
            .collect()
    }

    fn decisions(candles: &[Candle], id: StrategyId, per: Option<&[(i64, f64)]>) -> Run {
        let ind = Indicators::new(candles, per);
        simulate(candles, 0.0, |i, h| ind.decide(id, i, h))
    }

    fn entry_index(c: &[Candle], t: &BacktestTrade) -> usize {
        c.iter().position(|x| x.time == t.entry_time).unwrap()
    }
    fn exit_index(c: &[Candle], t: &BacktestTrade) -> usize {
        c.iter().position(|x| x.time == t.exit_time).unwrap()
    }

    /// Past decisions do not depend on future candles: the trades that ended before the cut are identical when
    /// the candles after the cut are replaced by something else.
    fn no_look_ahead(c: &[Candle], id: StrategyId, per: Option<&[(i64, f64)]>) {
        let full = decisions(c, id, per);
        for cut in [START + 20, START + 60, c.len() - 10].into_iter().filter(|k| *k < c.len()) {
            let mut other = c.to_vec();
            for (k, x) in other.iter_mut().enumerate().skip(cut) {
                let f = if k % 2 == 0 { 0.7 } else { 1.4 };
                *x = Candle { open: x.open * f, high: x.high * f * 1.1, low: x.low * f * 0.9, close: x.close * f, ..*x };
            }
            let changed = decisions(&other, id, per);
            let before = |r: &Run| r.trades.iter().filter(|t| exit_index(c, t) < cut).cloned().collect::<Vec<_>>();
            assert_eq!(before(&full), before(&changed), "{id:?} coupe {cut}");
            assert_eq!(full.curve[..cut - START], changed.curve[..cut - START], "{id:?} courbe coupe {cut}");
        }
    }

    /// 200 flat bars at 100, then the given closes.
    fn after_flat(tail: &[f64]) -> Vec<Candle> {
        let mut v = vec![100.0; START];
        v.extend_from_slice(tail);
        series(&v)
    }

    #[test]
    fn trend_enters_and_exits_on_the_200_day_average() {
        // Rise from bar 200 then a fall under the 200-day average.
        let mut tail: Vec<f64> = (1..=60).map(|k| 100.0 + k as f64).collect();
        tail.extend((1..=60).map(|k| 160.0 - k as f64 * 2.0));
        let c = after_flat(&tail);
        let r = decisions(&c, StrategyId::Trend, None);
        let t = &r.trades[0];
        // Bar 200: close 101 > SMA 200 (100.005) and SMA 50 (100.02) > SMA 200 → bought at the open of bar 201.
        assert_eq!(entry_index(&c, t), 201);
        let first_below = (START..c.len()).find(|&i| {
            let s: f64 = c[i + 1 - 200..=i].iter().map(|x| x.close).sum::<f64>() / 200.0;
            i > 201 && c[i].close < s
        });
        assert_eq!(exit_index(&c, t), first_below.unwrap() + 1);
        assert_eq!(t.exit_reason, "Signal de sortie");
        assert_eq!(t.exit_price, c[first_below.unwrap() + 1].open, "exécuté à l'ouverture suivante");
        no_look_ahead(&c, StrategyId::Trend, None);
    }

    #[test]
    fn momentum_rebalances_on_the_first_bar_of_the_month_only() {
        let tail: Vec<f64> = (1..=150).map(|k| 100.0 + k as f64 * 0.5).collect();
        let c = after_flat(&tail);
        let r = decisions(&c, StrategyId::Momentum, None);
        let t = &r.trades[0];
        let e = entry_index(&c, t);
        assert!(is_month_start(&c, e - 1), "décision le premier jour du mois");
        assert!(!(START - 1..e - 1).any(|i| is_month_start(&c, i) && c[i].close > c[i - 90].close), "pas de mois plus tôt");
        no_look_ahead(&c, StrategyId::Momentum, None);
    }

    #[test]
    fn breakout_uses_55_and_20_bar_channels() {
        // Flat, a new 55-bar high at bar 210, then a drop under the 20-bar low at bar 240.
        let mut tail = vec![100.0; 10];
        tail.extend([110.0; 29]);
        tail.push(90.0);
        tail.extend([90.0; 30]);
        let c = after_flat(&tail);
        let r = decisions(&c, StrategyId::Breakout, None);
        let t = &r.trades[0];
        assert_eq!(entry_index(&c, t), 211);
        assert_eq!(exit_index(&c, t), 240);
        no_look_ahead(&c, StrategyId::Breakout, None);
    }

    #[test]
    fn mean_reversion_exits_on_rsi_or_after_five_bars() {
        // Slow rise (above the 200-day average), two sharp down days (RSI 2 < 10), then flat: exit after 5 bars.
        let mut closes: Vec<f64> = (0..230).map(|k| 100.0 + k as f64 * 0.2).collect();
        let top = *closes.last().unwrap();
        closes.extend([top - 0.5, top - 3.5]);
        closes.extend(vec![top - 3.5; 20]);
        let c = series(&closes);
        let r = decisions(&c, StrategyId::MeanReversion, None);
        let t = &r.trades[0];
        assert_eq!(entry_index(&c, t), 232, "RSI 2 sous 10 à la clôture de la barre 231");
        assert_eq!(exit_index(&c, t), 237, "5 séances tenues");
        // With a rebound instead, the RSI exit comes first.
        let mut bounce = closes[..232].to_vec();
        bounce.push(top + 2.0);
        bounce.extend(vec![top + 2.0; 20]);
        let c2 = series(&bounce);
        let t2 = &decisions(&c2, StrategyId::MeanReversion, None).trades[0];
        assert_eq!(exit_index(&c2, t2), 233);
        no_look_ahead(&c, StrategyId::MeanReversion, None);
    }

    #[test]
    fn swing_stop_and_two_r_target() {
        // Uptrend, pull-back to the EMA 20, close above the previous high → entry; then a rise to 2 R.
        let mut closes: Vec<f64> = (0..220).map(|k| 100.0 + k as f64 * 0.5).collect();
        let top = *closes.last().unwrap();
        closes.extend([top - 4.0, top - 6.0, top - 1.0]);
        closes.extend((1..=30).map(|k| top - 1.0 + k as f64 * 2.0));
        let c = series(&closes);
        let ind = Indicators::new(&c, None);
        let r = simulate(&c, 0.0, |i, h| ind.decide(StrategyId::Swing, i, h));
        let t = r.trades.iter().find(|t| entry_index(&c, t) == 223).expect("entrée après la barre 222");
        let stop = c[218..=222].iter().fold(f64::MAX, |a, x| a.min(x.low));
        let target = t.entry_price + (t.entry_price - stop) * 2.0;
        assert_eq!(t.exit_reason, "Objectif");
        assert_eq!(t.exit_price, c[exit_index(&c, t)].open.max(target));
        assert!((r.risks[0] - (t.entry_price - stop) / t.entry_price * 100.0).abs() < 1e-9);
        // Same setup followed by a collapse: stopped out at the stop (or the open when it gaps under).
        let mut down = closes[..223].to_vec();
        down.extend((1..=10).map(|k| top - 1.0 - k as f64 * 3.0));
        let c2 = series(&down);
        let t2 = decisions(&c2, StrategyId::Swing, None).trades.into_iter().find(|t| entry_index(&c2, t) == 223).unwrap();
        assert_eq!(t2.exit_reason, "Stop");
        assert_eq!(t2.exit_price, c2[exit_index(&c2, &t2)].open.min(stop));
        no_look_ahead(&c, StrategyId::Swing, None);
    }

    #[test]
    fn buy_and_hold_and_dca() {
        let tail: Vec<f64> = (1..=100).map(|k| 100.0 + k as f64).collect();
        let c = after_flat(&tail);
        let r = decisions(&c, StrategyId::BuyHold, None);
        assert_eq!(r.trades.len(), 1);
        assert_eq!(entry_index(&c, &r.trades[0]), START);
        assert!((r.trades[0].return_percent - ((200.0 / 100.0) * (1.0 - FEE_RATE).powi(2) - 1.0) * 100.0).abs() < 1e-9);
        let (dca, flows, value) = simulate_dca(&c, 0.0);
        assert_eq!(flows[0].0, c[START].time);
        assert_eq!(flows.len(), 1 + (START + 1..c.len()).filter(|&i| is_month_start(&c, i)).count());
        assert_eq!(dca.curve.len(), c.len() - START);
        assert!(value / flows.len() as f64 > 1.0);
        // Monthly contributions in a rising market earn less than the lump sum.
        assert!(value / (flows.len() as f64) - 1.0 < r.trades[0].return_percent / 100.0);
    }

    #[test]
    fn value_uses_its_own_five_year_history() {
        // P/E published each day for 300 days before the candles and during them: first 100 at 20, then 30 on
        // days ≥ 250 of the candles, 10 on days 230–249.
        let c = after_flat(&vec![100.0; 100]);
        let mut per: Vec<(i64, f64)> = (1..=300).rev().map(|k| (T0 - k * DAY_MS, 20.0)).collect();
        for (i, x) in c.iter().enumerate() {
            per.push((
                x.time,
                if (230..250).contains(&i) {
                    10.0
                } else if i >= 250 {
                    30.0
                } else {
                    20.0
                },
            ));
        }
        let r = decisions(&c, StrategyId::Value, Some(&per));
        let t = &r.trades[0];
        assert_eq!(entry_index(&c, t), 231, "PER 10 < médiane 20 à la barre 230");
        assert_eq!(exit_index(&c, t), 251, "PER 30 > 80e centile à la barre 250");
        no_look_ahead(&c, StrategyId::Value, Some(&per));
        let bars = value_bars(&c[..1], &per[..10]);
        assert_eq!(bars, vec![None], "moins de 250 jours de PER");
    }

    #[test]
    fn metrics_by_hand() {
        // Two trades +10 % and −5 %: win rate 50 %, profit factor 2, expectancy 2.5 %.
        let t =
            |r: f64| BacktestTrade { entry_time: 0, exit_time: 0, entry_price: 1.0, exit_price: 1.0, return_percent: r, exit_reason: String::new() };
        let curve: Vec<(i64, f64)> = [1.1, 1.21, 1.0, 1.045].iter().enumerate().map(|(i, v)| (T0 + i as i64 * DAY_MS, *v)).collect();
        let run = Run { curve, trades: vec![t(10.0), t(-5.0)], risks: vec![f64::NAN, f64::NAN], bars_in_market: 2 };
        let m = metrics(&run, 4, Kind::Stock);
        assert_eq!(m.win_rate, Some(50.0));
        assert_eq!(m.profit_factor, Some(2.0));
        assert_eq!(m.expectancy, Some(2.5));
        assert_eq!(m.avg_r, None);
        assert_eq!(m.total_return, 4.5);
        assert_eq!(m.max_drawdown, round_to(-(1.0 - 1.0 / 1.21) * 100.0, 2));
        assert_eq!(m.exposure, 50.0);
        assert_eq!(m.cagr, None, "moins d'un an");
        assert_eq!(m.sharpe, None, "moins de 30 rendements");
        // IRR: 1 invested a year before the end, worth 1.1 → 10 % a year.
        let r = irr(&[(T0, 1.0)], 1.1, T0 + YEAR_MS as i64).unwrap();
        assert!((r - 0.1).abs() < 1e-6);
        assert_eq!(sample_indices(1000).len(), MAX_POINTS);
        assert_eq!(*sample_indices(1000).last().unwrap(), 999);
        assert_eq!(sample_indices(50).len(), 50);
    }

    #[test]
    fn report_shape() {
        let closes: Vec<f64> = (0..700).map(|k| 100.0 + (k as f64 / 15.0).sin() * 10.0 + k as f64 * 0.05).collect();
        let c = series(&closes);
        let r = compare("BTC", Kind::Crypto, &c, "test", PerInput::NotApplicable, 0).unwrap();
        assert_eq!(r.strategies.len(), ALL.len());
        assert_eq!(r.bars, 500);
        let value = r.strategies.iter().find(|s| s.id == StrategyId::Value).unwrap();
        assert!(!value.available && value.unavailable.as_deref().unwrap().contains("pas de bénéfices"));
        for s in r.strategies.iter().filter(|s| s.available) {
            assert!(s.equity.len() == MAX_POINTS, "{:?}", s.id);
            assert_eq!(s.equity[0].0, r.from);
            assert_eq!(s.equity.last().unwrap().0, r.to);
            assert_eq!(s.regimes.iter().any(|g| g.regime == super::super::backtest::Regime::Bull), s.id != StrategyId::BuyHold, "{:?}", s.id);
        }
        let dca = r.strategies.iter().find(|s| s.id == StrategyId::Dca).unwrap();
        assert!(dca.note.as_deref().unwrap().contains("tout investir au départ"));
        assert!(compare("BTC", Kind::Crypto, &c[..250], "test", PerInput::NotApplicable, 0).is_none());
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["strategies"][3]["id"], "meanReversion");
        assert!(json["strategies"][0]["equity"][0].is_array());
    }
}
