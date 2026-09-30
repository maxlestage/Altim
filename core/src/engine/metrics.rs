//! Track record of the signal on one asset for `/api/decision`: the backtest of `backtest.rs` (unchanged, walk
//! forward: the signal is computed on the closed candle i and executed at the open of i + 1), with a slippage
//! assumption added on top of its fees, a spread cost (half the bid/ask spread paid on each side: measured when the
//! decision has it, else a stated default), then the usual trade and risk statistics, the expectancy, the R multiples
//! and the results by market regime.
use serde::{Deserialize, Serialize};

use super::backtest::{BacktestResult, FEE_RATE, LOOKBACK, REWARD_RISK, RegimeStat, WARMUP, backtest_with_risk, regime_split, trade_stats};
use super::decision_types::Track;
use crate::js::fr;
use crate::types::{Candle, Kind};

/// Slippage assumed on each side of a trade, fraction (0.05 %).
pub const SLIPPAGE: f64 = 0.0005;
/// Bid/ask spread assumed without a measure (%, full spread): large cryptos 0.02 %, large-cap stocks 0.01 %.
pub const SPREAD_CRYPTO: f64 = 0.02;
pub const SPREAD_STOCK: f64 = 0.01;

/// Extra fields of the track record (flattened into `Track`; absent from older answers: defaults).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrackDetails {
    /// Full bid/ask spread used (%): half of it is paid on the buy, half on the sell.
    pub spread_pct: f64,
    /// true: measured on the order book at the time of the decision; false: default assumption.
    pub spread_measured: bool,
    pub spread_note: String,
    /// Average win × win rate − |average loss| × loss rate, % per trade, costs included.
    pub expectancy: Option<f64>,
    /// Average of (trade return ÷ initial risk to the stop).
    pub avg_r: Option<f64>,
    /// Results by market regime on the signal candle (bull, bear, range, crisis, and unknown when it has trades).
    pub regimes: Vec<RegimeStat>,
    /// Daily candles tested (after the warm-up).
    pub tested_bars: usize,
    /// How the test avoids flattering itself (French sentences).
    pub bias_notes: Vec<String>,
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

/// "0,005 %" (three decimals at most).
fn pct3(x: f64) -> String {
    format!("{} %", fr(x, 0, 3))
}

fn pct_fr(x: f64) -> String {
    format!("{}{} %", if x < 0.0 { "−" } else { "" }, fr(x.abs(), 0, 1))
}

/// Trade return (%) once a cost `side` (fraction) is paid on the buy and on the sell.
pub fn with_costs(return_percent: f64, side: f64) -> f64 {
    ((1.0 + return_percent / 100.0) * (1.0 - side) / (1.0 + side) - 1.0) * 100.0
}

/// Equity curve with the costs: every entry costs 1/(1 + s), every exit (1 − s), from the bar where it happens.
fn adjusted_curve(r: &BacktestResult, side: f64) -> Vec<f64> {
    r.equity
        .iter()
        .map(|p| {
            let entries = r.trades.iter().filter(|t| t.entry_time <= p.time).count() as i32;
            let exits = r.trades.iter().filter(|t| t.exit_time <= p.time).count() as i32;
            p.equity * (1.0 + side).powi(-entries) * (1.0 - side).powi(exits)
        })
        .collect()
}

/// Mean ÷ deviation of the daily returns, annualised; None with fewer than 30 returns or no variation.
pub fn ratios(curve: &[f64], per_year: f64) -> (Option<f64>, Option<f64>) {
    let r: Vec<f64> = curve.windows(2).filter(|w| w[0] > 0.0).map(|w| w[1] / w[0] - 1.0).collect();
    if r.len() < 30 {
        return (None, None);
    }
    let n = r.len() as f64;
    let mean = r.iter().sum::<f64>() / n;
    let sd = (r.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    let down = (r.iter().map(|x| x.min(0.0).powi(2)).sum::<f64>() / n).sqrt();
    let k = per_year.sqrt();
    let sharpe = (sd > 1e-12).then(|| round_to(mean / sd * k, 2));
    let sortino = (down > 1e-12).then(|| round_to(mean / down * k, 2));
    (sharpe, sortino)
}

/// Track record on daily candles (oldest first), with the default spread of the asset class. None when there are
/// too few candles for the backtest (it needs its 60-candle warm-up plus at least one candle to test).
pub fn track(candles_daily: &[Candle], kind: Kind) -> Option<Track> {
    track_with_spread(candles_daily, kind, None)
}

/// Same, with the spread measured at the time of the decision (%, full spread) when there is one.
pub fn track_with_spread(candles_daily: &[Candle], kind: Kind, measured_spread: Option<f64>) -> Option<Track> {
    track_run(candles_daily, kind, measured_spread).map(|(t, _)| t)
}

/// What the track record is computed from, for the cross-asset validation (`validation.rs`): the raw backtest, the
/// trade returns net of every cost (same order as the trades), their initial risks, and the equity curve with the
/// costs (one point per tested candle, same times as `result.equity`).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRun {
    pub result: BacktestResult,
    pub net_returns: Vec<f64>,
    pub risks: Vec<f64>,
    pub curve: Vec<f64>,
}

/// `track_with_spread` plus the run behind it (same backtest, same costs: one computation for both).
pub fn track_run(candles_daily: &[Candle], kind: Kind, measured_spread: Option<f64>) -> Option<(Track, TrackRun)> {
    let (r, risks) = backtest_with_risk(candles_daily, FEE_RATE, LOOKBACK, REWARD_RISK, WARMUP);
    if r.equity.is_empty() {
        return None;
    }
    let measured = measured_spread.filter(|s| s.is_finite() && *s >= 0.0);
    let spread = measured.unwrap_or(if kind == Kind::Crypto { SPREAD_CRYPTO } else { SPREAD_STOCK });
    // Cost paid on each side: slippage + half the spread.
    let side = SLIPPAGE + spread / 100.0 / 2.0;
    let returns: Vec<f64> = r.trades.iter().map(|t| with_costs(t.return_percent, side)).collect();
    let wins: Vec<f64> = returns.iter().copied().filter(|x| *x > 0.0).collect();
    let losses: Vec<f64> = returns.iter().copied().filter(|x| *x <= 0.0).collect();
    let mean = |v: &[f64]| (!v.is_empty()).then(|| round_to(v.iter().sum::<f64>() / v.len() as f64, 2));
    let gross_loss: f64 = -losses.iter().sum::<f64>();
    let profit_factor = (!losses.is_empty() && gross_loss > 0.0).then(|| round_to(wins.iter().sum::<f64>() / gross_loss, 2));
    let mut streak = 0usize;
    let mut losing_streak = 0usize;
    for x in &returns {
        streak = if *x <= 0.0 { streak + 1 } else { 0 };
        losing_streak = losing_streak.max(streak);
    }
    let curve = adjusted_curve(&r, side);
    let (sharpe, sortino) = ratios(&curve, if kind == Kind::Crypto { 365.0 } else { 252.0 });
    let mut peak = 0.0f64;
    let mut max_dd = 0.0f64;
    for e in &curve {
        peak = peak.max(*e);
        if peak > 0.0 {
            max_dd = max_dd.max((peak - e) / peak);
        }
    }
    let total_return = (curve.last().copied().unwrap_or(1.0) - 1.0) * 100.0;
    let buy_and_hold = r.buy_and_hold_percent;
    let n = returns.len();

    let mut note =
        vec!["Chaque décision ne voit que les bougies déjà clôturées : signal calculé à la clôture, exécuté à l'ouverture suivante.".to_string()];
    let spread_note = if measured.is_some() {
        format!("Écart achat/vente mesuré sur le carnet d'ordres : {} (moitié payée à l'achat, moitié à la vente).", pct3(spread))
    } else {
        format!(
            "Hypothèse : écart achat/vente de {} ({}), faute de mesure ; moitié payée à l'achat, moitié à la vente.",
            pct3(spread),
            if kind == Kind::Crypto { "typique des grandes cryptos, plus large sur les petites" } else { "typique des grandes actions américaines" }
        )
    };
    note.push(format!(
        "Frais de {}, glissement de {} et demi-écart achat/vente de {} par ordre inclus.",
        pct_fr(FEE_RATE * 100.0),
        pct3(SLIPPAGE * 100.0),
        pct3(spread / 2.0)
    ));
    if n == 0 {
        note.push("Aucun signal d'achat complet sur la période : rien à mesurer.".into());
    } else {
        if total_return < buy_and_hold {
            note.push(format!(
                "A fait moins bien que la simple détention sur cette période ({} contre {}).",
                pct_fr(total_return),
                pct_fr(buy_and_hold)
            ));
        } else {
            note.push(format!("A fait mieux que la simple détention sur cette période ({} contre {}).", pct_fr(total_return), pct_fr(buy_and_hold)));
        }
        if n < 10 {
            note.push(format!("Seulement {n} trade{} : résultats peu significatifs.", if n > 1 { "s" } else { "" }));
        }
    }
    note.push("Le passé ne garantit pas l'avenir.".into());

    let tested_bars = r.equity.len();
    let stats = trade_stats(&returns, &risks);
    let bias_notes = vec![
        "Signal calculé sur la bougie journalière clôturée, exécuté à l'ouverture suivante : aucune donnée future.".to_string(),
        format!(
            "Coûts payés à chaque ordre : frais {}, glissement {}, demi-écart achat/vente {}.",
            pct3(FEE_RATE * 100.0),
            pct3(SLIPPAGE * 100.0),
            pct3(spread / 2.0)
        ),
        "Paramètres fixes, ceux du signal en direct : aucune optimisation sur la période testée.".to_string(),
        "Régime de marché lu sur la bougie du signal, avec les seules données connues ce jour-là.".to_string(),
        format!(
            "Échantillon : {n} trade{} sur {tested_bars} bougies journalières{}.",
            if n > 1 { "s" } else { "" },
            if n < 30 { " (moins de 30 : peu significatif)" } else { "" }
        ),
    ];
    let details = TrackDetails {
        spread_pct: round_to(spread, 4),
        spread_measured: measured.is_some(),
        spread_note,
        expectancy: stats.expectancy.map(|x| round_to(x, 2)),
        avg_r: stats.avg_r.map(|x| round_to(x, 2)),
        regimes: regime_split(candles_daily, &r.trades, &returns)
            .into_iter()
            .map(|g| RegimeStat { win_rate: g.win_rate.map(|x| round_to(x, 1)), avg_return: g.avg_return.map(|x| round_to(x, 2)), ..g })
            .collect(),
        tested_bars,
        bias_notes,
    };

    let track = Track {
        period: format!("{} – {} (bougies journalières)", month_year(r.equity[0].time), month_year(r.equity[r.equity.len() - 1].time)),
        trades: n,
        win_rate: if n > 0 { round_to(wins.len() as f64 / n as f64 * 100.0, 1) } else { 0.0 },
        avg_win: mean(&wins),
        avg_loss: mean(&losses),
        profit_factor,
        sharpe,
        sortino,
        max_drawdown: round_to(-max_dd * 100.0, 2),
        total_return: round_to(total_return, 2),
        buy_and_hold: round_to(buy_and_hold, 2),
        fees_pct: round_to(FEE_RATE * 100.0, 4),
        slippage_pct: round_to(SLIPPAGE * 100.0, 4),
        losing_streak,
        note: note.join(" "),
        details,
    };
    Some((track, TrackRun { result: r, net_returns: returns, risks, curve }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slippage_and_ratios_by_hand() {
        // 1.10 × 0.9995 / 1.0005 − 1
        assert!((with_costs(10.0, SLIPPAGE) - 9.890_054_972_513_74).abs() < 1e-9);
        // Daily returns alternately 0 % and +1 % (40 of them): mean 0.005, sample deviation 0.005 × √(40/39).
        let mut curve = vec![1.0];
        for i in 0..40 {
            let last = *curve.last().unwrap();
            curve.push(last * if i % 2 == 1 { 1.01 } else { 1.0 });
        }
        let (sharpe, sortino) = ratios(&curve, 252.0);
        let expected = 0.005 / (0.005 * (40.0f64 / 39.0).sqrt()) * 252f64.sqrt();
        assert_eq!(sharpe, Some(round_to(expected, 2)));
        assert_eq!(sortino, None, "aucune journée en baisse");
        assert_eq!(ratios(&curve[..20], 252.0), (None, None), "moins de 30 rendements");
    }
}
