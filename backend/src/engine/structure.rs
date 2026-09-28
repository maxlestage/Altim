//! Technical structure of an asset (`structure` of `/api/decision`): Ichimoku, Supertrend, Donchian channel, rolling
//! VWAP, volume profile, floor pivots, support / resistance levels, breakouts and fake breakouts, swing structure and
//! strength relative to a benchmark. Pure functions on closed candles (daily in the decision); every item carries a
//! short French reading.
//!
//! Proxies are named as such, never passed off as the real thing:
//! - the VWAP is rolling over the last `VWAP_BARS` candles (typical price × volume), not anchored to a session;
//! - the volume profile spreads each candle's volume evenly over its high–low range: an approximation, the real
//!   distribution of the trades inside a candle is not in free data.
use serde::{Deserialize, Serialize};

use super::decision::{aligned_returns, correlation};
use super::format::format_price;
use super::guard::pivots;
use super::signal::{Candle, atr, clamp};
use crate::js::{fr, round};
use crate::types::{DAY_MS, Kind};

// ---------- Parameters ----------

pub const TENKAN: usize = 9;
pub const KIJUN: usize = 26;
pub const SENKOU_B: usize = 52;
/// A Tenkan / Kijun cross is reported when it happened within this many candles.
pub const TK_CROSS_BARS: usize = 10;
pub const SUPERTREND_ATR: usize = 10;
pub const SUPERTREND_MULT: f64 = 3.0;
pub const DONCHIAN: usize = 20;
pub const VWAP_BARS: usize = 20;
pub const PROFILE_BARS: usize = 120;
pub const PROFILE_BINS: usize = 24;
/// Share of the volume inside the value area.
pub const VALUE_AREA: f64 = 0.70;
/// Swing highs / lows: extreme of this many candles on each side.
pub const SWING_K: usize = 3;
/// Candles searched for swing highs / lows (levels and structure).
pub const LEVEL_BARS: usize = 250;
/// Swings closer than this many ATR form one level.
pub const LEVEL_ATR: f64 = 0.5;
pub const MIN_TOUCHES: usize = 2;
/// Close beyond a level with a volume at least this many times the 20-candle average: confirmed breakout.
pub const BREAKOUT_VOLUME: f64 = 1.5;
/// Candles within which a break that closes back inside is a fake breakout.
pub const FAKE_BARS: usize = 3;

// ---------- Contract ----------

/// Direction an item points to (for the colour and the composite score).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Bias {
    Bullish,
    Neutral,
    Bearish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudPosition {
    Above,
    Inside,
    Below,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ichimoku {
    pub tenkan: f64,
    pub kijun: f64,
    /// Cloud under the current candle (spans computed 26 candles earlier).
    pub senkou_a: f64,
    pub senkou_b: f64,
    /// Cloud projected 26 candles ahead (spans of the last candle).
    pub future_a: f64,
    pub future_b: f64,
    pub position: CloudPosition,
    /// Tenkan / Kijun cross within the last `TK_CROSS_BARS` candles: its side and how many candles ago.
    pub tk_cross: Option<Side>,
    pub tk_cross_bars: Option<usize>,
    pub bias: Bias,
    pub reading: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Supertrend {
    pub direction: Side,
    /// Trailing level (under the price when up, above when down).
    pub level: f64,
    /// Candles since the last change of direction (all of the history when it never changed).
    pub bars: usize,
    pub bias: Bias,
    pub reading: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Donchian {
    /// Highest high / lowest low of the 20 candles before the last one.
    pub upper: f64,
    pub lower: f64,
    pub mid: f64,
    /// Last close beyond the channel.
    pub breakout: Option<Side>,
    pub bias: Bias,
    pub reading: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vwap {
    pub value: f64,
    /// Candles of the rolling window.
    pub bars: usize,
    /// Price vs VWAP, %.
    pub deviation_pct: f64,
    pub bias: Bias,
    pub reading: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeProfile {
    /// Point of control: middle of the price bin with the most volume.
    pub poc: f64,
    pub value_area_high: f64,
    pub value_area_low: f64,
    pub bars: usize,
    pub bins: usize,
    pub bias: Bias,
    pub reading: String,
    /// "Approximation à partir des bougies …"
    pub note: String,
}

/// Classic floor pivots of the previous completed daily candle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorPivots {
    pub pivot: f64,
    pub r1: f64,
    pub r2: f64,
    pub s1: f64,
    pub s2: f64,
    /// Time of the daily candle they come from (ms).
    pub from: i64,
    pub reading: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LevelKind {
    Support,
    Resistance,
}

/// A price where several swing highs / lows gathered (within half an ATR).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SrLevel {
    pub price: f64,
    /// Swings at this level (the strength).
    pub touches: usize,
    pub kind: LevelKind,
    /// Distance from the price, % (positive above).
    pub distance_pct: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BreakoutKind {
    /// Close beyond the level with a volume ≥ 1.5 × the 20-candle average.
    Confirmed,
    /// Close beyond the level without that volume.
    Unconfirmed,
    /// Broke the level (intraday or on a close) then closed back inside within 3 candles.
    Fake,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakout {
    pub kind: BreakoutKind,
    /// Up = a resistance, down = a support.
    pub side: Option<Side>,
    pub level: Option<f64>,
    /// Volume of the last candle ÷ average of the 20 before it.
    pub volume_ratio: Option<f64>,
    pub bias: Bias,
    pub reading: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SwingTrend {
    /// Higher highs and higher lows.
    Up,
    /// Lower highs and lower lows.
    Down,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketStructure {
    pub trend: SwingTrend,
    /// Last two swing highs and lows, oldest first.
    pub highs: Vec<f64>,
    pub lows: Vec<f64>,
    pub bias: Bias,
    pub reading: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RsPeriod {
    /// "1 mois"
    pub label: String,
    pub days: i64,
    pub asset_pct: f64,
    pub benchmark_pct: f64,
    /// Asset return − benchmark return, percentage points.
    pub diff: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelativeStrength {
    /// "S&P 500 (SPY)"
    pub benchmark: String,
    pub symbol: String,
    pub periods: Vec<RsPeriod>,
    /// Correlation of the daily returns over 90 days (informational).
    pub correlation: Option<f64>,
    pub bias: Bias,
    pub reading: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Structure {
    /// "Bougies journalières clôturées"
    pub timeframe: String,
    /// −100 … +100: mean of the items' directions (used by the composite score); None without any item.
    pub score: Option<f64>,
    pub ichimoku: Option<Ichimoku>,
    pub supertrend: Option<Supertrend>,
    pub donchian: Option<Donchian>,
    pub vwap: Option<Vwap>,
    pub volume_profile: Option<VolumeProfile>,
    pub pivots: Option<FloorPivots>,
    /// Levels touched at least twice, nearest first.
    pub levels: Vec<SrLevel>,
    pub nearest_support: Option<SrLevel>,
    pub nearest_resistance: Option<SrLevel>,
    pub levels_reading: String,
    pub breakout: Option<Breakout>,
    pub market_structure: Option<MarketStructure>,
    pub relative: Vec<RelativeStrength>,
    /// Why there is no relative strength (the asset is the benchmark, history too short…).
    pub relative_note: Option<String>,
}

/// A benchmark's daily candles (Bitcoin for cryptos; the S&P 500 and Nasdaq-100 ETFs for stocks).
#[derive(Debug, Clone, PartialEq)]
pub struct Benchmark {
    /// "S&P 500 (SPY)"
    pub name: String,
    pub symbol: String,
    pub kind: Kind,
    pub daily: Vec<Candle>,
}

// ---------- Helpers ----------

fn usd(v: f64) -> String {
    format!("{} $", format_price(v))
}
fn signed(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, 1))
}
fn pts(v: f64) -> String {
    format!("{}{} pts", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, 1))
}
fn r2(v: f64) -> f64 {
    round(v * 100.0) / 100.0
}
fn bars(n: usize) -> String {
    format!("{n} bougie{}", if n > 1 { "s" } else { "" })
}
fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}
fn highest(c: &[Candle]) -> f64 {
    c.iter().map(|x| x.high).fold(f64::NEG_INFINITY, f64::max)
}
fn lowest(c: &[Candle]) -> f64 {
    c.iter().map(|x| x.low).fold(f64::INFINITY, f64::min)
}
/// Middle of the high–low range of the `p` candles ending at `i` (Ichimoku lines).
fn mid(c: &[Candle], i: usize, p: usize) -> f64 {
    let w = &c[i + 1 - p..=i];
    (highest(w) + lowest(w)) / 2.0
}

// ---------- Ichimoku (9 / 26 / 52) ----------

pub fn ichimoku(c: &[Candle], price: f64) -> Option<Ichimoku> {
    let n = c.len();
    // The cloud under the last candle was computed 26 candles earlier, on 52 candles.
    if n < SENKOU_B + KIJUN {
        return None;
    }
    let last = n - 1;
    let (tenkan, kijun) = (mid(c, last, TENKAN), mid(c, last, KIJUN));
    let j = last - KIJUN;
    let senkou_a = (mid(c, j, TENKAN) + mid(c, j, KIJUN)) / 2.0;
    let senkou_b = mid(c, j, SENKOU_B);
    let (top, bottom) = (senkou_a.max(senkou_b), senkou_a.min(senkou_b));
    let position = if price > top {
        CloudPosition::Above
    } else if price < bottom {
        CloudPosition::Below
    } else {
        CloudPosition::Inside
    };
    // Last Tenkan / Kijun cross within the window.
    let (mut tk_cross, mut tk_cross_bars) = (None, None);
    for back in 0..TK_CROSS_BARS {
        let i = last - back;
        if i < KIJUN {
            break;
        }
        let now = mid(c, i, TENKAN) - mid(c, i, KIJUN);
        let before = mid(c, i - 1, TENKAN) - mid(c, i - 1, KIJUN);
        if now > 0.0 && before <= 0.0 {
            (tk_cross, tk_cross_bars) = (Some(Side::Up), Some(back));
            break;
        }
        if now < 0.0 && before >= 0.0 {
            (tk_cross, tk_cross_bars) = (Some(Side::Down), Some(back));
            break;
        }
    }
    let bias = match position {
        CloudPosition::Above if tenkan > kijun => Bias::Bullish,
        CloudPosition::Below if tenkan < kijun => Bias::Bearish,
        _ => Bias::Neutral,
    };
    let mut reading = match position {
        CloudPosition::Above => "Prix au-dessus du nuage Ichimoku".to_string(),
        CloudPosition::Below => "Prix sous le nuage Ichimoku".to_string(),
        CloudPosition::Inside => "Prix dans le nuage Ichimoku (zone d'indécision)".to_string(),
    };
    reading += if tenkan > kijun {
        ", Tenkan au-dessus de la Kijun"
    } else if tenkan < kijun {
        ", Tenkan sous la Kijun"
    } else {
        ", Tenkan égale à la Kijun"
    };
    if let (Some(s), Some(b)) = (tk_cross, tk_cross_bars) {
        let when = if b == 0 { "sur la dernière bougie".to_string() } else { format!("il y a {}", bars(b)) };
        reading += &format!(" (croisement {} {when})", if s == Side::Up { "haussier" } else { "baissier" });
    }
    let future_a = (tenkan + kijun) / 2.0;
    let future_b = mid(c, last, SENKOU_B);
    Some(Ichimoku { tenkan, kijun, senkou_a, senkou_b, future_a, future_b, position, tk_cross, tk_cross_bars, bias, reading })
}

// ---------- Supertrend (ATR 10, × 3) ----------

pub fn supertrend(c: &[Candle], period: usize, mult: f64) -> Option<Supertrend> {
    let start = period.checked_sub(1)?;
    if c.len() < period + 2 {
        return None;
    }
    let a = atr(c, period);
    let hl2 = |i: usize| (c[i].high + c[i].low) / 2.0;
    let atr0 = a[start]?;
    let (mut upper, mut lower) = (hl2(start) + mult * atr0, hl2(start) - mult * atr0);
    let mut up = c[start].close >= hl2(start);
    let mut since = start;
    for i in start + 1..c.len() {
        let ai = a[i]?;
        let (bu, bl) = (hl2(i) + mult * ai, hl2(i) - mult * ai);
        // The bands only tighten, unless the previous close went through them.
        upper = if bu < upper || c[i - 1].close > upper { bu } else { upper };
        lower = if bl > lower || c[i - 1].close < lower { bl } else { lower };
        let flip = if up { c[i].close < lower } else { c[i].close > upper };
        if flip {
            up = !up;
            since = i;
        }
    }
    let n = c.len() - since;
    let level = if up { lower } else { upper };
    let reading = format!(
        "Supertrend {} depuis {}{} (niveau {})",
        if up { "haussier" } else { "baissier" },
        if since == start { "au moins " } else { "" },
        bars(n),
        usd(level)
    );
    Some(Supertrend {
        direction: if up { Side::Up } else { Side::Down },
        level,
        bars: n,
        bias: if up { Bias::Bullish } else { Bias::Bearish },
        reading,
    })
}

// ---------- Donchian (20) ----------

pub fn donchian(c: &[Candle], period: usize) -> Option<Donchian> {
    let n = c.len();
    if n < period + 1 || period == 0 {
        return None;
    }
    let w = &c[n - 1 - period..n - 1];
    let (upper, lower) = (highest(w), lowest(w));
    let close = c[n - 1].close;
    let breakout = if close > upper {
        Some(Side::Up)
    } else if close < lower {
        Some(Side::Down)
    } else {
        None
    };
    let (bias, reading) = match breakout {
        Some(Side::Up) => {
            (Bias::Bullish, format!("Clôture au-dessus du plus haut des {period} bougies précédentes ({}) : cassure haussière", usd(upper)))
        }
        Some(Side::Down) => {
            (Bias::Bearish, format!("Clôture sous le plus bas des {period} bougies précédentes ({}) : cassure baissière", usd(lower)))
        }
        None => (Bias::Neutral, format!("Dans le canal de Donchian {period} ({} – {})", usd(lower), usd(upper))),
    };
    Some(Donchian { upper, lower, mid: (upper + lower) / 2.0, breakout, bias, reading })
}

// ---------- Rolling VWAP ----------

pub fn vwap(c: &[Candle], period: usize, price: f64) -> Option<Vwap> {
    if c.len() < period || period == 0 {
        return None;
    }
    let w = &c[c.len() - period..];
    let vol: f64 = w.iter().map(|x| x.volume).sum();
    if vol <= 0.0 {
        return None;
    }
    let value = w.iter().map(|x| (x.high + x.low + x.close) / 3.0 * x.volume).sum::<f64>() / vol;
    let deviation_pct = (price / value - 1.0) * 100.0;
    let bias = if deviation_pct > 0.5 {
        Bias::Bullish
    } else if deviation_pct < -0.5 {
        Bias::Bearish
    } else {
        Bias::Neutral
    };
    let reading = format!(
        "Prix {} du VWAP glissant sur {period} bougies ({}, {}) ; glissant, pas ancré à une séance",
        if deviation_pct >= 0.0 { "au-dessus" } else { "en dessous" },
        usd(value),
        signed(deviation_pct)
    );
    Some(Vwap { value, bars: period, deviation_pct: r2(deviation_pct), bias, reading })
}

// ---------- Volume profile (approximation) ----------

/// Volume by price over the last `count` candles, each candle's volume spread evenly over its high–low range.
pub fn volume_profile(c: &[Candle], count: usize, bins: usize, price: f64) -> Option<VolumeProfile> {
    if c.len() < 20 || bins < 3 {
        return None;
    }
    let w = &c[c.len().saturating_sub(count)..];
    let (lo, hi) = (lowest(w), highest(w));
    if hi <= lo {
        return None;
    }
    let step = (hi - lo) / bins as f64;
    let mut vol = vec![0.0; bins];
    for x in w.iter().filter(|x| x.volume > 0.0) {
        if x.high <= x.low {
            vol[(((x.close - lo) / step) as usize).min(bins - 1)] += x.volume;
            continue;
        }
        for (b, v) in vol.iter_mut().enumerate() {
            let (from, to) = (lo + step * b as f64, lo + step * (b + 1) as f64);
            let overlap = x.high.min(to) - x.low.max(from);
            if overlap > 0.0 {
                *v += x.volume * overlap / (x.high - x.low);
            }
        }
    }
    let total: f64 = vol.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let poc_bin = (0..bins).fold(0, |best, b| if vol[b] > vol[best] { b } else { best });
    // Value area: from the POC, add the heavier neighbouring bin until 70 % of the volume.
    let (mut a, mut b) = (poc_bin, poc_bin);
    let mut inside = vol[poc_bin];
    while inside < VALUE_AREA * total && (a > 0 || b < bins - 1) {
        let below = if a > 0 { vol[a - 1] } else { -1.0 };
        let above = if b < bins - 1 { vol[b + 1] } else { -1.0 };
        if above >= below {
            b += 1;
            inside += above;
        } else {
            a -= 1;
            inside += below;
        }
    }
    let poc = lo + step * (poc_bin as f64 + 0.5);
    let (val, vah) = (lo + step * a as f64, lo + step * (b + 1) as f64);
    let (bias, where_) = if price > vah {
        (Bias::Bullish, "au-dessus de la zone de valeur")
    } else if price < val {
        (Bias::Bearish, "sous la zone de valeur")
    } else {
        (Bias::Neutral, "dans la zone de valeur")
    };
    Some(VolumeProfile {
        poc,
        value_area_high: vah,
        value_area_low: val,
        bars: w.len(),
        bins,
        bias,
        reading: format!("Prix {where_} ({} – {}), prix le plus échangé vers {}", usd(val), usd(vah), usd(poc)),
        note: format!(
            "Approximation à partir des bougies : le volume de chacune des {} dernières bougies est réparti uniformément entre son plus bas et son plus haut ({bins} tranches de prix, zone de valeur = 70 % du volume)",
            w.len()
        ),
    })
}

// ---------- Floor pivots ----------

/// Classic pivots of the last daily candle (a completed one: only closed candles are analysed).
pub fn floor_pivots(daily: &[Candle], price: f64) -> Option<FloorPivots> {
    let d = daily.last()?;
    let p = (d.high + d.low + d.close) / 3.0;
    let range = d.high - d.low;
    let (r1, s1, r2, s2) = (2.0 * p - d.low, 2.0 * p - d.high, p + range, p - range);
    let reading = if price >= r1 {
        format!("Prix au-dessus de R1 ({})", usd(r1))
    } else if price >= p {
        format!("Prix entre le pivot ({}) et R1 ({})", usd(p), usd(r1))
    } else if price > s1 {
        format!("Prix entre S1 ({}) et le pivot ({})", usd(s1), usd(p))
    } else {
        format!("Prix sous S1 ({})", usd(s1))
    };
    Some(FloorPivots { pivot: p, r1, r2, s1, s2, from: d.time, reading })
}

// ---------- Support / resistance ----------

/// Levels (price, touches) from the swing highs and lows of the candles up to `until` (included, within
/// `LEVEL_BARS`): swings closer than `tolerance` to a level's running mean join it; only levels touched twice or more.
pub fn sr_levels(c: &[Candle], until: usize, tolerance: f64) -> Vec<(f64, usize)> {
    if until >= c.len() || until < 2 * SWING_K {
        return vec![];
    }
    let w = &c[(until + 1).saturating_sub(LEVEL_BARS)..=until];
    let highs: Vec<f64> = w.iter().map(|x| x.high).collect();
    let lows: Vec<f64> = w.iter().map(|x| x.low).collect();
    let last = w.len() as i64 - 1;
    let mut prices: Vec<f64> = pivots(&highs, true, SWING_K, last).into_iter().map(|i| highs[i]).collect();
    prices.extend(pivots(&lows, false, SWING_K, last).into_iter().map(|i| lows[i]));
    prices.sort_by(|a, b| a.total_cmp(b));
    let mut out: Vec<(f64, usize)> = vec![];
    let mut sum = 0.0;
    for p in prices {
        match out.last_mut() {
            Some((mean, n)) if p - *mean <= tolerance => {
                sum += p;
                *n += 1;
                *mean = sum / *n as f64;
            }
            _ => {
                sum = p;
                out.push((p, 1));
            }
        }
    }
    out.retain(|(_, n)| *n >= MIN_TOUCHES);
    out
}

fn to_level(price: f64, touches: usize, ref_price: f64) -> SrLevel {
    SrLevel {
        price,
        touches,
        kind: if price <= ref_price { LevelKind::Support } else { LevelKind::Resistance },
        distance_pct: r2((price / ref_price - 1.0) * 100.0),
    }
}

// ---------- Breakout / fake breakout ----------

/// On the last `FAKE_BARS` candles, against the nearest levels known before them (swings confirmed before the
/// window): a close beyond with a volume ≥ 1.5 × the 20-candle average is a confirmed breakout, without it an
/// unconfirmed one; a break (intraday or on a close) followed by a last close back inside is a fake breakout.
pub fn breakout(c: &[Candle], tolerance: f64) -> Option<Breakout> {
    let n = c.len();
    if n < 60 {
        return None;
    }
    let before = n - 1 - FAKE_BARS;
    let reference = c[before].close;
    let levels = sr_levels(c, before - SWING_K, tolerance);
    let res = levels.iter().map(|l| l.0).filter(|p| *p > reference).fold(f64::INFINITY, f64::min);
    let sup = levels.iter().map(|l| l.0).filter(|p| *p < reference).fold(f64::NEG_INFINITY, f64::max);
    let last = &c[n - 1];
    let avg = c[n - 21..n - 1].iter().map(|x| x.volume).sum::<f64>() / 20.0;
    let ratio = if avg > 0.0 && last.volume > 0.0 { Some(r2(last.volume / avg)) } else { None };
    let window = &c[before + 1..];
    let make =
        |kind, side, level: f64, bias, reading: String| Breakout { kind, side: Some(side), level: Some(level), volume_ratio: ratio, bias, reading };
    let vol_text = ratio.map(|r| format!("volume {} × la moyenne", fr(r, 0, 1))).unwrap_or_else(|| "volume inconnu".into());
    let strong = ratio.is_some_and(|r| r >= BREAKOUT_VOLUME);
    let threshold = fr(BREAKOUT_VOLUME, 1, 1);
    if res.is_finite() {
        if last.close > res {
            return Some(if strong {
                make(BreakoutKind::Confirmed, Side::Up, res, Bias::Bullish, format!("Cassure confirmée de la résistance {} ({vol_text})", usd(res)))
            } else {
                let text =
                    format!("Clôture au-dessus de la résistance {} sans volume ({vol_text}, seuil {threshold} ×) : cassure non confirmée", usd(res));
                make(BreakoutKind::Unconfirmed, Side::Up, res, Bias::Neutral, text)
            });
        }
        if window.iter().any(|x| x.high > res) {
            let text = format!("Fausse cassure : la résistance {} a été dépassée puis le prix a refermé en dessous", usd(res));
            return Some(make(BreakoutKind::Fake, Side::Up, res, Bias::Bearish, text));
        }
    }
    if sup.is_finite() {
        if last.close < sup {
            return Some(if strong {
                make(BreakoutKind::Confirmed, Side::Down, sup, Bias::Bearish, format!("Cassure confirmée du support {} ({vol_text})", usd(sup)))
            } else {
                let text = format!("Clôture sous le support {} sans volume ({vol_text}, seuil {threshold} ×) : cassure non confirmée", usd(sup));
                make(BreakoutKind::Unconfirmed, Side::Down, sup, Bias::Neutral, text)
            });
        }
        if window.iter().any(|x| x.low < sup) {
            let text = format!("Fausse cassure baissière : le support {} a été enfoncé puis le prix a refermé au-dessus", usd(sup));
            return Some(make(BreakoutKind::Fake, Side::Down, sup, Bias::Bullish, text));
        }
    }
    Some(Breakout {
        kind: BreakoutKind::None,
        side: None,
        level: None,
        volume_ratio: ratio,
        bias: Bias::Neutral,
        reading: "Aucune cassure ni fausse cassure sur les 3 dernières bougies".into(),
    })
}

// ---------- Swing structure ----------

/// Last two swing highs and lows: higher highs and higher lows (up), lower highs and lower lows (down), else mixed.
pub fn market_structure(c: &[Candle]) -> Option<MarketStructure> {
    let w = &c[c.len().saturating_sub(LEVEL_BARS)..];
    let highs: Vec<f64> = w.iter().map(|x| x.high).collect();
    let lows: Vec<f64> = w.iter().map(|x| x.low).collect();
    let last = w.len() as i64 - 1;
    let ph = pivots(&highs, true, SWING_K, last);
    let pl = pivots(&lows, false, SWING_K, last);
    if ph.len() < 2 || pl.len() < 2 {
        return None;
    }
    let (h1, h2) = (highs[ph[ph.len() - 2]], highs[ph[ph.len() - 1]]);
    let (l1, l2) = (lows[pl[pl.len() - 2]], lows[pl[pl.len() - 1]]);
    let (trend, bias, text) = if h2 > h1 && l2 > l1 {
        (SwingTrend::Up, Bias::Bullish, "Sommets et creux ascendants : structure haussière")
    } else if h2 < h1 && l2 < l1 {
        (SwingTrend::Down, Bias::Bearish, "Sommets et creux descendants : structure baissière")
    } else {
        (SwingTrend::Mixed, Bias::Neutral, "Sommets et creux sans progression commune : structure sans direction")
    };
    Some(MarketStructure {
        trend,
        highs: vec![h1, h2],
        lows: vec![l1, l2],
        bias,
        reading: format!("{text} (sommets {} puis {}, creux {} puis {})", usd(h1), usd(h2), usd(l1), usd(l2)),
    })
}

// ---------- Relative strength ----------

pub const RS_PERIODS: [(&str, i64); 3] = [("1 mois", 30), ("3 mois", 91), ("6 mois", 182)];

/// Close of the last candle at or before `t` (candles sorted by time).
fn close_at(c: &[Candle], t: i64) -> Option<f64> {
    let i = c.partition_point(|x| x.time <= t);
    if i == 0 { None } else { Some(c[i - 1].close) }
}

/// Asset return − benchmark return over 1, 3 and 6 months (calendar days back from the asset's last candle), and
/// the correlation of the daily returns over 90 days.
pub fn relative_strength(asset: &[Candle], b: &Benchmark) -> Option<RelativeStrength> {
    let end = asset.last()?.time;
    let mut periods = vec![];
    for (label, days) in RS_PERIODS {
        let start = end - days * DAY_MS;
        let (Some(a0), Some(a1), Some(b0), Some(b1)) =
            (close_at(asset, start), close_at(asset, end), close_at(&b.daily, start), close_at(&b.daily, end))
        else {
            continue;
        };
        if a0 <= 0.0 || b0 <= 0.0 {
            continue;
        }
        let (ra, rb) = ((a1 / a0 - 1.0) * 100.0, (b1 / b0 - 1.0) * 100.0);
        periods.push(RsPeriod { label: label.into(), days, asset_pct: r2(ra), benchmark_pct: r2(rb), diff: r2(ra - rb) });
    }
    // Direction from the 3-month gap (else the longest one measured).
    let main = periods.iter().find(|p| p.days == 91).or(periods.last())?.diff;
    let r = aligned_returns(&[asset, &b.daily], 90);
    let corr = correlation(&r[0], &r[1]).map(r2);
    let (bias, word) = if main >= 5.0 {
        (Bias::Bullish, "Plus fort que")
    } else if main <= -5.0 {
        (Bias::Bearish, "Plus faible que")
    } else {
        (Bias::Neutral, "En ligne avec")
    };
    let detail = periods.iter().map(|p| format!("{} {}", p.label, pts(p.diff))).collect::<Vec<_>>().join(", ");
    let corr_text = corr.map(|c| format!(" ; corrélation sur 90 jours {}", fr(c, 2, 2))).unwrap_or_default();
    Some(RelativeStrength {
        benchmark: b.name.clone(),
        symbol: b.symbol.clone(),
        reading: format!("{word} le {} ({detail}){corr_text}", b.name),
        periods,
        correlation: corr,
        bias,
    })
}

// ---------- Everything ----------

fn bias_value(b: Bias) -> f64 {
    match b {
        Bias::Bullish => 1.0,
        Bias::Neutral => 0.0,
        Bias::Bearish => -1.0,
    }
}

/// Technical structure on closed daily candles at `price` (the current price), against the benchmarks (an asset
/// is never compared with itself).
pub fn structure(daily: &[Candle], price: f64, symbol: &str, kind: Kind, benchmarks: &[Benchmark]) -> Structure {
    let c = daily;
    let n = c.len();
    let price = if price > 0.0 { price } else { c.last().map_or(0.0, |x| x.close) };
    let a = atr(c, 14).last().copied().flatten();
    let tolerance = a.unwrap_or(0.0) * LEVEL_ATR;
    let ichimoku = ichimoku(c, price);
    let supertrend = supertrend(c, SUPERTREND_ATR, SUPERTREND_MULT);
    let donchian = donchian(c, DONCHIAN);
    let vwap = vwap(c, VWAP_BARS, price);
    let volume_profile = volume_profile(c, PROFILE_BARS, PROFILE_BINS, price);
    let pivots = floor_pivots(c, price);
    let raw = if a.is_some() && price > 0.0 { sr_levels(c, n - 1, tolerance) } else { vec![] };
    let mut levels: Vec<SrLevel> = raw.iter().map(|(p, t)| to_level(*p, *t, price)).collect();
    levels.sort_by(|x, y| x.distance_pct.abs().total_cmp(&y.distance_pct.abs()));
    let nearest_support = levels.iter().find(|l| l.kind == LevelKind::Support).cloned();
    let nearest_resistance = levels.iter().find(|l| l.kind == LevelKind::Resistance).cloned();
    let parts: Vec<String> = [
        nearest_support.as_ref().map(|s| format!("support le plus proche {} ({} contacts, {})", usd(s.price), s.touches, signed(s.distance_pct))),
        nearest_resistance
            .as_ref()
            .map(|r| format!("résistance la plus proche {} ({} contacts, {})", usd(r.price), r.touches, signed(r.distance_pct))),
    ]
    .into_iter()
    .flatten()
    .collect();
    let levels_reading =
        if parts.is_empty() { "Aucun niveau touché au moins deux fois sur l'historique analysé".to_string() } else { cap(&parts.join(" ; ")) };
    let breakout = if a.is_some() { breakout(c, tolerance) } else { None };
    let market_structure = market_structure(c);
    let mut relative = vec![];
    let mut relative_note = None;
    for b in benchmarks {
        if b.symbol == symbol && b.kind == kind {
            relative_note = Some(format!("{symbol} est lui-même l'indice de référence : pas de force relative calculée"));
            continue;
        }
        relative.extend(relative_strength(c, b));
    }
    if relative.is_empty() && relative_note.is_none() {
        relative_note = Some(if benchmarks.is_empty() {
            "Indice de référence indisponible (bougies non chargées)".into()
        } else {
            "Historique trop court pour comparer à l'indice de référence".into()
        });
    }
    // Direction of the whole: each item counts once (a fake breakout half, against the side it failed on).
    let mut comps: Vec<f64> = vec![];
    comps.extend(ichimoku.as_ref().map(|x| bias_value(x.bias)));
    comps.extend(supertrend.as_ref().map(|x| bias_value(x.bias)));
    comps.extend(donchian.as_ref().map(|x| bias_value(x.bias)));
    comps.extend(market_structure.as_ref().map(|x| bias_value(x.bias)));
    comps.extend(breakout.as_ref().map(|x| if x.kind == BreakoutKind::Fake { 0.5 } else { 1.0 } * bias_value(x.bias)));
    if let Some(p) = relative.first().and_then(|r| r.periods.iter().find(|p| p.days == 91).or(r.periods.last())) {
        comps.push(clamp(p.diff / 20.0, -1.0, 1.0));
    }
    let score = if comps.is_empty() { None } else { Some(round(comps.iter().sum::<f64>() / comps.len() as f64 * 100.0)) };
    Structure {
        timeframe: "Bougies journalières clôturées".into(),
        score,
        ichimoku,
        supertrend,
        donchian,
        vwap,
        volume_profile,
        pivots,
        levels,
        nearest_support,
        nearest_resistance,
        levels_reading,
        breakout,
        market_structure,
        relative,
        relative_note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(time: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Candle {
        Candle { time, open, high, low, close, volume }
    }

    /// Daily candles with high = close + 1 and low = close − 1.
    fn from_closes(closes: &[f64]) -> Vec<Candle> {
        closes.iter().enumerate().map(|(i, &c)| k(i as i64 * DAY_MS, c, c + 1.0, c - 1.0, c, 100.0)).collect()
    }

    #[test]
    fn ichimoku_by_hand() {
        // 78 candles rising by 1: close i = 100 + i.
        let c = from_closes(&(0..78).map(|i| 100.0 + i as f64).collect::<Vec<_>>());
        let x = ichimoku(&c, 177.0).unwrap();
        // Tenkan = (highest high of 69..77 = 178 + lowest low = 168) / 2.
        assert_eq!(x.tenkan, 173.0);
        // Kijun = (178 + lowest low of 52..77 = 151) / 2.
        assert_eq!(x.kijun, 164.5);
        // Cloud under the last candle, from candle 51: Tenkan (152 + 142) / 2 = 147, Kijun (152 + 125) / 2 = 138.5,
        // A = 142.75; B = (152 + 99) / 2 = 125.5.
        assert_eq!(x.senkou_a, 142.75);
        assert_eq!(x.senkou_b, 125.5);
        assert_eq!(x.future_a, (173.0 + 164.5) / 2.0);
        assert_eq!(x.position, CloudPosition::Above);
        assert_eq!(x.bias, Bias::Bullish);
        assert!(x.reading.starts_with("Prix au-dessus du nuage Ichimoku, Tenkan au-dessus de la Kijun"), "{}", x.reading);
        assert!(ichimoku(&c[..77], 176.0).is_none());
        assert_eq!(ichimoku(&c, 120.0).unwrap().position, CloudPosition::Below);
        assert_eq!(ichimoku(&c, 130.0).unwrap().position, CloudPosition::Inside);
    }

    #[test]
    fn tk_cross_is_dated() {
        // 70 candles falling to 131, then 12 rising by 6. Both lines share the low (candle 69) until the Tenkan's
        // 9-candle window leaves it at candle 78: Tenkan (186 + 136) / 2 = 161 > Kijun (186 + 130) / 2 = 158.
        let mut closes: Vec<f64> = (0..70).map(|i| 200.0 - i as f64).collect();
        closes.extend((1..=12).map(|i| 131.0 + 6.0 * i as f64));
        let c = from_closes(&closes);
        assert_eq!((mid(&c, 78, 9), mid(&c, 78, 26)), (161.0, 158.0));
        assert_eq!(mid(&c, 77, 9), mid(&c, 77, 26));
        let x = ichimoku(&c, 203.0).unwrap();
        assert_eq!((x.tk_cross, x.tk_cross_bars), (Some(Side::Up), Some(3)));
        assert!(x.reading.contains("croisement haussier il y a 3 bougies"), "{}", x.reading);
    }

    #[test]
    fn supertrend_by_hand() {
        // Flat candles (high 101, low 99, close 100): ATR 2, bands 100 ± 6, up (close ≥ middle), never flipped.
        let mut c: Vec<Candle> = (0..12).map(|i| k(i * DAY_MS, 100.0, 101.0, 99.0, 100.0, 1.0)).collect();
        let s = supertrend(&c, 10, 3.0).unwrap();
        assert_eq!((s.direction, s.bars), (Side::Up, 3));
        assert!((s.level - 94.0).abs() < 1e-9, "{}", s.level);
        assert!(s.reading.starts_with("Supertrend haussier depuis au moins 3 bougies"), "{}", s.reading);
        // A candle 100 → 90 (low 89): true range 11, ATR (2 × 9 + 11) / 10 = 2.9; close under the 94 floor → down.
        // Upper band: middle 94.5 + 3 × 2.9 = 103.2, tighter than 106.
        c.push(k(12 * DAY_MS, 100.0, 100.0, 89.0, 90.0, 1.0));
        let s = supertrend(&c, 10, 3.0).unwrap();
        assert_eq!((s.direction, s.bars), (Side::Down, 1));
        assert!((s.level - 103.2).abs() < 1e-9, "{}", s.level);
        assert!(s.reading.starts_with("Supertrend baissier depuis 1 bougie ("), "{}", s.reading);
    }

    #[test]
    fn donchian_and_vwap_by_hand() {
        let mut c = from_closes(&(0..21).map(|i| 100.0 + (i % 5) as f64).collect::<Vec<_>>());
        // Candles 0..19: closes 100…104, highs up to 105, lows down to 99.
        let d = donchian(&c, 20).unwrap();
        assert_eq!((d.upper, d.lower, d.mid, d.breakout), (105.0, 99.0, 102.0, None));
        c.push(k(21 * DAY_MS, 104.0, 107.0, 103.0, 106.0, 100.0));
        assert_eq!(donchian(&c, 20).unwrap().breakout, Some(Side::Up));
        // Typical prices 10 (volume 1) and 20 (volume 3): (10 + 60) / 4 = 17.5.
        let v = [k(0, 10.0, 11.0, 9.0, 10.0, 1.0), k(DAY_MS, 20.0, 21.0, 19.0, 20.0, 3.0)];
        let x = vwap(&v, 2, 17.5 * 1.1).unwrap();
        assert!((x.value - 17.5).abs() < 1e-12);
        assert_eq!((x.deviation_pct, x.bias), (10.0, Bias::Bullish));
        assert!(x.reading.contains("glissant") && x.reading.contains("pas ancré"), "{}", x.reading);
        assert!(vwap(&[k(0, 1.0, 1.0, 1.0, 1.0, 0.0)], 1, 1.0).is_none(), "no volume, no VWAP");
    }

    #[test]
    fn volume_profile_by_hand() {
        // 20 candles 100–110 with volume 10, one candle 110–120 with volume 100: range 100–120, 4 bins of 5.
        let mut c: Vec<Candle> = (0..20).map(|i| k(i * DAY_MS, 105.0, 110.0, 100.0, 105.0, 10.0)).collect();
        c.push(k(20 * DAY_MS, 115.0, 120.0, 110.0, 115.0, 100.0));
        let p = volume_profile(&c, 120, 4, 112.0).unwrap();
        // Bins 100, 100, 50, 50: the POC is the first of the heaviest.
        assert_eq!(p.poc, 102.5);
        // 70 % of 300 = 210: 100, + 100 above = 200, + 50 above = 250 → 100 – 115.
        assert_eq!((p.value_area_low, p.value_area_high), (100.0, 115.0));
        assert_eq!(p.bias, Bias::Neutral);
        assert!(p.note.starts_with("Approximation à partir des bougies"));
    }

    #[test]
    fn floor_pivots_by_hand() {
        let p = floor_pivots(&[k(0, 100.0, 110.0, 90.0, 105.0, 1.0)], 104.0).unwrap();
        // P = (110 + 90 + 105) / 3; R1 = 2P − 90; S1 = 2P − 110; R2 = P + 20; S2 = P − 20.
        let pv = 305.0 / 3.0;
        assert!((p.pivot - pv).abs() < 1e-12);
        assert!((p.r1 - (2.0 * pv - 90.0)).abs() < 1e-12 && (p.s1 - (2.0 * pv - 110.0)).abs() < 1e-12);
        assert!((p.r2 - (pv + 20.0)).abs() < 1e-12 && (p.s2 - (pv - 20.0)).abs() < 1e-12);
        assert!(p.reading.starts_with("Prix entre le pivot"), "{}", p.reading);
    }

    /// Zigzag 100 → 120 → 100 (period 8): swing highs at 121, swing lows at 99.
    fn zigzag(n: usize) -> Vec<Candle> {
        let path = [100.0, 105.0, 110.0, 115.0, 120.0, 115.0, 110.0, 105.0];
        from_closes(&(0..n).map(|i| path[i % 8]).collect::<Vec<_>>())
    }

    #[test]
    fn levels_cluster_the_swings() {
        // 64 candles: lows at 8, 16, …, 56 (7) and highs at 4, 12, …, 60 (8; 60 still has 3 candles after it).
        let c = zigzag(64);
        assert_eq!(sr_levels(&c, 63, 1.0), vec![(99.0, 7), (121.0, 8)]);
        let s = structure(&c, 110.0, "X", Kind::Crypto, &[]);
        assert_eq!(s.nearest_support.as_ref().map(|l| (l.price, l.touches)), Some((99.0, 7)));
        assert_eq!(s.nearest_resistance.as_ref().map(|l| (l.price, l.kind)), Some((121.0, LevelKind::Resistance)));
        assert!(s.levels_reading.starts_with("Support le plus proche 99,00 $ (7 contacts"), "{}", s.levels_reading);
        assert_eq!(s.relative_note.as_deref(), Some("Indice de référence indisponible (bougies non chargées)"));
    }

    #[test]
    fn breakouts_and_fakes() {
        // Zigzag, then a rise through 121 on 3 × the volume: confirmed; on the usual volume: unconfirmed.
        let mut c = zigzag(64);
        let t = |i: i64| i * DAY_MS;
        c.push(k(t(64), 100.0, 106.0, 99.0, 105.0, 100.0));
        c.push(k(t(65), 105.0, 116.0, 104.0, 115.0, 100.0));
        c.push(k(t(66), 115.0, 126.0, 114.0, 125.0, 300.0));
        let b = breakout(&c, 1.0).unwrap();
        assert_eq!((b.kind, b.side, b.level, b.volume_ratio), (BreakoutKind::Confirmed, Some(Side::Up), Some(121.0), Some(3.0)), "{b:?}");
        c[66].volume = 100.0;
        assert_eq!(breakout(&c, 1.0).unwrap().kind, BreakoutKind::Unconfirmed);
        // Above 121 intraday, close back at 118: fake breakout.
        c[66] = k(t(66), 115.0, 124.0, 114.0, 118.0, 300.0);
        let b = breakout(&c, 1.0).unwrap();
        assert_eq!((b.kind, b.bias), (BreakoutKind::Fake, Bias::Bearish), "{b:?}");
        assert!(b.reading.starts_with("Fausse cassure"));
        // Far from both levels: none.
        c[66] = k(t(66), 115.0, 116.0, 110.0, 112.0, 100.0);
        assert_eq!(breakout(&c, 1.0).unwrap().kind, BreakoutKind::None);
    }

    #[test]
    fn swing_structure() {
        // Zigzag drifting up by 2 per cycle: higher highs and higher lows; reversed in time: lower and lower.
        let path = [100.0, 105.0, 110.0, 115.0, 120.0, 115.0, 110.0, 105.0];
        let closes: Vec<f64> = (0..48).map(|i| path[i % 8] + 2.0 * (i / 8) as f64).collect();
        assert_eq!(market_structure(&from_closes(&closes)).unwrap().trend, SwingTrend::Up);
        let rev: Vec<f64> = closes.iter().rev().copied().collect();
        assert_eq!(market_structure(&from_closes(&rev)).unwrap().trend, SwingTrend::Down);
        assert_eq!(market_structure(&zigzag(48)).unwrap().trend, SwingTrend::Mixed);
    }

    #[test]
    fn relative_strength_by_hand() {
        // 200 days: the asset doubles linearly, the benchmark gains 10 % linearly.
        let line = |gain: f64| from_closes(&(0..200).map(|i| 100.0 * (1.0 + gain * i as f64 / 199.0)).collect::<Vec<_>>());
        let b = Benchmark { name: "S&P 500 (SPY)".into(), symbol: "SPY".into(), kind: Kind::Stock, daily: line(0.1) };
        let r = relative_strength(&line(1.0), &b).unwrap();
        assert_eq!(r.periods.len(), 3);
        // 1 month: closes of days 199 and 169.
        let a1 = (200.0 / (100.0 * (1.0 + 169.0 / 199.0)) - 1.0) * 100.0;
        let b1 = (110.0 / (100.0 * (1.0 + 0.1 * 169.0 / 199.0)) - 1.0) * 100.0;
        assert_eq!((r.periods[0].asset_pct, r.periods[0].benchmark_pct, r.periods[0].diff), (r2(a1), r2(b1), r2(a1 - b1)));
        assert_eq!(r.bias, Bias::Bullish);
        assert!(r.reading.starts_with("Plus fort que le S&P 500 (SPY) (1 mois +"), "{}", r.reading);
        // Never compared with itself.
        let s = structure(&b.daily, 110.0, "SPY", Kind::Stock, std::slice::from_ref(&b));
        assert!(s.relative.is_empty() && s.relative_note.unwrap().contains("référence"));
    }
}
