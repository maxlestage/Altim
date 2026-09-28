//! Buy zones by horizon, with Fibonacci retracements (`web/src/engine/fibonacci.ts`).
//!
//! Traders buy the pull-back of an upward move: after a rise from a low L to a high H, the price often comes back
//! to 38.2 %–65 % of the move before resuming (the "golden pocket" is 61.8 %–65 %). The move depends on the horizon:
//! - short term: 4 h candles, move of the last ~2 weeks (a few days to 2 weeks of holding);
//! - medium term: daily candles, move of the last ~4 months (a few weeks to a few months);
//! - long term: weekly candles, move of the last ~2 years (months to years).
//!
//! Fibonacci is a convention, not a law: each zone is checked on the asset's own history (how often a first entry
//! into the zone went back to the high before breaking the low, compared with random entries with the same
//! distances). No look-ahead: at each past candle, the move is recomputed with the data known at that time.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::Evidence;
use super::signal::{Series, atr, sanitize};
use crate::js::{fr, fr_sig};
use crate::types::{Candle, DAY_MS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Horizon {
    Short,
    Medium,
    Long,
}

impl Horizon {
    pub fn as_str(self) -> &'static str {
        match self {
            Horizon::Short => "short",
            Horizon::Medium => "medium",
            Horizon::Long => "long",
        }
    }
    pub fn parse(s: &str) -> Option<Horizon> {
        match s {
            "short" => Some(Horizon::Short),
            "medium" => Some(Horizon::Medium),
            "long" => Some(Horizon::Long),
            _ => None,
        }
    }
    /// `HORIZONS[h]`.
    pub fn cfg(self) -> &'static HorizonCfg {
        match self {
            Horizon::Short => &HORIZONS[0],
            Horizon::Medium => &HORIZONS[1],
            Horizon::Long => &HORIZONS[2],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ZoneStatus {
    Above,
    InZone,
    Golden,
    Deep,
    Broken,
    Downtrend,
    None,
}

impl ZoneStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ZoneStatus::Above => "above",
            ZoneStatus::InZone => "inZone",
            ZoneStatus::Golden => "golden",
            ZoneStatus::Deep => "deep",
            ZoneStatus::Broken => "broken",
            ZoneStatus::Downtrend => "downtrend",
            ZoneStatus::None => "none",
        }
    }
}

pub const RATIOS: [f64; 6] = [0.236, 0.382, 0.5, 0.618, 0.65, 0.786];
pub const EXTENSIONS: [f64; 2] = [1.272, 1.618];

/// Settings of a horizon (`HORIZONS[h]`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HorizonCfg {
    pub label: &'static str,
    pub unit: &'static str,
    pub holding: &'static str,
    pub window: usize,
    pub outcome: usize,
    pub min_atr: f64,
}

/// In the order short, medium, long (see `Horizon::cfg`).
pub const HORIZONS: [HorizonCfg; 3] = [
    HorizonCfg { label: "Court terme", unit: "4 h", holding: "quelques jours à 2 semaines", window: 90, outcome: 30, min_atr: 4.0 },
    HorizonCfg { label: "Moyen terme", unit: "1 j", holding: "quelques semaines à quelques mois", window: 120, outcome: 40, min_atr: 4.0 },
    HorizonCfg { label: "Long terme", unit: "1 sem.", holding: "plusieurs mois à plusieurs années", window: 104, outcome: 26, min_atr: 3.0 },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Trend {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Swing {
    pub trend: Trend,
    pub low: f64,
    pub high: f64,
    pub low_index: usize,
    pub high_index: usize,
    pub low_time: i64,
    pub high_time: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub ratio: f64,
    pub price: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Band {
    pub from: f64,
    pub to: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FibZone {
    pub horizon: Horizon,
    pub label: String,
    pub unit: String,
    pub holding: String,
    pub status: ZoneStatus,
    pub swing: Option<Swing>,
    /// Retracement levels of the up move (price for each ratio).
    pub levels: Vec<Level>,
    /// Buy zone 38.2 %–65 % (from = lower bound), golden pocket 61.8 %–65 %.
    pub zone: Option<Band>,
    pub golden: Option<Band>,
    /// Below this price (the low of the move), the zone is invalidated.
    pub invalidation: Option<f64>,
    /// Previous high, then extensions 127.2 % and 161.8 % of the move.
    pub targets: Vec<f64>,
    /// % to go down to reach the top of the zone (status "above").
    pub distance: Option<f64>,
    /// How the zones behaved on this asset's history.
    pub evidence: Option<Evidence>,
    pub text: String,
}

/// Weekly candles from daily ones (weeks starting Monday, UTC). The last week may be in progress.
pub fn weekly(daily: &[Candle]) -> Vec<Candle> {
    let mut out: Vec<Candle> = Vec::new();
    const MONDAY: i64 = 4 * DAY_MS; // 1970-01-05 was a Monday: epoch + 4 days
    let mut key: Option<i64> = None;
    for c in sanitize(daily) {
        let k = (c.time - MONDAY).div_euclid(7 * DAY_MS);
        match out.last_mut() {
            Some(last) if key == Some(k) => {
                last.high = last.high.max(c.high);
                last.low = last.low.min(c.low);
                last.close = c.close;
                last.volume += c.volume;
            }
            _ => {
                out.push(c);
                key = Some(k);
            }
        }
    }
    out
}

/// Last significant move known at candle `until`, within `window` candles:
/// - lowest low before the highest high → up move (buy zones in its pull-back);
/// - highest high before the lowest low → down move, unless the price has since rebounded by at least 38.2 % of the
///   fall: the rebound from the low is then the current up move.
///
/// A move smaller than `min_atr` ATR is ignored. `atr_series` defaults to `atr(c, 14)` (pass `None`).
pub fn swing_at(c: &[Candle], until: usize, window: usize, min_atr: f64, atr_series: Option<&Series>) -> Option<Swing> {
    let owned;
    let atr_series = match atr_series {
        Some(a) => a,
        None => {
            owned = atr(c, 14);
            &owned
        }
    };
    let start = (until + 1).saturating_sub(window);
    if until < start + 20 {
        return None;
    }
    let a = (*atr_series.get(until)?)?;
    let argmax = |from: usize| {
        let mut k = from;
        for i in from..=until {
            if c[i].high >= c[k].high {
                k = i;
            }
        }
        k
    };
    let mut lo = start;
    for i in start..=until {
        if c[i].low <= c[lo].low {
            lo = i;
        }
    }
    let hi = argmax(start);
    let make = |l: usize, h: usize, trend: Trend| Swing {
        trend,
        low: c[l].low,
        high: c[h].high,
        low_index: l,
        high_index: h,
        low_time: c[l].time,
        high_time: c[h].time,
    };
    if lo < hi {
        return if c[hi].high - c[lo].low >= min_atr * a { Some(make(lo, hi, Trend::Up)) } else { None };
    }
    if lo == until {
        return if c[hi].high - c[lo].low >= min_atr * a { Some(make(lo, hi, Trend::Down)) } else { None };
    }
    // Down move, then possibly a rebound from the low.
    let fall = c[hi].high - c[lo].low;
    let hi2 = argmax(lo + 1);
    let rebound = c[hi2].high - c[lo].low;
    if rebound >= min_atr * a && rebound >= 0.382 * fall {
        return Some(make(lo, hi2, Trend::Up));
    }
    if fall >= min_atr * a { Some(make(lo, hi, Trend::Down)) } else { None }
}

pub fn level(s: &Swing, r: f64) -> f64 {
    s.high - r * (s.high - s.low)
}

/// Outcome of an entry: 1 if the price reaches `up` before `down` within `bars`, 0 otherwise.
fn outcome(c: &[Candle], i: usize, up: f64, down: f64, bars: usize) -> f64 {
    let end = (c.len() - 1).min(i + bars);
    for x in c.iter().take(end + 1).skip(i + 1) {
        if x.low <= down {
            return 0.0; // pessimistic when both are touched in the same candle
        }
        if x.high >= up {
            return 1.0;
        }
    }
    0.0
}

/// History of the zones on this asset: every first entry into the 38.2–65 % zone of an up move (known at that time),
/// success = back to the high before breaking the low. Base = same distances from every candle of the history.
pub fn zone_evidence(c: &[Candle], h: Horizon) -> Option<Evidence> {
    let HorizonCfg { window, outcome: bars, min_atr, .. } = *h.cfg();
    let a = atr(c, 14);
    let mut events: Vec<(usize, f64, f64)> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();
    let end = c.len().saturating_sub(bars);
    for i in 30..end {
        let Some(s) = swing_at(c, i, window, min_atr, Some(&a)) else { continue };
        if s.trend != Trend::Up || seen.contains(&s.high_index) || s.high_index == i {
            continue;
        }
        let top = level(&s, 0.382);
        let bottom = level(&s, 0.65);
        let p = c[i].close;
        if p <= top && p >= bottom && c[i - 1].close > top {
            seen.insert(s.high_index);
            events.push((i, s.high / p, s.low / p));
        }
    }
    if events.is_empty() {
        return None;
    }
    let mut wins = 0.0;
    for &(i, up, down) in &events {
        wins += outcome(c, i, c[i].close * up, c[i].close * down, bars);
    }
    let (mut base_wins, mut base_n) = (0.0, 0.0);
    for &(_, up, down) in &events {
        for j in 30..end {
            base_wins += outcome(c, j, c[j].close * up, c[j].close * down, bars);
            base_n += 1.0;
        }
    }
    let n = events.len() as f64;
    let rate = (wins / n) * 100.0;
    let base = if base_n != 0.0 { (base_wins / base_n) * 100.0 } else { 0.0 };
    let lift = if base > 0.0 {
        rate / base
    } else if rate > 0.0 {
        2.0
    } else {
        1.0
    };
    Some(Evidence { samples: n, rate, base, lift })
}

fn px(v: f64) -> String {
    let s = if v >= 1.0 { fr(v, 2, 2) } else { fr_sig(v, 4) };
    format!("{s} $")
}

fn pct(v: f64) -> String {
    format!("{} %", fr(v, 0, 1))
}

/// Result of `zone_state`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneState {
    pub status: ZoneStatus,
    pub distance: Option<f64>,
    pub text: String,
}

/// Position of a price against the zones of an up move: status, distance to the zone, explanation.
pub fn zone_state(s: &Swing, p: f64) -> ZoneState {
    let top = level(s, 0.382);
    let golden_top = level(s, 0.618);
    let bottom = level(s, 0.65);
    let status = if p > top {
        ZoneStatus::Above
    } else if p >= golden_top {
        ZoneStatus::InZone
    } else if p >= bottom {
        ZoneStatus::Golden
    } else if p >= s.low {
        ZoneStatus::Deep
    } else {
        ZoneStatus::Broken
    };
    let distance = if status == ZoneStatus::Above { Some((1.0 - top / p) * 100.0) } else { None };
    let text = match status {
        ZoneStatus::Above => format!(
            "Prix au-dessus de la zone : attendre un repli vers {} – {} (−{} pour l'atteindre).",
            px(top),
            px(bottom),
            pct(distance.unwrap_or(0.0))
        ),
        ZoneStatus::InZone => format!("Prix dans la zone d'achat (38,2 % – 61,8 % du mouvement de {} à {}).", px(s.low), px(s.high)),
        ZoneStatus::Golden => "Prix dans la « zone d'or » (61,8 % – 65 %), le repli le plus surveillé par les traders.".to_string(),
        ZoneStatus::Deep => {
            format!("Repli profond (au-delà de 65 %) : dernier soutien avant {} ; un passage sous ce plus bas invalide la zone.", px(s.low))
        }
        ZoneStatus::Broken => format!("Le plus bas du mouvement ({}) est cassé : zone invalidée, attendre un nouveau point bas.", px(s.low)),
        ZoneStatus::Downtrend | ZoneStatus::None => String::new(),
    };
    ZoneState { status, distance, text }
}

/// Buy zone of one horizon, from the candles of that horizon (4 h, daily or weekly).
pub fn fib_zone(raw: &[Candle], h: Horizon, price: Option<f64>, with_evidence: bool) -> FibZone {
    let c = sanitize(raw);
    let cfg = h.cfg();
    let empty = |status: ZoneStatus, swing: Option<Swing>, levels: Vec<Level>, text: String| FibZone {
        horizon: h,
        label: cfg.label.into(),
        unit: cfg.unit.into(),
        holding: cfg.holding.into(),
        status,
        swing,
        levels,
        zone: None,
        golden: None,
        invalidation: None,
        targets: vec![],
        distance: None,
        evidence: None,
        text,
    };
    let s = if c.len() >= 30 { swing_at(&c, c.len() - 1, cfg.window, cfg.min_atr, None) } else { None };
    let Some(s) = s else {
        let text = if c.len() < 30 { "Historique insuffisant pour cet horizon." } else { "Pas de mouvement assez net pour tracer des niveaux." };
        return empty(ZoneStatus::None, None, vec![], text.into());
    };
    let p = price.unwrap_or(c[c.len() - 1].close);
    if s.trend == Trend::Down {
        let levels = RATIOS.iter().map(|&r| Level { ratio: r, price: s.low + r * (s.high - s.low) }).collect();
        let text = format!(
            "Mouvement baissier en cours (de {} à {}) : pas de zone d'achat en repli. Les niveaux de Fibonacci au-dessus du prix sont des résistances ; attendre qu'un nouveau creux se forme et que le prix reparte.",
            px(s.high),
            px(s.low)
        );
        return empty(ZoneStatus::Downtrend, Some(s), levels, text);
    }
    let levels = RATIOS.iter().map(|&r| Level { ratio: r, price: level(&s, r) }).collect();
    let zone = Band { from: level(&s, 0.65), to: level(&s, 0.382) };
    let golden = Band { from: level(&s, 0.65), to: level(&s, 0.618) };
    let mut targets = vec![s.high];
    targets.extend(EXTENSIONS.iter().map(|e| s.low + e * (s.high - s.low)));
    let evidence = if with_evidence { zone_evidence(&c, h) } else { None };
    let st = zone_state(&s, p);
    FibZone {
        horizon: h,
        label: cfg.label.into(),
        unit: cfg.unit.into(),
        holding: cfg.holding.into(),
        status: st.status,
        swing: Some(s),
        levels,
        zone: Some(zone),
        golden: Some(golden),
        invalidation: Some(s.low),
        targets,
        distance: st.distance,
        evidence,
        text: st.text,
    }
}

/// The three horizons: 4 h candles, daily candles, and a long daily history (≈ 3 years) turned into weeks.
pub fn fib_zones(h4: &[Candle], daily: &[Candle], long_daily: &[Candle], price: Option<f64>, with_evidence: bool) -> Vec<FibZone> {
    let w = weekly(if long_daily.len() > daily.len() { long_daily } else { daily });
    vec![
        fib_zone(h4, Horizon::Short, price, with_evidence),
        fib_zone(daily, Horizon::Medium, price, with_evidence),
        fib_zone(&w, Horizon::Long, price, with_evidence),
    ]
}
