//! Selection: which stocks or cryptos to buy, and why, broken down criterion by criterion, for 8 durations
//! (30 min, 1 h, 5 h, 7 days, 14 days, 1 month, 3 months, 6 months) — `web/src/engine/screener.ts`.
//!
//! Each asset gets five scores from 0 to 100 (signal, trend, relative strength, buy zone, risk). ONE of them ranks,
//! chosen per market and duration from what was measured on the past (see SPECS and README); the others each have
//! a role (entry price, warning, stop and amount, information). Each selection is replayed on the past, without
//! look-ahead, and the page says plainly when a duration shows no edge once trading costs are paid.
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::fibonacci::{Horizon as ZoneHorizon, ZoneStatus, fib_zone, weekly};
use super::signal::{Action, AnalyzeOptions, analyze, atr, sanitize, sma};
use crate::js::{fr, iso_date, number_to_string, round};
use crate::types::{Candle, Kind};

/// "stock" | "crypto".
pub type Market = Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Horizon {
    #[serde(rename = "30m")]
    M30,
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "5h")]
    H5,
    #[serde(rename = "7d")]
    D7,
    #[serde(rename = "14d")]
    D14,
    #[serde(rename = "1m")]
    Mo1,
    #[serde(rename = "3m")]
    Mo3,
    #[serde(rename = "6m")]
    Mo6,
}

pub const HORIZON_LIST: [Horizon; 8] = [Horizon::M30, Horizon::H1, Horizon::H5, Horizon::D7, Horizon::D14, Horizon::Mo1, Horizon::Mo3, Horizon::Mo6];

impl Horizon {
    pub fn as_str(self) -> &'static str {
        match self {
            Horizon::M30 => "30m",
            Horizon::H1 => "1h",
            Horizon::H5 => "5h",
            Horizon::D7 => "7d",
            Horizon::D14 => "14d",
            Horizon::Mo1 => "1m",
            Horizon::Mo3 => "3m",
            Horizon::Mo6 => "6m",
        }
    }
    pub fn parse(s: &str) -> Option<Horizon> {
        HORIZON_LIST.into_iter().find(|h| h.as_str() == s)
    }
    /// `HORIZON_LABEL[h]`.
    pub fn label(self) -> &'static str {
        match self {
            Horizon::M30 => "30 min",
            Horizon::H1 => "1 h",
            Horizon::H5 => "5 h",
            Horizon::D7 => "7 j",
            Horizon::D14 => "14 j",
            Horizon::Mo1 => "1 mois",
            Horizon::Mo3 => "3 mois",
            Horizon::Mo6 => "6 mois",
        }
    }
    fn index(self) -> usize {
        HORIZON_LIST.iter().position(|h| *h == self).unwrap()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Criterion {
    Signal,
    Trend,
    Momentum,
    Zone,
    Risk,
}

/// A `Record<Criterion, T>` (same keys, same order as in the TypeScript).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ByCriterion<T> {
    pub signal: T,
    pub trend: T,
    pub momentum: T,
    pub zone: T,
    pub risk: T,
}

impl<T> ByCriterion<T> {
    pub fn get(&self, c: Criterion) -> &T {
        match c {
            Criterion::Signal => &self.signal,
            Criterion::Trend => &self.trend,
            Criterion::Momentum => &self.momentum,
            Criterion::Zone => &self.zone,
            Criterion::Risk => &self.risk,
        }
    }
}

pub const CRITERIA: ByCriterion<&str> =
    ByCriterion { signal: "Signal technique", trend: "Tendance de fond", momentum: "Force relative", zone: "Zone d'achat", risk: "Risque" };

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CandleInterval {
    #[serde(rename = "5m")]
    M5,
    #[serde(rename = "15m")]
    M15,
    #[serde(rename = "30m")]
    M30,
    #[serde(rename = "1d")]
    D1,
}

impl CandleInterval {
    pub fn as_str(self) -> &'static str {
        match self {
            CandleInterval::M5 => "5m",
            CandleInterval::M15 => "15m",
            CandleInterval::M30 => "30m",
            CandleInterval::D1 => "1d",
        }
    }
    pub fn parse(s: &str) -> Option<CandleInterval> {
        [CandleInterval::M5, CandleInterval::M15, CandleInterval::M30, CandleInterval::D1].into_iter().find(|i| i.as_str() == s)
    }
    /// Minutes per candle (`MIN`).
    pub fn minutes(self) -> i64 {
        match self {
            CandleInterval::M5 => 5,
            CandleInterval::M15 => 15,
            CandleInterval::M30 => 30,
            CandleInterval::D1 => 1440,
        }
    }
    /// Cache duration of the candles of this interval, ms (`TTL` in `web/server/screener.ts`).
    pub fn ttl(self) -> u64 {
        match self {
            CandleInterval::M5 => 4 * 60_000,
            CandleInterval::M15 => 8 * 60_000,
            CandleInterval::M30 => 12 * 60_000,
            CandleInterval::D1 => 30 * 60_000,
        }
    }
}

/// What ranks: the technical signal, relative strength, its opposite (the biggest recent fall first), low risk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RankRule {
    Signal,
    Momentum,
    Reversal,
    LowRisk,
}

impl RankRule {
    pub fn as_str(self) -> &'static str {
        match self {
            RankRule::Signal => "signal",
            RankRule::Momentum => "momentum",
            RankRule::Reversal => "reversal",
            RankRule::LowRisk => "lowRisk",
        }
    }
    /// `RANKED_CRITERION[rule]`.
    pub fn criterion(self) -> Criterion {
        match self {
            RankRule::Signal => Criterion::Signal,
            RankRule::Momentum | RankRule::Reversal => Criterion::Momentum,
            RankRule::LowRisk => Criterion::Risk,
        }
    }
}

/// `RANKED_CRITERION`.
pub fn ranked_criterion(rule: RankRule) -> Criterion {
    rule.criterion()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub interval: CandleInterval,
    /// Holding period, in candles of `interval`.
    pub hold: usize,
    /// Replay: a new selection every `step` candles.
    pub step: usize,
    pub rank: RankRule,
    /// Relative-strength window (candles) and skipped last candles.
    pub mom_len: usize,
    pub mom_skip: usize,
    /// Medium or Long only.
    pub zone: ZoneHorizon,
    /// Stop distance in ATR of `interval`; the target is at twice the risk.
    pub stop_atr: f64,
    /// Round-trip trading costs (fees and spread), as a fraction.
    pub cost: f64,
    /// What the measurement found, shown on the page.
    pub evidence: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn s(
    interval: CandleInterval,
    hold: usize,
    step: usize,
    rank: RankRule,
    mom_len: usize,
    mom_skip: usize,
    zone: ZoneHorizon,
    stop_atr: f64,
    evidence: &'static str,
    cost: f64,
) -> Spec {
    Spec { interval, hold, step, rank, mom_len, mom_skip, zone, stop_atr, cost, evidence }
}

use CandleInterval::{D1 as I1D, M5 as I5M, M15 as I15M, M30 as I30M};
use RankRule::{LowRisk, Momentum, Reversal, Signal as RSignal};
use ZoneHorizon::{Long as ZL, Medium as ZM};

/// Choices measured in September 2026 (top 10 vs the average of the universe; stocks: 136–149 large US stocks, daily
/// 2021–2026 and 60 days of 5-minute candles; cryptos: 80–108 coins, daily 2024–2026 and 2 weeks of 5-minute candles).
/// In the order of `HORIZON_LIST`; see `spec`.
pub const SPECS_STOCK: [Spec; 8] = [
    s(
        I5M,
        6,
        24,
        Reversal,
        6,
        0,
        ZM,
        3.0,
        "À 30 minutes, rien ne fait mieux que la moyenne une fois les frais payés : le meilleur critère (acheter ce qui vient de baisser) rapporte +0,047 % pour 0,05 % de frais.",
        0.0005,
    ),
    s(
        I15M,
        4,
        8,
        Reversal,
        4,
        0,
        ZM,
        3.0,
        "Acheter ce qui a le plus baissé dans l'heure : +0,04 % de mieux que la moyenne, mieux 59 % du temps, à peine au-dessus des frais.",
        0.0005,
    ),
    s(
        I30M,
        10,
        10,
        Momentum,
        10,
        0,
        ZM,
        2.5,
        "Acheter ce qui a le plus monté sur les 5 dernières heures de cotation : +0,34 % de mieux que la moyenne, mieux 64 % du temps (60 jours mesurés).",
        0.0005,
    ),
    s(I1D, 5, 10, Momentum, 126, 21, ZM, 1.5, "Force relative sur 6 mois : +0,6 % de mieux que la moyenne par semaine (2021–2026).", 0.0005),
    s(
        I1D,
        10,
        10,
        Momentum,
        126,
        21,
        ZM,
        1.5,
        "Force relative sur 6 mois : +0,6 à 0,8 % de mieux que la moyenne sur 2 semaines (2021–2026).",
        0.0005,
    ),
    s(
        I1D,
        21,
        21,
        Momentum,
        126,
        21,
        ZM,
        2.0,
        "Force relative sur 6 mois : +2,0 % de mieux que la moyenne par mois, mieux 62 % du temps (2021–2026).",
        0.0005,
    ),
    s(
        I1D,
        63,
        21,
        Momentum,
        126,
        21,
        ZM,
        2.0,
        "Force relative sur 6 mois : +9,9 % contre +5,3 % sur 3 mois, mieux 69 % du temps (2021–2026).",
        0.0005,
    ),
    s(
        I1D,
        126,
        21,
        Momentum,
        126,
        21,
        ZL,
        3.0,
        "Force relative sur 6 mois : +20,5 % contre +11,2 % sur 6 mois, mieux 73 % du temps (2021–2026).",
        0.0005,
    ),
];

pub const SPECS_CRYPTO: [Spec; 8] = [
    s(
        I5M,
        6,
        24,
        Reversal,
        78,
        0,
        ZM,
        3.0,
        "À 30 minutes, rien ne fait mieux que la moyenne une fois les frais payés : le meilleur critère (acheter ce qui a baissé sur 6 h) rapporte +0,05 % pour 0,2 % de frais.",
        0.002,
    ),
    s(
        I15M,
        4,
        8,
        Momentum,
        4,
        0,
        ZM,
        3.0,
        "Acheter ce qui a le plus monté dans l'heure : +0,09 % de mieux que la moyenne, entièrement mangé par les frais (0,2 %).",
        0.002,
    ),
    s(
        I30M,
        10,
        10,
        Reversal,
        10,
        0,
        ZM,
        2.5,
        "Acheter ce qui a le plus baissé sur 5 h (retour à la moyenne) : +0,34 % de mieux que la moyenne, mieux 60 % du temps (2 semaines mesurées : échantillon court).",
        0.002,
    ),
    s(
        I1D,
        7,
        7,
        Momentum,
        90,
        0,
        ZM,
        1.5,
        "Force relative sur 3 mois : +0,75 % de mieux que la moyenne par semaine, positive dans les deux moitiés de 2024–2026.",
        0.002,
    ),
    s(I1D, 14, 14, Momentum, 90, 0, ZM, 1.5, "Force relative sur 3 mois : +1,1 % de mieux que la moyenne sur 2 semaines (2024–2026).", 0.002),
    s(I1D, 30, 15, RSignal, 126, 21, ZM, 2.0, "Signal technique d'Altim : +2,3 % contre −1,9 % par mois, mieux 60 % du temps (2024–2026).", 0.002),
    s(I1D, 90, 15, RSignal, 126, 21, ZM, 2.0, "Signal technique d'Altim : +2,0 % contre −6,4 % sur 3 mois, mieux 66 % du temps (2024–2026).", 0.002),
    s(
        I1D,
        180,
        15,
        LowRisk,
        126,
        21,
        ZL,
        3.0,
        "Les cryptos les plus calmes (les grandes) : +9,4 % contre −12,8 % sur 6 mois, mieux 94 % du temps (périodes qui se chevauchent : à prendre avec prudence).",
        0.002,
    ),
];

/// `SPECS[market][h]`.
pub fn spec(market: Market, h: Horizon) -> &'static Spec {
    match market {
        Kind::Stock => &SPECS_STOCK[h.index()],
        Kind::Crypto => &SPECS_CRYPTO[h.index()],
    }
}

/// Former horizons → new ones.
pub fn to_horizon(h: &str, market: Market) -> Option<Horizon> {
    if let Some(x) = Horizon::parse(h) {
        return Some(x);
    }
    match h {
        "short" => Some(Horizon::D14),
        "medium" => Some(if market == Kind::Crypto { Horizon::Mo1 } else { Horizon::Mo3 }),
        "long" => Some(if market == Kind::Crypto { Horizon::Mo3 } else { Horizon::Mo6 }),
        _ => None,
    }
}

/// Criterion shown as the one that ranks, and the role of each criterion.
pub fn roles(spec: &Spec, market: Market) -> ByCriterion<String> {
    let what = if market == Kind::Crypto { "les cryptos" } else { "les actions" };
    let mut r = ByCriterion {
        signal: "information".to_string(),
        trend: "alerte si baissière".to_string(),
        momentum: "information".to_string(),
        zone: "fixe le prix d'entrée".to_string(),
        risk: "règle le stop et le montant".to_string(),
    };
    match spec.rank {
        RankRule::Signal => r.signal = format!("classe {what}"),
        RankRule::Momentum => r.momentum = format!("classe {what}"),
        RankRule::Reversal => r.momentum = "classe à l'envers : les plus en baisse d'abord".to_string(),
        RankRule::LowRisk => r.risk = format!("classe {what} (les plus calmes d'abord)"),
    }
    r
}

/// At most this many stocks of the same sector in a selection.
pub const SECTOR_CAP: usize = 3;

/// A coin that barely moves follows a currency or another asset (stablecoin missing from the lists, pegged token):
/// volatility brought to a daily scale under 0.5 %.
pub fn is_pegged(atr_pct: Option<f64>, interval: CandleInterval) -> bool {
    match atr_pct {
        None => false,
        Some(a) => a * (1440.0 / interval.minutes() as f64).sqrt() < 0.5,
    }
}

/// Duration in words of `n` candles of an interval ("30 min", "5 h", "6 mois"…).
pub fn span(n: usize, interval: CandleInterval, market: Market) -> String {
    if interval != CandleInterval::D1 {
        let m = n as i64 * interval.minutes();
        return if m < 60 {
            format!("{m} min")
        } else {
            format!("{} h", number_to_string(round((m as f64 / 60.0) * 10.0) / 10.0)).replacen('.', ",", 1)
        };
    }
    let per_month = if market == Kind::Stock { 21 } else { 30 };
    if n < per_month {
        return format!("{n} {}", if market == Kind::Stock { "séances" } else { "jours" });
    }
    let months = round(n as f64 / per_month as f64);
    format!("{} mois", number_to_string(months))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawFactors {
    pub signal: f64,
    pub trend: f64,
    pub zone: f64,
    pub zone_status: ZoneStatus,
    pub zone_distance: Option<f64>,
    /// Raw values, turned into ranks across the universe.
    pub momentum: Option<f64>,
    pub volatility: Option<f64>,
    pub drawdown: Option<f64>,
    pub price: f64,
    pub action: Action,
    pub signal_score: f64,
    pub confidence: f64,
    pub atr_pct: Option<f64>,
}

/// `Math.max(0, Math.min(100, v))` (NaN stays NaN).
fn clamp(v: f64) -> f64 {
    if v.is_nan() { v } else { 0f64.max(100f64.min(v)) }
}

/// Factors of one asset known at the close of candle `i` (only candles 0…i are read).
pub fn factors_at(all: &[Candle], i: usize, spec: &Spec) -> Option<RawFactors> {
    if i < 210 {
        return None;
    }
    let c = &all[(i.saturating_sub(399)).min(all.len())..(i + 1).min(all.len())];
    let last = *c.last()?;
    let closes: Vec<f64> = c.iter().map(|x| x.close).collect();
    let s = analyze(c, &AnalyzeOptions::default());
    let signal = s.as_ref().map(|s| clamp(50.0 + s.score / 2.0)).unwrap_or(50.0);
    let s200 = sma(&closes, 200);
    let s50 = sma(&closes, 50);
    let a = s200[s200.len() - 1];
    let b = s50[s50.len() - 1];
    let a20 = s200.len().checked_sub(21).and_then(|k| s200[k]);
    let trend = match (a, b) {
        (Some(a), Some(b)) => {
            (if last.close > a { 40.0 } else { 0.0 }) + (if b > a { 30.0 } else { 0.0 }) + (if a20.is_some_and(|a20| a > a20) { 30.0 } else { 0.0 })
        }
        _ => 50.0,
    };
    let end = c.len() as isize - 1 - spec.mom_skip as isize;
    let start = end - spec.mom_len as isize;
    let momentum = if start >= 0 { Some((c[end as usize].close / c[start as usize].close - 1.0) * 100.0) } else { None };
    let at = atr(c, 14);
    let atr_pct = at[at.len() - 1].map(|v| (v / last.close) * 100.0);
    let mut peak = 0f64;
    for x in &c[c.len().saturating_sub(252)..] {
        peak = peak.max(x.high);
    }
    let drawdown = if peak > 0.0 { Some((1.0 - last.close / peak) * 100.0) } else { None };
    let z = if spec.zone == ZoneHorizon::Long {
        fib_zone(&weekly(c), ZoneHorizon::Long, Some(last.close), false)
    } else {
        fib_zone(c, ZoneHorizon::Medium, Some(last.close), false)
    };
    let zone = match z.status {
        ZoneStatus::InZone | ZoneStatus::Golden => 100.0,
        ZoneStatus::Deep => 60.0,
        ZoneStatus::None => 40.0,
        ZoneStatus::Broken => 0.0,
        ZoneStatus::Downtrend => 10.0,
        ZoneStatus::Above => clamp(100.0 - z.distance.unwrap_or(0.0) * 6.0),
    };
    Some(RawFactors {
        signal,
        trend,
        zone,
        zone_status: z.status,
        zone_distance: z.distance,
        momentum,
        volatility: atr_pct,
        drawdown,
        price: last.close,
        action: s.as_ref().map(|s| s.action).unwrap_or(Action::Hold),
        signal_score: s.as_ref().map(|s| s.score).unwrap_or(0.0),
        confidence: s.as_ref().map(|s| s.confidence).unwrap_or(0.0),
        atr_pct,
    })
}

/// Percentile rank (0–100) of each value among the others; None stays None.
pub fn ranks(values: &[Option<f64>], higher_is_better: bool) -> Vec<Option<f64>> {
    let mut known: Vec<f64> = values.iter().flatten().copied().filter(|v| v.is_finite()).collect();
    known.sort_by(|x, y| x.partial_cmp(y).unwrap());
    if known.len() < 2 {
        return values.iter().map(|v| v.map(|_| 50.0)).collect();
    }
    values
        .iter()
        .map(|v| {
            let v = (*v).filter(|v| v.is_finite())?;
            let (mut below, mut equal) = (0.0, 0.0);
            for &k in &known {
                if k < v {
                    below += 1.0;
                } else if k == v {
                    equal += 1.0;
                }
            }
            let p = ((below + (equal - 1.0) / 2.0) / (known.len() as f64 - 1.0)) * 100.0;
            Some(if higher_is_better { p } else { 100.0 - p })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Scored {
    pub scores: ByCriterion<f64>,
    pub total: f64,
}

/// Scores of a whole universe at the same date: momentum and risk are ranked against each other; `total` is the
/// ranking score of the spec's rule.
pub fn score_universe(list: &[Option<RawFactors>], spec: &Spec) -> Vec<Option<Scored>> {
    let mom = ranks(&list.iter().map(|f| f.as_ref().and_then(|f| f.momentum)).collect::<Vec<_>>(), true);
    let vol = ranks(&list.iter().map(|f| f.as_ref().and_then(|f| f.volatility)).collect::<Vec<_>>(), false);
    let dd = ranks(&list.iter().map(|f| f.as_ref().and_then(|f| f.drawdown)).collect::<Vec<_>>(), false);
    list.iter()
        .enumerate()
        .map(|(i, f)| {
            let f = f.as_ref()?;
            if matches!(spec.rank, RankRule::Momentum | RankRule::Reversal) && mom[i].is_none() {
                return None;
            }
            if spec.rank == RankRule::LowRisk && vol[i].is_none() {
                return None;
            }
            let risk = match (vol[i], dd[i]) {
                (Some(v), Some(d)) => (v + d) / 2.0,
                _ => 50.0,
            };
            let scores = ByCriterion { signal: f.signal, trend: f.trend, momentum: mom[i].unwrap_or(50.0), zone: f.zone, risk };
            let total = match spec.rank {
                RankRule::Signal => f.signal,
                RankRule::Momentum => mom[i].unwrap(),
                RankRule::Reversal => 100.0 - mom[i].unwrap(),
                RankRule::LowRisk => vol[i].unwrap(),
            };
            Some(Scored { scores, total })
        })
        .collect()
}

/// Best `top_n` by ranking score, at most SECTOR_CAP per sector (sectors optional).
pub fn pick<T>(items: &[T], score: impl Fn(&T) -> Option<f64>, sector: impl Fn(&T) -> Option<String>, top_n: usize) -> Vec<&T> {
    let mut sorted: Vec<(&T, f64)> = items.iter().filter_map(|t| score(t).map(|v| (t, v))).collect();
    // Stable, like Array.prototype.sort.
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut per: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<&T> = Vec::new();
    for (t, _) in sorted {
        if let Some(sec) = sector(t).filter(|s| !s.is_empty()) {
            let n = per.entry(sec).or_insert(0);
            if *n >= SECTOR_CAP {
                continue;
            }
            *n += 1;
        }
        out.push(t);
        if out.len() >= top_n {
            break;
        }
    }
    out
}

fn signed_pct(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, 2))
}

fn rnd(v: f64) -> String {
    number_to_string(round(v))
}

/// Plain-language explanation of each criterion's score.
pub fn explain(f: &RawFactors, s: &Scored, spec: &Spec, market: Market) -> ByCriterion<String> {
    let period = format!(
        "{}{}",
        span(spec.mom_len, spec.interval, market),
        if spec.mom_skip != 0 { format!(" (hors {})", span(spec.mom_skip, spec.interval, market)) } else { String::new() }
    );
    let per = if spec.interval == CandleInterval::D1 { "par jour".to_string() } else { format!("par bougie de {} min", spec.interval.minutes()) };
    let zone = match f.zone_status {
        ZoneStatus::InZone => "Dans la zone d'achat Fibonacci.".to_string(),
        ZoneStatus::Golden => "Dans la « zone d'or » (61,8–65 %).".to_string(),
        ZoneStatus::Deep => "Repli profond, proche du dernier soutien.".to_string(),
        ZoneStatus::Above => format!("Au-dessus de la zone : repli de {} pour l'atteindre.", signed_pct(-f.zone_distance.unwrap_or(0.0))),
        ZoneStatus::Broken => "Plus bas cassé : zone invalidée.".to_string(),
        ZoneStatus::Downtrend => "Mouvement baissier : pas de zone d'achat.".to_string(),
        ZoneStatus::None => "Pas de mouvement net pour tracer une zone.".to_string(),
    };
    let signal = match f.action {
        Action::StrongBuy => format!("Achat fort (score {}, confiance {} %).", rnd(f.signal_score), rnd(f.confidence)),
        Action::Buy => format!("Achat (score {}, confiance {} %).", rnd(f.signal_score), rnd(f.confidence)),
        Action::Hold => format!("Neutre (score {}).", rnd(f.signal_score)),
        _ => format!("Vente (score {}).", rnd(f.signal_score)),
    };
    let t = s.scores.trend;
    let trend = if t >= 100.0 {
        "Haussière : au-dessus de la moyenne 200 jours, qui monte."
    } else if t >= 70.0 {
        "Plutôt haussière."
    } else if t >= 40.0 {
        "Mitigée."
    } else {
        "Baissière : sous la moyenne 200 jours."
    }
    .to_string();
    let momentum = match f.momentum {
        None => "Historique trop court.".to_string(),
        Some(m) if spec.rank == RankRule::Reversal => {
            format!("{} sur {period} : a plus baissé que {} % des autres (rebond attendu).", signed_pct(m), rnd(100.0 - s.scores.momentum))
        }
        Some(m) => format!("{} sur {period}, mieux que {} % des autres.", signed_pct(m), rnd(s.scores.momentum)),
    };
    let risk = format!(
        "Volatilité {}{} : plus calme que {} % des autres.",
        match f.atr_pct {
            Some(a) => format!("{} % {per}", fr(a, 0, 2)),
            None => "inconnue".to_string(),
        },
        match f.drawdown {
            Some(d) => format!(", {} % sous son plus haut récent", rnd(d)),
            None => String::new(),
        },
        rnd(s.scores.risk)
    );
    ByCriterion { signal, trend, momentum, zone, risk }
}

/// clear: ahead of the average by more than the costs, more than 55 % of the time · weak: ahead on average but about
/// one time in two (a few big winners) · none: not ahead once the costs are paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Clear,
    Weak,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Validation {
    pub horizon: Horizon,
    pub periods: usize,
    /// Average forward return of the top N, of the whole universe, and share of periods where the top N did better.
    pub top: f64,
    pub universe: f64,
    pub beat_rate: f64,
    pub top_n: usize,
    pub hold: usize,
    pub from: Option<i64>,
    pub to: Option<i64>,
    /// Return of the reference (Bitcoin for cryptos) over the same periods.
    pub benchmark: Option<f64>,
    /// Round-trip costs (%).
    pub cost: f64,
    pub edge: Edge,
    pub no_edge: bool,
}

/// Replays the selection on the past: every `step` candles, score the universe with the data known that day,
/// take the top N, and compare their return over the next `hold` candles with the universe average.
/// Series must be aligned on the same dates (see align_series). `top_n` defaults to 10 in the TypeScript.
pub fn validate(
    series: &[Vec<Candle>],
    spec: &Spec,
    h: Horizon,
    top_n: usize,
    sectors: Option<&[String]>,
    benchmark: Option<usize>,
) -> Option<Validation> {
    let n = series.iter().map(|s| s.len()).min()?;
    let (hold, every) = (spec.hold, spec.step.max(1));
    let (mut tops, mut alls, mut bench): (Vec<f64>, Vec<f64>, Vec<f64>) = (vec![], vec![], vec![]);
    let mut beat = 0usize;
    let (mut from, mut to): (Option<i64>, Option<i64>) = (None, None);
    let mut i = 280;
    while i + hold < n {
        let f: Vec<Option<RawFactors>> = series.iter().map(|s| factors_at(s, s.len() - n + i, spec)).collect();
        let sc = score_universe(&f, spec);
        let fwd: Vec<f64> = series
            .iter()
            .map(|s| {
                let j = s.len() - n + i;
                s[j + hold].close / s[j].close - 1.0
            })
            .collect();
        let items: Vec<(usize, Option<f64>)> = sc.iter().enumerate().map(|(k, x)| (k, x.map(|x| x.total))).collect();
        let idx: Vec<usize> = pick(&items, |x| x.1, |x| sectors.and_then(|s| s.get(x.0).cloned()), top_n).into_iter().map(|x| x.0).collect();
        if idx.len() < top_n {
            i += every;
            continue;
        }
        let top_r = idx.iter().fold(0.0, |a, &k| a + fwd[k]) / idx.len() as f64;
        let all = fwd.iter().fold(0.0, |a, b| a + b) / fwd.len() as f64;
        tops.push(top_r);
        alls.push(all);
        if let Some(b) = benchmark {
            bench.push(fwd[b]);
        }
        if top_r > all {
            beat += 1;
        }
        let t = series[0][series[0].len() - n + i].time;
        from.get_or_insert(t);
        to = Some(t);
        i += every;
    }
    if tops.is_empty() {
        return None;
    }
    let avg = |v: &[f64]| (v.iter().fold(0.0, |a, b| a + b) / v.len() as f64) * 100.0;
    let top = avg(&tops);
    let universe = avg(&alls);
    let beat_rate = (beat as f64 / tops.len() as f64) * 100.0;
    let cost = spec.cost * 100.0;
    let edge = if top - universe <= cost || beat_rate < 45.0 {
        Edge::None
    } else if beat_rate >= 55.0 {
        Edge::Clear
    } else {
        Edge::Weak
    };
    Some(Validation {
        horizon: h,
        periods: tops.len(),
        top,
        universe,
        beat_rate,
        top_n,
        hold,
        from,
        to,
        benchmark: if bench.is_empty() { None } else { Some(avg(&bench)) },
        cost,
        edge,
        no_edge: edge != Edge::Clear,
    })
}

/// Keeps the dates (daily) or the exact times (intraday) present in every series.
pub fn align_series(series: &[Vec<Candle>], intraday: bool) -> Vec<Vec<Candle>> {
    let clean: Vec<Vec<Candle>> = series.iter().map(|s| sanitize(s)).collect();
    let day = |t: i64| if intraday { t.to_string() } else { iso_date(t) };
    let mut common: Option<HashSet<String>> = None;
    for s in &clean {
        let d: HashSet<String> = s.iter().map(|c| day(c.time)).collect();
        common = Some(match common {
            Some(acc) => acc.into_iter().filter(|x| d.contains(x)).collect(),
            None => d,
        });
    }
    let common = common.unwrap_or_default();
    clean.into_iter().map(|s| s.into_iter().filter(|c| common.contains(&day(c.time))).collect()).collect()
}
