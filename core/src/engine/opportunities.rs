//! "Opportunités du moment": scan categories computed on the daily candles the selection already loads
//! (`/api/opportunities`). Each category is a precise, checkable rule on closed sessions only; the reason given for
//! an asset states the measured values. A hit is something to look at, never a buy signal on its own.
use serde::Serialize;

use super::guard::{Direction, divergence};
use super::signal::{atr, rsi, sma};
use super::structure::{Side, donchian};
use crate::js::fr;
use crate::types::Candle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    /// Altim's selection (existing screener), to buy now.
    Setup,
    /// RSI leaving oversold + bullish divergence or a key average reclaimed.
    Reversal,
    /// Close above the 20- or 55-session high on volume ≥ 1.5× its average.
    Breakout,
    /// Volume ≥ 3× the 20-session average.
    Volume,
    /// RSI(14) < 25, or ≥ 2.5 ATR under the 20-session mean.
    Oversold,
    /// Stocks: a recent filing with a growth change, or analyst revisions; cryptos: TVL change over 30 days.
    Fundamentals,
}

pub const CATEGORIES: [Category; 6] =
    [Category::Setup, Category::Reversal, Category::Breakout, Category::Volume, Category::Oversold, Category::Fundamentals];

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Setup => "Configurations intéressantes",
            Category::Reversal => "Retournements potentiels",
            Category::Breakout => "Cassures (breakouts)",
            Category::Volume => "Anomalies de volume",
            Category::Oversold => "Fortement survendus",
            Category::Fundamentals => "Fondamentaux qui évoluent",
        }
    }
}

/// Thresholds (shown on the page with each rule).
pub const RSI_OVERSOLD: f64 = 30.0;
pub const RSI_DEEP: f64 = 25.0;
/// Sessions within which the RSI must have left the oversold zone.
pub const RSI_EXIT_WITHIN: usize = 5;
pub const BREAKOUT_VOLUME: f64 = 1.5;
pub const VOLUME_SPIKE: f64 = 3.0;
/// Distance under the 20-session mean, in ATR(14).
pub const FAR_BELOW_ATR: f64 = 2.5;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub category: Category,
    /// Measured values in words ("Clôture 123 $ au-dessus du plus haut 55 j (120 $), volume ×1,8").
    pub reason: String,
    /// For the order within a category (higher = stronger reading).
    pub strength: f64,
}

/// Figures of the last closed session used by the filters and the cards.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub price: f64,
    pub time: i64,
    /// Change of the last session, %.
    pub change1d: Option<f64>,
    pub rsi14: Option<f64>,
    /// Last volume ÷ average of the 20 sessions before.
    pub volume_ratio: Option<f64>,
    /// ATR(14) ÷ price, % per day.
    pub volatility: Option<f64>,
    /// Average daily traded value (close × volume) over 20 sessions, USD (the candle source's volume).
    pub liquidity: Option<f64>,
    /// (close − 20-session mean) ÷ ATR(14).
    pub distance_atr: Option<f64>,
}

fn fx(v: f64) -> String {
    format!("×{}", fr(v, 0, 1))
}

fn price(v: f64) -> String {
    crate::fx::money_with(v, |v| {
        let d = if v >= 100.0 {
            0
        } else if v >= 1.0 {
            2
        } else {
            4
        };
        fr(v, 0, d)
    })
}

fn valid(c: &Candle) -> bool {
    c.close.is_finite() && c.close > 0.0 && c.high.is_finite() && c.low.is_finite() && c.volume.is_finite() && c.volume >= 0.0
}

fn avg_volume(c: &[Candle]) -> Option<f64> {
    if c.is_empty() {
        return None;
    }
    let m = c.iter().map(|x| x.volume).sum::<f64>() / c.len() as f64;
    (m > 0.0).then_some(m)
}

pub fn metrics(c: &[Candle]) -> Option<Metrics> {
    let n = c.len();
    if n < 2 || !valid(&c[n - 1]) {
        return None;
    }
    let last = c[n - 1];
    let closes: Vec<f64> = c.iter().map(|x| x.close).collect();
    let r = rsi(&closes, 14);
    let a = atr(c, 14);
    let s20 = sma(&closes, 20);
    let atr_last = a[n - 1].filter(|v| *v > 0.0);
    let ratio = (n > 20).then(|| avg_volume(&c[n - 21..n - 1]).map(|m| last.volume / m)).flatten();
    let liquidity = (n >= 20 && c[n - 20..].iter().all(valid)).then(|| c[n - 20..].iter().map(|x| x.close * x.volume).sum::<f64>() / 20.0);
    Some(Metrics {
        price: last.close,
        time: last.time,
        change1d: (c[n - 2].close > 0.0).then(|| (last.close / c[n - 2].close - 1.0) * 100.0),
        rsi14: r[n - 1],
        volume_ratio: ratio,
        volatility: atr_last.map(|v| v / last.close * 100.0),
        liquidity: liquidity.filter(|v| *v > 0.0),
        distance_atr: match (s20[n - 1], atr_last) {
            (Some(m), Some(at)) => Some((last.close - m) / at),
            _ => None,
        },
    })
}

/// RSI(14) left the oversold zone within the last sessions: (lowest RSI in the zone, RSI now).
fn rsi_exit(r: &[Option<f64>]) -> Option<(f64, f64)> {
    let n = r.len();
    let now = r[n - 1]?;
    if now < RSI_OVERSOLD || n < RSI_EXIT_WITHIN + 1 {
        return None;
    }
    let low = r[n - 1 - RSI_EXIT_WITHIN..n - 1].iter().flatten().copied().fold(f64::INFINITY, f64::min);
    (low < RSI_OVERSOLD).then_some((low, now))
}

/// Key average (20 or 50 sessions) crossed from below within the last 3 sessions, close still above.
fn reclaimed(c: &[Candle], closes: &[f64]) -> Option<usize> {
    let n = c.len();
    for p in [20usize, 50] {
        let m = sma(closes, p);
        let Some(now) = m[n - 1] else { continue };
        if closes[n - 1] <= now {
            continue;
        }
        let crossed = (n.saturating_sub(4)..n - 1).any(|k| m[k].is_some_and(|mk| closes[k] <= mk));
        if crossed {
            return Some(p);
        }
    }
    None
}

/// Technical categories of one asset on its closed daily candles (only candles 0…n−1 are read).
pub fn technical_hits(c: &[Candle]) -> Vec<Hit> {
    let n = c.len();
    let mut out = Vec::new();
    if n < 60 || !c[n - 56..].iter().all(valid) {
        return out;
    }
    let last = c[n - 1];
    let closes: Vec<f64> = c.iter().map(|x| x.close).collect();
    let r = rsi(&closes, 14);
    let Some(m) = metrics(c) else { return out };

    // Retournement potentiel.
    if let Some((low, now)) = rsi_exit(&r) {
        let div = divergence(c, &r, Direction::Up, n as i64 - 1, 60);
        let ma = reclaimed(c, &closes);
        if div || ma.is_some() {
            let mut why = vec![format!("RSI 14 sorti de la survente ({} → {})", fr(low, 0, 0), fr(now, 0, 0))];
            if div {
                why.push("divergence haussière RSI (plus bas du prix plus bas, RSI plus haut)".into());
            }
            if let Some(p) = ma {
                why.push(format!("clôture repassée au-dessus de la moyenne {p} j"));
            }
            out.push(Hit { category: Category::Reversal, reason: why.join(" + "), strength: now - low + if div { 10.0 } else { 0.0 } });
        }
    }

    // Cassure : clôture au-dessus du plus haut des 20 ou 55 séances précédentes, volume ≥ 1,5× la moyenne 20 j.
    // The Donchian channels of the decision's structure (highest high of the sessions before the last one).
    if let Some(ratio) = m.volume_ratio.filter(|v| *v >= BREAKOUT_VOLUME) {
        let up = |k: usize| donchian(c, k).filter(|d| d.breakout == Some(Side::Up)).map(|d| (k, d.upper));
        if let Some((k, h)) = up(55).or_else(|| up(20)) {
            out.push(Hit {
                category: Category::Breakout,
                reason: format!(
                    "Clôture {} au-dessus du plus haut {k} j ({}), volume {} par rapport à la moyenne 20 j",
                    price(last.close),
                    price(h),
                    fx(ratio)
                ),
                strength: ratio + if k == 55 { 1.0 } else { 0.0 },
            });
        }
    }

    // Anomalie de volume.
    if let Some(ratio) = m.volume_ratio.filter(|v| *v >= VOLUME_SPIKE) {
        let ch = m.change1d.unwrap_or(0.0);
        out.push(Hit {
            category: Category::Volume,
            reason: format!(
                "Volume {} par rapport à la moyenne 20 j, séance {}{} %",
                fx(ratio),
                if ch >= 0.0 { "+" } else { "−" },
                fr(ch.abs(), 0, 1)
            ),
            strength: ratio,
        });
    }

    // Fortement survendu.
    let deep = m.rsi14.filter(|v| *v < RSI_DEEP);
    let far = m.distance_atr.filter(|v| *v <= -FAR_BELOW_ATR);
    if deep.is_some() || far.is_some() {
        let mut why = Vec::new();
        if let Some(v) = deep {
            why.push(format!("RSI 14 à {} (seuil {})", fr(v, 0, 0), fr(RSI_DEEP, 0, 0)));
        }
        if let Some(d) = far {
            why.push(format!("{} ATR sous la moyenne 20 j (seuil {})", fr(d.abs(), 1, 1), fr(FAR_BELOW_ATR, 1, 1)));
        }
        out.push(Hit {
            category: Category::Oversold,
            reason: why.join(", "),
            strength: deep.map_or(0.0, |v| RSI_DEEP - v) + far.map_or(0.0, |d| d.abs() * 4.0),
        });
    }
    out
}
