//! Track record of the signal on one asset for `/api/decision`: the backtest of `backtest.rs` (unchanged, walk
//! forward: the signal is computed on the closed candle i and executed at the open of i + 1), with a slippage
//! assumption added on top of its fees, then the usual trade and risk statistics.
use super::backtest::{BacktestResult, FEE_RATE, backtest_default};
use super::decision_types::Track;
use crate::js::fr;
use crate::types::{Candle, Kind};

/// Slippage assumed on each side of a trade, fraction (0.05 %).
pub const SLIPPAGE: f64 = 0.0005;

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
    format!("{}{} %", if x < 0.0 { "−" } else { "" }, fr(x.abs(), 0, 1))
}

/// Trade return (%) once the slippage is paid on the buy and on the sell.
fn with_slippage(return_percent: f64) -> f64 {
    ((1.0 + return_percent / 100.0) * (1.0 - SLIPPAGE) / (1.0 + SLIPPAGE) - 1.0) * 100.0
}

/// Equity curve with the slippage: every entry costs 1/(1 + s), every exit (1 − s), from the bar where it happens.
fn adjusted_curve(r: &BacktestResult) -> Vec<f64> {
    r.equity
        .iter()
        .map(|p| {
            let entries = r.trades.iter().filter(|t| t.entry_time <= p.time).count() as i32;
            let exits = r.trades.iter().filter(|t| t.exit_time <= p.time).count() as i32;
            p.equity * (1.0 + SLIPPAGE).powi(-entries) * (1.0 - SLIPPAGE).powi(exits)
        })
        .collect()
}

/// Mean ÷ deviation of the daily returns, annualised; None with fewer than 30 returns or no variation.
fn ratios(curve: &[f64], per_year: f64) -> (Option<f64>, Option<f64>) {
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

/// Track record on daily candles (oldest first). None when there are too few candles for the backtest (it needs
/// its 60-candle warm-up plus at least one candle to test).
pub fn track(candles_daily: &[Candle], kind: Kind) -> Option<Track> {
    let r = backtest_default(candles_daily);
    if r.equity.is_empty() {
        return None;
    }
    let returns: Vec<f64> = r.trades.iter().map(|t| with_slippage(t.return_percent)).collect();
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
    let curve = adjusted_curve(&r);
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
    note.push(format!("Frais de {} et glissement de {} par ordre inclus.", pct_fr(FEE_RATE * 100.0), fr(SLIPPAGE * 100.0, 2, 2) + " %"));
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

    Some(Track {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slippage_and_ratios_by_hand() {
        // 1.10 × 0.9995 / 1.0005 − 1
        assert!((with_slippage(10.0) - 9.890_054_972_513_74).abs() < 1e-9);
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
