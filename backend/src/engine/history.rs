//! History of the portfolio (`web/src/engine/history.ts`): what the lines held today were worth each day of the
//! period, from the daily closes of each asset, compared with Bitcoin and the S&P 500 held over the same days. Past
//! purchases and sales are not known: the curve answers "how did what I own now behave", not "how did my account do".
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::DAY_MS;

/// `[time, close]`.
pub type Close = (i64, f64);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryLine {
    pub id: String,
    pub quantity: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub t: i64,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkPoint {
    pub t: i64,
    pub pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Benchmark {
    pub id: String,
    pub label: String,
    /// Change since the first day of the curve, in %.
    pub change: f64,
    /// Change in % since the first day, one point per day of the curve.
    pub points: Vec<BenchmarkPoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DayChange {
    pub t: i64,
    pub change: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioHistory {
    pub points: Vec<HistoryPoint>,
    /// Change of the value between the first and the last day, in %.
    pub change: f64,
    /// Deepest fall from a previous high, in % (negative or 0).
    pub max_drawdown: f64,
    pub best: Option<DayChange>,
    pub worst: Option<DayChange>,
    pub benchmarks: Vec<Benchmark>,
    /// Lines left out: no history on the whole period (asset too recent, source down).
    pub missing: Vec<String>,
    /// The curve starts later than asked because a line has a shorter history.
    pub shortened: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchmarkDef {
    pub id: &'static str,
    pub label: &'static str,
}

pub const BENCHMARKS: [BenchmarkDef; 2] =
    [BenchmarkDef { id: "crypto:BTC", label: "Bitcoin" }, BenchmarkDef { id: "stock:SPY", label: "S&P 500 (SPY)" }];

fn day_of(t: i64) -> i64 {
    t.div_euclid(DAY_MS) * DAY_MS
}

/// Last close known at the end of each day of the grid (weekends and holidays keep the Friday close).
fn on_grid(closes: &[Close], grid: &[i64]) -> Vec<Option<f64>> {
    let mut sorted: Vec<Close> = closes.iter().filter(|(_, c)| c.is_finite() && *c > 0.0).copied().collect();
    sorted.sort_by_key(|c| c.0);
    let mut out = Vec::with_capacity(grid.len());
    let mut i = 0;
    let mut last: Option<f64> = None;
    for &d in grid {
        while i < sorted.len() && day_of(sorted[i].0) <= d {
            last = Some(sorted[i].1);
            i += 1;
        }
        out.push(last);
    }
    out
}

pub fn portfolio_history(lines: &[HistoryLine], series: &HashMap<String, Vec<Close>>, days: usize, now: i64) -> Option<PortfolioHistory> {
    let empty: Vec<Close> = Vec::new();
    let get = |id: &str| series.get(id).unwrap_or(&empty);
    let held: Vec<&HistoryLine> = lines.iter().filter(|l| l.quantity > 0.0).collect();
    // The curve ends on the last closed day of the held assets (only closed candles are served).
    let last_time = held.iter().flat_map(|l| get(&l.id).iter().map(|c| c.0)).filter(|t| *t <= now).max();
    let end = day_of(last_time.unwrap_or(now));
    let grid: Vec<i64> = (0..=days).map(|i| end - (days - i) as i64 * DAY_MS).collect();
    let mut missing: Vec<String> = Vec::new();
    let mut valued: Vec<(f64, Vec<Option<f64>>)> = Vec::new();
    for l in &held {
        let g = on_grid(get(&l.id), &grid);
        if g[g.len() - 1].is_none() {
            missing.push(l.id.clone());
        } else {
            valued.push((l.quantity, g));
        }
    }
    if valued.is_empty() {
        return None;
    }
    // The curve starts on the first day every line has a price: an asset listed later would otherwise look like a gain.
    let start = (0..grid.len()).find(|&i| valued.iter().all(|v| v.1[i].is_some()))?;
    if grid.len() - start < 2 {
        return None;
    }
    let points: Vec<HistoryPoint> = (start..grid.len())
        .map(|i| HistoryPoint { t: grid[i], value: valued.iter().fold(0.0, |s, v| s + v.0 * v.1[i].unwrap()) })
        .collect();

    let first = points[0].value;
    let last = points[points.len() - 1].value;
    let mut peak = first;
    let mut max_drawdown: f64 = 0.0;
    let mut best: Option<DayChange> = None;
    let mut worst: Option<DayChange> = None;
    for i in 0..points.len() {
        let v = points[i].value;
        peak = peak.max(v);
        max_drawdown = max_drawdown.min((v / peak - 1.0) * 100.0);
        if i > 0 {
            let c = (v / points[i - 1].value - 1.0) * 100.0;
            if best.is_none_or(|b| c > b.change) {
                best = Some(DayChange { t: points[i].t, change: c });
            }
            if worst.is_none_or(|w| c < w.change) {
                worst = Some(DayChange { t: points[i].t, change: c });
            }
        }
    }

    let mut benchmarks: Vec<Benchmark> = Vec::new();
    for b in BENCHMARKS {
        let g = &on_grid(get(b.id), &grid)[start..];
        let Some(base) = g[0] else { continue };
        if g.iter().any(|c| c.is_none()) {
            continue;
        }
        let pts: Vec<BenchmarkPoint> =
            g.iter().enumerate().map(|(i, c)| BenchmarkPoint { t: points[i].t, pct: (c.unwrap() / base - 1.0) * 100.0 }).collect();
        benchmarks.push(Benchmark { id: b.id.into(), label: b.label.into(), change: pts[pts.len() - 1].pct, points: pts });
    }

    Some(PortfolioHistory {
        points,
        change: (last / first - 1.0) * 100.0,
        max_drawdown,
        best: best.filter(|b| b.change > 0.0),
        worst: worst.filter(|w| w.change < 0.0),
        benchmarks,
        missing,
        shortened: start > 0,
    })
}
