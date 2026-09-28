//! Market guard (`web/src/engine/guard.ts`): what a short-term bot cannot see on its own.
//!
//! 1. Regime   — the background trend (daily + 4 h), for the long-term bot.
//! 2. Shock    — abnormal volatility, price jumps, volume spikes, compression before a breakout, news bursts,
//!    market-wide fear (VIX): when to reduce or pause short-term trading.
//! 3. Reversal — a move AGAINST the current trend: technical exhaustion, crowd positioning, sentiment extremes and
//!    news tone.
//!
//! Every technical score is calibrated on the asset's own history (how often it was followed by an adverse move).
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::Evidence;
use super::macro_ctx::{MACRO, MacroReport};
use super::signal::{Candle, Series, adx, atr, ema, rsi, sanitize};
use crate::js::{fr, number_to_string, round};
use crate::types::{Interval, Kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Trend {
    Up,
    Down,
    Range,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShockLevel {
    Calm,
    Agitated,
    Shock,
}

/// Direction of a reversal ("down": reversing an up move).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Down,
    Up,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Positioning {
    /// Current perpetual funding rate per 8 h, as a fraction (0.0001 = 0.01 %).
    #[serde(default)]
    pub funding_rate: Option<f64>,
    /// Long/short account ratio, oldest first.
    #[serde(default)]
    pub long_short_ratio: Vec<f64>,
    /// Open interest (USD), hourly, oldest first.
    #[serde(default)]
    pub open_interest: Vec<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentimentInput {
    /// Fear & Greed index, oldest first.
    #[serde(default)]
    pub fear_greed: Vec<f64>,
    /// Share of bullish messages (StockTwits), 0-100.
    #[serde(default)]
    pub social_bullish: Option<f64>,
    #[serde(default)]
    pub social_sample: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewsItem {
    pub title: String,
    pub time: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Macro / geopolitical context (market-wide) and what its stress announced on this asset's history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroContext {
    pub report: MacroReport,
    pub evidence: Option<Evidence>,
}

/// `GuardInput`; `now` is explicit (the TypeScript defaults to `Date.now()`).
#[derive(Debug, Clone, Copy)]
pub struct GuardInput<'a> {
    pub kind: Kind,
    pub daily: &'a [Candle],
    pub h4: &'a [Candle],
    pub h1: &'a [Candle],
    pub positioning: Option<&'a Positioning>,
    pub sentiment: Option<&'a SentimentInput>,
    pub news: Option<&'a [NewsItem]>,
    /// VIX daily closes, oldest first (stocks). Ignored when `macro_ctx` is given.
    pub vix: Option<&'a [f64]>,
    pub macro_ctx: Option<&'a MacroContext>,
    pub now: i64,
}

/// verified: worked on this asset · unproven: too few past cases (half) · rejected: never predictive here (not
/// counted) · unverifiable: no history available (news, positioning…).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FactorStatus {
    Verified,
    Unproven,
    Rejected,
    Unverifiable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardFactor {
    pub code: String,
    pub base_points: f64,
    /// Points actually counted (base points × weight from the evidence).
    pub points: f64,
    pub text: String,
    pub status: FactorStatus,
    pub evidence: Option<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Regime {
    pub trend: Trend,
    pub strength: f64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shock {
    pub score: f64,
    pub level: ShockLevel,
    pub factors: Vec<GuardFactor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reversal {
    pub score: f64,
    pub direction: Option<Direction>,
    pub factors: Vec<GuardFactor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scalping {
    Ok,
    Reduce,
    Pause,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub scalping: Scalping,
    pub size_multiplier: f64,
    pub stop_multiplier: f64,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GuardResult {
    pub regime: Regime,
    pub shock: Shock,
    pub reversal: Reversal,
    pub policy: Policy,
}

// ---------- Thresholds ----------

pub struct GuardThresholds {
    pub shock_agitated: f64,
    pub shock_level: f64,
    pub reversal_high: f64,
    /// Minimum past occurrences for a factor's evidence to be trusted.
    pub min_samples: f64,
    /// Weight of a factor with no history available.
    pub unverifiable_weight: f64,
    pub funding_hot: f64,
    pub funding_very_hot: f64,
    pub funding_cold: f64,
    pub funding_very_cold: f64,
}

pub const GUARD: GuardThresholds = GuardThresholds {
    shock_agitated: 35.0,
    shock_level: 65.0,
    reversal_high: 50.0,
    min_samples: 20.0,
    unverifiable_weight: 0.75,
    funding_hot: 0.0003,
    funding_very_hot: 0.0006,
    funding_cold: -0.0001,
    funding_very_cold: -0.0003,
};

fn last_value(s: &[Option<f64>]) -> Option<f64> {
    s.iter().rev().find_map(|v| *v)
}
fn mean(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().fold(0.0, |a, b| a + b) / v.len() as f64 }
}
fn std(v: &[f64]) -> f64 {
    if v.len() < 2 {
        return 0.0;
    }
    let m = mean(v);
    (v.iter().fold(0.0, |a, b| a + (b - m) * (b - m)) / (v.len() - 1) as f64).sqrt()
}
fn pct(v: f64) -> String {
    format!("{} %", fr(v, 0, 1))
}
fn one(v: f64) -> String {
    fr(v, 0, 1)
}
/// `Math.max(...v)` (−∞ when empty, NaN if any NaN).
fn js_max(v: impl IntoIterator<Item = f64>) -> f64 {
    let mut m = f64::NEG_INFINITY;
    for x in v {
        if x.is_nan() {
            return f64::NAN;
        }
        if x > m {
            m = x;
        }
    }
    m
}
fn js_min(v: impl IntoIterator<Item = f64>) -> f64 {
    let mut m = f64::INFINITY;
    for x in v {
        if x.is_nan() {
            return f64::NAN;
        }
        if x < m {
            m = x;
        }
    }
    m
}
/// `a.slice(start, end)` with non-negative bounds (clamped to the length).
fn slice<T>(a: &[T], start: i64, end: i64) -> &[T] {
    let n = a.len() as i64;
    let s = start.clamp(0, n) as usize;
    let e = end.clamp(0, n) as usize;
    if s >= e { &a[..0] } else { &a[s..e] }
}
fn get(s: &Series, i: i64) -> Option<f64> {
    if i < 0 { None } else { s.get(i as usize).copied().flatten() }
}
/// JavaScript truthiness of a number that may be missing.
fn truthy(v: Option<f64>) -> Option<f64> {
    v.filter(|x| *x != 0.0 && !x.is_nan())
}

/// Percentile rank (0-100) of the last value among all values.
pub fn percentile_rank(values: &[f64], x: f64) -> f64 {
    if values.is_empty() {
        return 50.0;
    }
    let below = values.iter().filter(|v| **v < x).count() as f64;
    let equal = values.iter().filter(|v| **v == x).count() as f64;
    (below + 0.5 * equal) / values.len() as f64 * 100.0
}

// ---------- 1. Regime ----------

pub fn regime(daily_raw: &[Candle], h4_raw: &[Candle]) -> Regime {
    let d = sanitize(daily_raw);
    let closes: Vec<f64> = d.iter().map(|c| c.close).collect();
    if closes.len() < 60 {
        return Regime { trend: Trend::Range, strength: 0.0, text: "Historique journalier insuffisant pour juger la tendance de fond.".into() };
    }
    let long = if closes.len() >= 200 { 200 } else { 100 };
    let e_l = ema(&closes, long);
    let e50 = ema(&closes, 50);
    let close = *closes.last().unwrap();
    let l = last_value(&e_l).or(last_value(&e50)).unwrap_or(f64::NAN);
    let m = last_value(&e50).unwrap_or(f64::NAN);
    let m10 = e50.get(e50.len().wrapping_sub(11)).copied().flatten().unwrap_or(m);
    let slope = if truthy(Some(m10)).is_some() { ((m - m10) / m10) * 100.0 } else { 0.0 };
    let a = last_value(&adx(&d, 14)).unwrap_or(0.0);
    let mut trend = Trend::Range;
    if close > l && m > l && slope > 0.0 {
        trend = Trend::Up;
    } else if close < l && m < l && slope < 0.0 {
        trend = Trend::Down;
    }
    // 4 h confirmation: EMA 20 vs EMA 50 in the same direction.
    let h4: Vec<f64> = sanitize(h4_raw).iter().map(|c| c.close).collect();
    let f20 = last_value(&ema(&h4, 20));
    let f50 = last_value(&ema(&h4, 50));
    let aligned = match (f20, f50) {
        (Some(f20), Some(f50)) => (trend == Trend::Up && f20 > f50) || (trend == Trend::Down && f20 < f50),
        _ => false,
    };
    let strength =
        if trend == Trend::Range { round(a.min(40.0)) } else { round(100f64.min(a * 2.5 + if aligned { 10.0 } else { 0.0 })) };
    let text = if trend == Trend::Range {
        format!(
            "Pas de tendance de fond nette (prix {} de la moyenne {long} jours, ADX {}) : marché sans direction.",
            if close > l { "au-dessus" } else { "en dessous" },
            number_to_string(round(a))
        )
    } else {
        format!(
            "Tendance de fond {} (moyennes 50 et {long} jours alignées, ADX {}){}.",
            if trend == Trend::Up { "haussière" } else { "baissière" },
            number_to_string(round(a)),
            if aligned { ", confirmée en 4 h" } else { ", pas encore confirmée en 4 h" }
        )
    };
    Regime { trend, strength, text }
}

// ---------- News tone ----------

const NEGATIVE: &[&str] = &[
    "hack", "hacked", "exploit", "breach", "stolen", "lawsuit", "sues", "sued", "sec charges", "investigation", "probe", "fraud",
    "bankrupt", "bankruptcy", "insolvency", "liquidation", "liquidated", "delist", "ban", "banned", "crackdown", "crash", "plunge",
    "plunges", "tumble", "tumbles", "sell-off", "selloff", "downgrade", "downgraded", "misses", "miss estimates", "cuts guidance",
    "layoffs", "recall", "halt", "halted", "outage", "default", "warning", "subpoena", "indictment", "sanction",
    "piratage", "faillite", "enquête", "plainte", "effondrement", "chute", "interdiction", "fraude",
];
const POSITIVE: &[&str] = &[
    "approval", "approved", "approves", "etf inflows", "record high", "all-time high", "surge", "surges", "soars", "rally",
    "rallies", "upgrade", "upgraded", "beats", "beat estimates", "raises guidance", "buyback", "partnership", "adoption",
    "launch", "launches", "breakthrough", "acquisition", "wins",
    "approbation", "hausse", "partenariat", "rachat",
];

fn word_regexes(words: &[&str]) -> Vec<Regex> {
    words.iter().map(|w| Regex::new(&format!("[^a-zà-ü]{}[^a-zà-ü]", regex::escape(w))).expect("regex")).collect()
}
static NEGATIVE_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| word_regexes(NEGATIVE));
static POSITIVE_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| word_regexes(POSITIVE));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsTone {
    pub negative: usize,
    pub positive: usize,
}

/// Headline tone: count of negative and positive headlines (a headline counts once).
pub fn news_tone<'a>(items: impl IntoIterator<Item = &'a NewsItem>) -> NewsTone {
    let (mut negative, mut positive) = (0, 0);
    for it in items {
        let t = format!(" {} ", it.title.to_lowercase());
        let neg = NEGATIVE_RE.iter().any(|r| r.is_match(&t));
        let pos = POSITIVE_RE.iter().any(|r| r.is_match(&t));
        if neg && !pos {
            negative += 1;
        } else if pos && !neg {
            positive += 1;
        }
    }
    NewsTone { negative, positive }
}

// ---------- Evidence (self-validation on the asset's own history) ----------

/// Weight of a factor from its evidence.
pub fn weigh(e: Option<&Evidence>, historical: bool) -> (f64, FactorStatus) {
    if !historical {
        return (GUARD.unverifiable_weight, FactorStatus::Unverifiable);
    }
    match e {
        Some(e) if e.samples >= GUARD.min_samples => {
            if e.lift < 1.1 {
                (0.0, FactorStatus::Rejected)
            } else {
                (1f64.min(0.2f64.max((e.lift - 1.0) / 0.5)), FactorStatus::Verified)
            }
        }
        _ => (0.5, FactorStatus::Unproven),
    }
}

/// A factor before weighting (`Raw` in guard.ts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawFactor {
    pub code: String,
    pub points: f64,
    pub text: String,
}

fn raw(code: impl Into<String>, points: f64, text: String) -> RawFactor {
    RawFactor { code: code.into(), points, text }
}

pub type EvidenceMap = IndexMap<String, Evidence>;

fn finalize(raw: Vec<RawFactor>, evidence: &EvidenceMap, historical: impl Fn(&str) -> bool) -> Vec<GuardFactor> {
    raw.into_iter()
        .map(|f| {
            let e = evidence.get(&f.code).copied();
            let (weight, status) = weigh(e.as_ref(), historical(&f.code));
            GuardFactor { points: round(f.points * weight), base_points: f.points, code: f.code, text: f.text, status, evidence: e }
        })
        .collect()
}

type Stats = IndexMap<String, (u32, u32)>;

fn tally(stats: &mut Stats, key: &str, hit: bool) {
    let s = stats.entry(key.to_string()).or_insert((0, 0));
    s.0 += 1;
    if hit {
        s.1 += 1;
    }
}

fn to_evidence(stats: &Stats) -> EvidenceMap {
    let mut out = EvidenceMap::new();
    let Some(&(bn, bh)) = stats.get("_base") else { return out };
    if bn == 0 {
        return out;
    }
    let b = (bh as f64 / bn as f64) * 100.0;
    for (k, &(n, hit)) in stats {
        if k == "_base" {
            continue;
        }
        let rate = (hit as f64 / n as f64) * 100.0;
        out.insert(k.clone(), Evidence { samples: n as f64, rate, base: b, lift: if b > 0.0 { rate / b } else { 0.0 } });
    }
    out
}

// ---------- 2. Shock ----------

/// Hourly factors at candle i (returns r[j] = log(close j+1 / close j)), no look-ahead.
pub fn hourly_shock_factors(h1: &[Candle], r: &[f64], i: usize) -> Vec<RawFactor> {
    let mut f = Vec::new();
    if i < 96 {
        return f;
    }
    let ii = i as i64;
    let base = slice(r, 0.max(ii - 120), ii - 24);
    let recent = slice(r, ii - 24, ii);
    let s_long = std(base);
    let s_now = std(recent);
    let ratio = if s_long > 0.0 { s_now / s_long } else { 1.0 };
    if ratio >= 2.0 {
        f.push(raw("vol2", 35.0, format!("Volatilité des dernières 24 h {} fois supérieure à la normale.", one(ratio))));
    } else if ratio >= 1.5 {
        f.push(raw("vol15", 20.0, format!("Volatilité des dernières 24 h {} fois supérieure à la normale.", one(ratio))));
    }
    let jump = if s_long > 0.0 { js_max(slice(r, ii - 6, ii).iter().map(|x| x.abs())) / s_long } else { 0.0 };
    if jump >= 4.0 {
        f.push(raw("jump4", 35.0, format!("Saut de prix de {} écarts-types en une heure (mouvement anormal).", one(jump))));
    } else if jump >= 3.0 {
        f.push(raw("jump3", 20.0, format!("Mouvement horaire de {} écarts-types, inhabituel.", one(jump))));
    }
    let vols: Vec<f64> = slice(h1, 0.max(ii - 100), ii - 2).iter().map(|c| c.volume).collect();
    let v_now = mean(&slice(h1, ii - 2, ii + 1).iter().map(|c| c.volume).collect::<Vec<_>>());
    let z = if std(&vols) > 0.0 { (v_now - mean(&vols)) / std(&vols) } else { 0.0 };
    if z >= 3.0 {
        f.push(raw("volume", 15.0, format!("Volume des 3 dernières heures très au-dessus de la normale ({} écarts-types).", one(z))));
    }
    f
}

/// Bollinger bandwidth (20, 2σ) of 4 h closes, relative to the mean.
fn bandwidth(closes: &[f64]) -> Series {
    (0..closes.len())
        .map(|i| {
            if i < 19 {
                return None;
            }
            let w = &closes[i - 19..=i];
            let m = mean(w);
            if m > 0.0 { Some((4.0 * std(w)) / m) } else { None }
        })
        .collect()
}

fn squeeze_factor(bw: &Series, i: usize) -> Vec<RawFactor> {
    let now = bw.get(i).copied().flatten();
    let hist: Vec<f64> = slice(bw, 0.max(i as i64 - 200), i as i64).iter().flatten().copied().collect();
    let Some(now) = now else { return vec![] };
    if hist.len() < 100 {
        return vec![];
    }
    let rank = percentile_rank(&hist, now);
    if rank <= 10.0 {
        vec![raw(
            "squeeze",
            15.0,
            format!(
                "Volatilité anormalement comprimée (bandes de Bollinger 4 h plus étroites que {} % du temps) : un mouvement brutal peut suivre, sans direction connue.",
                number_to_string(round(100.0 - rank))
            ),
        )]
    } else {
        vec![]
    }
}

fn log_returns(h1: &[Candle]) -> Vec<f64> {
    h1.windows(2).map(|w| (w[1].close / w[0].close).ln()).collect()
}

/// Evidence of the shock factors on the asset's history. Hourly factors: followed by a move of 3 standard
/// deviations within the next 6 hours? Squeeze: followed by a 6-candle (24 h) range of 4 ATR?
pub fn shock_evidence(h1_raw: &[Candle], h4_raw: &[Candle]) -> EvidenceMap {
    let mut out = EvidenceMap::new();
    let h1 = sanitize(h1_raw);
    let r = log_returns(&h1);
    let mut s1 = Stats::new();
    let mut i = 96;
    while i + 6 < r.len() {
        let s_long = std(slice(&r, 0.max(i as i64 - 120), i as i64 - 24));
        if s_long > 0.0 {
            let hit = js_max(r[i..i + 6].iter().map(|x| x.abs())) >= 3.0 * s_long;
            tally(&mut s1, "_base", hit);
            for f in hourly_shock_factors(&h1, &r, i) {
                tally(&mut s1, &f.code, hit);
            }
        }
        i += 1;
    }
    out.extend(to_evidence(&s1));
    let h4 = sanitize(h4_raw);
    let bw = bandwidth(&h4.iter().map(|c| c.close).collect::<Vec<_>>());
    let a = atr(&h4, 14);
    let mut s4 = Stats::new();
    let mut i = 120;
    while i + 6 < h4.len() {
        if let Some(at) = truthy(a[i]) {
            let next = &h4[i + 1..i + 7];
            let hit = js_max(next.iter().map(|c| c.high)) - js_min(next.iter().map(|c| c.low)) >= 4.0 * at;
            tally(&mut s4, "_base", hit);
            for f in squeeze_factor(&bw, i) {
                tally(&mut s4, &f.code, hit);
            }
        }
        i += 1;
    }
    let e4 = to_evidence(&s4);
    if let Some(e) = e4.get("squeeze") {
        out.insert("squeeze".into(), *e);
    }
    out
}

pub fn shock(input: &GuardInput) -> Shock {
    let h1 = sanitize(input.h1);
    let h4 = sanitize(input.h4);
    let r = log_returns(&h1);
    let mut raw_f: Vec<RawFactor> = Vec::new();
    if r.len() >= 96 {
        raw_f.extend(hourly_shock_factors(&h1, &r, r.len()));
    }
    if h4.len() >= 120 {
        let bw = bandwidth(&h4.iter().map(|c| c.close).collect::<Vec<_>>());
        raw_f.extend(squeeze_factor(&bw, bw.len() - 1));
    }
    // News burst: headlines of the last 6 h vs the average 6 h of the week.
    let now = input.now;
    let news: Vec<&NewsItem> = input.news.unwrap_or(&[]).iter().filter(|n| n.time <= now && n.time >= now - 7 * 86_400_000).collect();
    if news.len() >= 5 {
        let recent = news.iter().filter(|n| n.time >= now - 6 * 3_600_000).count();
        let avg = news.len() as f64 / 28.0;
        let burst = if avg > 0.0 { recent as f64 / avg } else { 0.0 };
        if burst >= 3.0 && recent >= 4 {
            raw_f.push(raw("newsBurst", 20.0, format!("Rafale d'actualités : {recent} articles en 6 h, {} fois plus que d'habitude.", one(burst))));
        } else if burst >= 2.0 && recent >= 3 {
            raw_f.push(raw("newsBusy", 10.0, format!("Actualité plus chargée que d'habitude ({recent} articles en 6 h).")));
        }
    }
    // Macro / geopolitical context: market stress (checked on this asset) and escalation headlines (not checkable).
    let m = input.macro_ctx.map(|x| &x.report);
    if let Some(m) = m {
        if m.market_score >= MACRO.tense {
            let mut top: Vec<_> = m.factors.iter().filter(|f| f.code != "escalation").collect();
            top.sort_by(|a, b| b.points.partial_cmp(&a.points).unwrap_or(std::cmp::Ordering::Equal));
            let top: Vec<&str> = top.iter().take(2).map(|f| f.text.as_str()).collect();
            let high = m.market_score >= MACRO.high;
            raw_f.push(raw(
                "macro",
                if high { 35.0 } else { 20.0 },
                format!("Contexte macro {} : {}", if high { "très tendu" } else { "tendu" }, top.join(" ")),
            ));
        }
        if let Some(esc) = m.factors.iter().find(|f| f.code == "escalation") {
            raw_f.push(raw("macroNews", esc.points, esc.text.clone()));
        }
    }
    // Market-wide fear (stocks): VIX.
    let vix = input.vix.unwrap_or(&[]);
    if m.is_none() && input.kind == Kind::Stock && vix.len() >= 2 {
        let v = vix[vix.len() - 1];
        let prev = vix[vix.len() - 2];
        if v >= 30.0 {
            raw_f.push(raw("vixHigh", 20.0, format!("Peur généralisée sur les marchés (VIX à {}).", one(v))));
        }
        if prev > 0.0 && v / prev - 1.0 >= 0.2 {
            raw_f.push(raw("vixJump", 15.0, format!("Le VIX a bondi de {} en une séance.", pct((v / prev - 1.0) * 100.0))));
        }
    }
    const HISTORICAL: [&str; 7] = ["vol2", "vol15", "jump4", "jump3", "volume", "squeeze", "macro"];
    let mut evidence = if raw_f.is_empty() { EvidenceMap::new() } else { shock_evidence(&h1, &h4) };
    if let Some(e) = input.macro_ctx.and_then(|x| x.evidence) {
        evidence.insert("macro".into(), e);
    }
    let factors = finalize(raw_f, &evidence, |c| HISTORICAL.contains(&c));
    let score = 100f64.min(factors.iter().fold(0.0, |acc, f| acc + f.points));
    let level = if score >= GUARD.shock_level {
        ShockLevel::Shock
    } else if score >= GUARD.shock_agitated {
        ShockLevel::Agitated
    } else {
        ShockLevel::Calm
    };
    Shock { score, level, factors }
}

// ---------- 3. Reversal ----------

/// Swing highs / lows: index whose value is the extreme of `k` bars on each side (`until` defaults to the last index
/// in the TypeScript: pass `values.len() as i64 - 1`).
pub fn pivots(values: &[f64], high: bool, k: usize, until: i64) -> Vec<usize> {
    let mut out = Vec::new();
    let k = k as i64;
    let mut i = k;
    while i <= until - k {
        let mut ok = true;
        let mut j = i - k;
        while j <= i + k && ok {
            if j != i {
                let (a, b) = (values[i as usize], values[j as usize]);
                ok = if high { a > b } else { a < b };
            }
            j += 1;
        }
        if ok {
            out.push(i as usize);
        }
        i += 1;
    }
    out
}

/// Regular divergence on the last two swings (within `window` bars, 60 in the TypeScript).
pub fn divergence(c: &[Candle], rsi_series: &Series, direction: Direction, until: i64, window: i64) -> bool {
    let from = 0.max(until - window);
    let down = direction == Direction::Down;
    let values: Vec<f64> = c.iter().map(|x| if down { x.high } else { x.low }).collect();
    let p: Vec<usize> = pivots(&values, down, 3, until).into_iter().filter(|i| *i as i64 >= from).collect();
    if p.len() < 2 {
        return false;
    }
    let (a, b) = (p[p.len() - 2], p[p.len() - 1]);
    let (Some(ra), Some(rb)) = (get(rsi_series, a as i64), get(rsi_series, b as i64)) else { return false };
    if down { values[b] > values[a] && rb < ra - 3.0 } else { values[b] < values[a] && rb > ra + 3.0 }
}

/// Technical reversal factors at candle i (data up to i only), on 4 h or daily candles (`tf`: `H4` or `D1`).
pub fn technical_reversal(c: &[Candle], r: &Series, e20: &Series, a: &Series, i: usize, direction: Direction, tf: Interval) -> Vec<RawFactor> {
    let mut f = Vec::new();
    let up = direction == Direction::Down; // reversing an up move
    let is4 = tf == Interval::H4;
    let tfs = tf.as_str();
    let label = if is4 { "4 h" } else { "journalier" };
    let (hi, lo) = if is4 { (80.0, 20.0) } else { (75.0, 25.0) };
    if let Some(rv) = get(r, i as i64) {
        if if up { rv > hi } else { rv < lo } {
            f.push(raw(format!("rsi{tfs}"), if is4 { 10.0 } else { 15.0 }, format!("RSI {label} extrême ({}).", number_to_string(round(rv)))));
        }
    }
    if divergence(c, r, direction, i as i64, 60) {
        f.push(raw(
            format!("div{tfs}"),
            20.0,
            format!(
                "Divergence {} en {label} : le prix fait un nouveau {} mais pas le RSI (essoufflement).",
                if up { "baissière" } else { "haussière" },
                if up { "sommet" } else { "creux" }
            ),
        ));
    }
    if let (Some(m), Some(at)) = (get(e20, i as i64), truthy(get(a, i as i64))) {
        if at > 0.0 {
            let ext = (c[i].close - m) / at;
            if if up { ext > 3.0 } else { ext < -3.0 } {
                f.push(raw(format!("extension{tfs}"), 15.0, format!("Prix très éloigné de sa moyenne 20 périodes en {label} ({} ATR).", one(ext.abs()))));
            }
        }
    }
    let k = &c[i];
    let body = truthy(Some((k.close - k.open).abs())).unwrap_or(k.close * 1e-6);
    let wick = if up { k.high - k.open.max(k.close) } else { k.open.min(k.close) - k.low };
    let vols: Vec<f64> = slice(c, 0.max(i as i64 - 50), i as i64).iter().map(|x| x.volume).collect();
    let z = if std(&vols) > 0.0 { (k.volume - mean(&vols)) / std(&vols) } else { 0.0 };
    if wick > 2.0 * body && z > 2.0 {
        f.push(raw(
            format!("rejection{tfs}"),
            10.0,
            format!("Bougie de rejet sur fort volume en {label} (longue mèche {}).", if up { "haute" } else { "basse" }),
        ));
    }
    f
}

/// Evidence of the technical reversal factors on the asset's history: at each past candle, direction = against the
/// short trend (EMA 20 vs EMA 50); event = counter-trend move of 3 ATR within the horizon (18 × 4 h, 5 days).
pub fn reversal_evidence(raw_c: &[Candle], tf: Interval) -> EvidenceMap {
    let c = sanitize(raw_c);
    let horizon = if tf == Interval::H4 { 18 } else { 5 };
    if c.len() < 100 {
        return EvidenceMap::new();
    }
    let closes: Vec<f64> = c.iter().map(|x| x.close).collect();
    let r = rsi(&closes, 14);
    let e20 = ema(&closes, 20);
    let e50 = ema(&closes, 50);
    let a = atr(&c, 14);
    let mut stats = Stats::new();
    let mut i = 60;
    while i + horizon < c.len() {
        if let (Some(m20), Some(m50), Some(at)) = (e20[i], e50[i], truthy(a[i])) {
            let direction = if m20 > m50 { Direction::Down } else { Direction::Up };
            let future = &c[i + 1..i + 1 + horizon];
            let hit = if direction == Direction::Down {
                js_min(future.iter().map(|x| x.low)) <= c[i].close - 3.0 * at
            } else {
                js_max(future.iter().map(|x| x.high)) >= c[i].close + 3.0 * at
            };
            tally(&mut stats, "_base", hit);
            for f in technical_reversal(&c, &r, &e20, &a, i, direction, tf) {
                tally(&mut stats, &f.code, hit);
            }
        }
        i += 1;
    }
    to_evidence(&stats)
}

fn is_technical(code: &str) -> bool {
    code.ends_with("4h") || code.ends_with("1d")
}

pub fn reversal(input: &GuardInput, trend: Trend) -> Reversal {
    let d = sanitize(input.daily);
    let h4 = sanitize(input.h4);
    let closes_d: Vec<f64> = d.iter().map(|x| x.close).collect();
    // Direction: against the background trend; in a range, against the last 5 days.
    let mut direction = match trend {
        Trend::Up => Some(Direction::Down),
        Trend::Down => Some(Direction::Up),
        Trend::Range => None,
    };
    if direction.is_none() && closes_d.len() > 6 {
        let mv = closes_d[closes_d.len() - 1] / closes_d[closes_d.len() - 6] - 1.0;
        if mv.abs() >= 0.03 {
            direction = Some(if mv > 0.0 { Direction::Down } else { Direction::Up });
        }
    }
    let Some(direction) = direction else { return Reversal { score: 0.0, direction: None, factors: vec![] } };
    let up = direction == Direction::Down;
    let mut raw_f: Vec<RawFactor> = Vec::new();

    // Technical (daily and 4 h), self-validated below.
    if d.len() > 60 {
        raw_f.extend(technical_reversal(&d, &rsi(&closes_d, 14), &ema(&closes_d, 20), &atr(&d, 14), d.len() - 1, direction, Interval::D1));
    }
    if h4.len() > 60 {
        let c4: Vec<f64> = h4.iter().map(|x| x.close).collect();
        raw_f.extend(technical_reversal(&h4, &rsi(&c4, 14), &ema(&c4, 20), &atr(&h4, 14), h4.len() - 1, direction, Interval::H4));
    }

    // Crowd positioning (crypto derivatives).
    let p = input.positioning;
    if let Some(fr_) = p.and_then(|p| p.funding_rate) {
        let fr_text = format!("{} % par 8 h", fr(fr_ * 100.0, 0, 4));
        if up && fr_ >= GUARD.funding_very_hot {
            raw_f.push(raw("funding", 25.0, format!("Financement des contrats perpétuels très élevé ({fr_text}) : les acheteurs à levier sont surchargés, risque de liquidations en cascade.")));
        } else if up && fr_ >= GUARD.funding_hot {
            raw_f.push(raw("funding", 15.0, format!("Financement élevé ({fr_text}) : beaucoup d'acheteurs à levier.")));
        } else if !up && fr_ <= GUARD.funding_very_cold {
            raw_f.push(raw("funding", 25.0, format!("Financement très négatif ({fr_text}) : les vendeurs à découvert sont surchargés, risque de rachat brutal (short squeeze).")));
        } else if !up && fr_ <= GUARD.funding_cold {
            raw_f.push(raw("funding", 15.0, format!("Financement négatif ({fr_text}) : beaucoup de vendeurs à découvert.")));
        }
    }
    let ls: &[f64] = p.map(|p| p.long_short_ratio.as_slice()).unwrap_or(&[]);
    if ls.len() >= 24 {
        let last = ls[ls.len() - 1];
        let rank = percentile_rank(&ls[..ls.len() - 1], last);
        if up && rank >= 90.0 {
            raw_f.push(raw("longShort", 10.0, format!("Ratio acheteurs/vendeurs à {}, parmi les plus hauts du mois : la foule est déjà acheteuse.", one(last))));
        }
        if !up && rank <= 10.0 {
            raw_f.push(raw("longShort", 10.0, format!("Ratio acheteurs/vendeurs à {}, parmi les plus bas du mois : la foule est déjà vendeuse.", one(last))));
        }
    }
    let oi: &[f64] = p.map(|p| p.open_interest.as_slice()).unwrap_or(&[]);
    if oi.len() >= 25 && h4.len() > 7 {
        let oi_change = oi[oi.len() - 1] / oi[oi.len() - 25] - 1.0;
        let price_change = (h4[h4.len() - 1].close / h4[h4.len() - 7].close - 1.0).abs();
        if oi_change >= 0.1 && price_change < 0.01 {
            raw_f.push(raw("openInterest", 10.0, format!("Positions à levier en hausse de {} en 24 h sans que le prix avance : situation fragile.", pct(oi_change * 100.0))));
        }
    }

    // Sentiment (contrarian at the extremes).
    let fg: &[f64] = input.sentiment.map(|s| s.fear_greed.as_slice()).unwrap_or(&[]);
    if let Some(&v) = fg.last() {
        let vs = number_to_string(v);
        if up && v >= 80.0 {
            raw_f.push(raw("fearGreed", 15.0, format!("Avidité extrême (Fear & Greed {vs}) : historiquement proche des sommets.")));
        } else if up && v >= 75.0 {
            raw_f.push(raw("fearGreed", 8.0, format!("Forte avidité (Fear & Greed {vs}).")));
        } else if !up && v <= 20.0 {
            raw_f.push(raw("fearGreed", 15.0, format!("Peur extrême (Fear & Greed {vs}) : historiquement proche des creux.")));
        } else if !up && v <= 25.0 {
            raw_f.push(raw("fearGreed", 8.0, format!("Forte peur (Fear & Greed {vs}).")));
        }
    }
    let sb = input.sentiment.and_then(|s| s.social_bullish);
    let ss = input.sentiment.map(|s| s.social_sample).unwrap_or(0.0);
    if let Some(sb) = sb {
        if ss >= 20.0 {
            let pct_s = number_to_string(round(sb));
            if up && sb >= 85.0 {
                raw_f.push(raw("social", 10.0, format!("Réseaux sociaux quasi unanimement optimistes ({pct_s} % haussiers sur StockTwits).")));
            }
            if !up && sb <= 30.0 {
                raw_f.push(raw("social", 10.0, format!("Réseaux sociaux très pessimistes ({pct_s} % haussiers seulement sur StockTwits).")));
            }
        }
    }

    // News tone against the trend (last 24 h).
    let now = input.now;
    let tone = news_tone(input.news.unwrap_or(&[]).iter().filter(|n| n.time >= now - 86_400_000 && n.time <= now));
    if up && tone.negative >= 2 && tone.negative > tone.positive {
        raw_f.push(raw("newsTone", 15.0, format!("{} actualités négatives en 24 h alors que la tendance est haussière.", tone.negative)));
    }
    if !up && tone.positive >= 2 && tone.positive > tone.negative {
        raw_f.push(raw("newsTone", 15.0, format!("{} actualités positives en 24 h alors que la tendance est baissière.", tone.positive)));
    }

    let technical: Vec<&RawFactor> = raw_f.iter().filter(|f| is_technical(&f.code)).collect();
    let mut evidence = EvidenceMap::new();
    if technical.iter().any(|f| f.code.ends_with("1d")) {
        evidence.extend(reversal_evidence(&d, Interval::D1));
    }
    if technical.iter().any(|f| f.code.ends_with("4h")) {
        evidence.extend(reversal_evidence(&h4, Interval::H4));
    }
    let factors = finalize(raw_f, &evidence, is_technical);
    Reversal { score: 100f64.min(factors.iter().fold(0.0, |acc, x| acc + x.points)), direction: Some(direction), factors }
}

// ---------- Everything together ----------

pub fn guard(input: &GuardInput) -> GuardResult {
    let reg = regime(input.daily, input.h4);
    let sh = shock(input);
    let rev = reversal(input, reg.trend);
    let mut notes: Vec<String> = Vec::new();
    let (mut scalping, mut size, mut stop) = (Scalping::Ok, 1.0, 1.0);
    if sh.level == ShockLevel::Shock {
        scalping = Scalping::Pause;
        size = 0.0;
        stop = 2.0;
        notes.push("Marché en choc : suspendre les nouvelles positions à court terme, laisser passer la tempête.".into());
    } else if sh.level == ShockLevel::Agitated {
        scalping = Scalping::Reduce;
        size = 0.5;
        stop = 1.5;
        notes.push("Marché agité : diviser la taille des positions par deux et élargir les stops (×1,5) pour ne pas être sorti par le bruit.".into());
    }
    if rev.score >= GUARD.reversal_high {
        if let Some(dir) = rev.direction {
            if scalping == Scalping::Ok {
                scalping = Scalping::Reduce;
                size = 0.5;
                stop = 1.0;
            }
            notes.push(format!(
                "Risque de retournement à la {} élevé : ne pas ouvrir de position dans le sens de la tendance actuelle, resserrer les stops des positions existantes.",
                if dir == Direction::Down { "baisse" } else { "hausse" }
            ));
        }
    }
    if notes.is_empty() {
        notes.push("Conditions normales : pas de signal de choc ni de retournement.".into());
    }
    if reg.trend != Trend::Range {
        notes.push(format!(
            "Pour le long terme : {} tant que la tendance de fond tient.",
            if reg.trend == Trend::Up { "privilégier les achats sur repli" } else { "privilégier la prudence, les rebonds sont fragiles" }
        ));
    }
    GuardResult { regime: reg, shock: sh, reversal: rev, policy: Policy { scalping, size_multiplier: size, stop_multiplier: stop, notes } }
}
