//! Decision engine behind `GET /api/decision`: one verdict per asset (ACHETER, ZONE D'ACHAT, ATTENDRE,
//! AUCUNE POSITION, ALLÉGER, VENDRE), why, what would change it and what could make it wrong.
//!
//! Pure and deterministic: every input is passed in (`DecisionInput`), nothing is fetched here. It reuses the other
//! engines rather than re-deriving them: the signal (`signal::analyze` + `reliability::gate`), the Fibonacci buy
//! zones, the guard (regime, shock, reversal, divergence) and the macro report.
//!
//! Rules (all thresholds are constants below):
//! - families: independent scores −100 … +100 (positive = favourable to buying); a family without data is
//!   "unavailable", never estimated;
//! - vetoes: every check is listed; an active one forbids ACHETER. "Blocking" vetoes (downtrend, unreliable data,
//!   crash, extreme volatility, abnormal pump, artificial volume, insufficient liquidity, spread, regulation,
//!   major event, macro shock) mean AUCUNE POSITION; the "timing" ones (risk/reward, earnings, divergence) mean
//!   ATTENDRE;
//! - setup: "achat sur repli en tendance haussière", 9 steps. Complete = steps 1-3 (trend, correction, support),
//!   7 (confirmation), 8 and 9 (stop, target) are met, and no other step is failed (unknown is tolerated);
//! - ACHETER: complete setup, no active veto, risk/reward from the current price ≥ 2;
//!   ZONE D'ACHAT: price on the support (step 3), no veto other than the risk/reward, confirmation missing;
//!   AUCUNE POSITION: a blocking veto; ATTENDRE: everything else;
//! - held (average cost given): VENDRE when a defensive exit is met now (close under the stop, move broken,
//!   background trend down), ALLÉGER when a profit or macro exit is met now, otherwise ATTENDRE (keep);
//! - confidence = 45 % agreement of the families + 35 % data reliability (sources and available families)
//!   + 20 % the signal's own track record (50 when unknown);
//! - technical structure (`structure.rs`, daily candles): Ichimoku and Supertrend agreeing add or remove 10 points
//!   to the trend family, the nearest resistance and a fake breakout are cited in "pourquoi attendre", target 3 is
//!   the next level beyond target 2;
//! - "quand ne pas trader", action zones, watched scenarios, counter-argument and snapshot (`guidance.rs`) are built
//!   from what is measured here and never change the verdict;
//! - rating, composite score, "signal dégradé", market regime and horizon (`synthesis.rs`) summarise the decision
//!   without changing the verdict;
//! - « preuve du modèle » (`model_evidence.rs`, from an already-computed cross-asset validation only): weak evidence
//!   on the asset's class caps the confidence at 60 and adds a con, unproven evidence adds the con, a strong rating
//!   without an edge becomes a plain buy / sell; never raises anything, never changes the verdict.
//! - « Bot Altim » (`bot.rs`, from an already-trained report only): when today's action (ACHETER / VENDRE) is on a
//!   side with an out-of-sample edge, a pro or con and ± 3 confidence points on a buy-side verdict; otherwise shown
//!   only. Never changes the verdict or a veto.
use serde::{Deserialize, Serialize};

use super::Evidence;
use super::bot::{BotReport, bot_view};
use super::decision_types::*;
use super::fibonacci::{FibZone, Horizon, Swing, Trend as SwingTrend, ZoneStatus, fib_zones, level, weekly};
use super::guard::{Direction, GuardResult, NewsItem, Regime, ShockLevel, Trend, divergence, news_tone, percentile_rank, regime};
use super::guidance::{self, Observed};
use super::macro_ctx::{MACRO, MacroLevel, MacroReport};
use super::model_evidence::model_evidence;
use super::reliability::{Reliability, ReliabilityLevel, gate};
use super::signal::{AnalyzeOptions, Candle, Signal, adx, analyze, atr, is_sell, rsi, sanitize, sma};
use super::structure::{Benchmark, Bias, BreakoutKind, Structure, structure};
use super::synthesis::{self, PlanCandles, ScoreWeights};
use super::validation::ValidationReport;
use crate::calendar::{CalendarEvent, EventKind, Importance};
use crate::js::{fr, iso_date, round};
use crate::types::{DAY_MS, Interval, Kind};

// ---------- Thresholds ----------

/// Below it, no entry.
pub const MIN_RISK_REWARD: f64 = 2.0;
/// Spread above it (%): veto.
pub const MAX_SPREAD_PCT: f64 = 0.5;
/// Average daily traded value below it (USD): veto.
pub const MIN_DAILY_VALUE: f64 = 1_000_000.0;
/// Daily ATR above it (% of the price): extreme volatility, veto (same limit as the signal's warning).
pub const MAX_ATR_PCT: f64 = 8.0;
/// Earnings within this many days: veto (stocks).
pub const EARNINGS_DAYS: f64 = 5.0;
/// 3-day fall beyond this many daily ATR: crash (veto while it has not stabilised).
pub const CRASH_ATR: f64 = 3.0;
/// 3-day rise beyond this many standard deviations of the asset's own 3-day moves: abnormal pump.
pub const PUMP_SIGMA: f64 = 3.0;
/// Day volume ≥ this many times the 20-day median with a move under half an ATR: artificial volume.
pub const VOLUME_ANOMALY: f64 = 5.0;
/// Macro stress score from which it is a shock (the macro engine's "high" level), or VIX at or above 30.
pub const MACRO_SHOCK_VIX: f64 = 30.0;
/// Share of the portfolio correlated to the same factor from which a warning is given (%), and the correlation.
pub const EXPOSURE_WARNING: f64 = 35.0;
pub const EXPOSURE_CORRELATION: f64 = 0.7;
/// Family status: score at or above +20 positive, at or below −20 negative.
const STATUS_LIMIT: f64 = 20.0;

// ---------- Inputs ----------

/// Sentiment and news observed for the asset (the guard's inputs: alternative.me, OKX, StockTwits, Google News).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketInputs {
    pub fear_greed: Option<f64>,
    /// Perpetual funding rate per 8 h, fraction.
    pub funding_rate: Option<f64>,
    pub social_bullish: Option<f64>,
    pub social_sample: f64,
    /// Headlines about the asset (7 days); None = not fetched.
    pub news: Option<Vec<NewsItem>>,
}

/// One line of the user's portfolio, with its daily candles (for the correlation). Never stored.
#[derive(Debug, Clone, PartialEq)]
pub struct HoldingSeries {
    pub symbol: String,
    pub kind: Kind,
    /// % of the portfolio.
    pub weight: f64,
    pub daily: Vec<Candle>,
}

/// The user's weights and the factor they are compared with (Bitcoin for a crypto, the S&P 500 for a stock).
#[derive(Debug, Clone, PartialEq)]
pub struct ExposureInput {
    pub factor: String,
    pub factor_symbol: String,
    pub factor_daily: Vec<Candle>,
    pub holdings: Vec<HoldingSeries>,
}

#[derive(Debug, Clone)]
pub struct DecisionInput<'a> {
    pub symbol: &'a str,
    pub kind: Kind,
    pub name: &'a str,
    pub now: i64,
    /// Consensus quote (falls back on the last close).
    pub price: Option<f64>,
    pub h1: &'a [Candle],
    pub h4: &'a [Candle],
    pub daily: &'a [Candle],
    /// ≈ 3 years of daily candles (long-term zone, 200-day average, history of the volatility).
    pub long: &'a [Candle],
    /// Reliability of the candles (the weakest of the 4 h and daily consensus) and their quality issues.
    pub reliability: Reliability,
    pub quality_issues: Vec<String>,
    /// Platforms agreeing on the daily candles, out of `sources`.
    pub agreeing: usize,
    pub sources: usize,
    /// How the Fibonacci zones behaved on this asset's history, by horizon (from `fib_zones(.., true)`).
    pub zone_evidence: Vec<(Horizon, Evidence)>,
    pub guard: Option<&'a GuardResult>,
    pub macro_report: Option<&'a MacroReport>,
    pub macro_evidence: Option<Evidence>,
    pub market: MarketInputs,
    pub fundamentals: Option<Fundamentals>,
    pub liquidity: Option<Liquidity>,
    pub track: Option<Track>,
    /// The user's average cost (personal mode).
    pub cost: Option<f64>,
    /// The user's weights (personal mode).
    pub exposure: Option<ExposureInput>,
    /// Benchmarks' daily candles (Bitcoin for a crypto; S&P 500 then Nasdaq-100 for a stock): relative strength,
    /// correlation, and the first one's trend for the market regime. Empty when not loaded.
    pub benchmarks: Vec<Benchmark>,
    /// Weights of the composite score (`w=`); None = default weights.
    pub score_weights: Option<ScoreWeights>,
    /// Upcoming events of the next days (calendar::upcoming_for); None when the calendar could not be loaded.
    pub events: Option<Vec<CalendarEvent>>,
    /// Cross-asset validation report already in the cache (never computed for a decision); None = not computed yet.
    pub validation: Option<&'a ValidationReport>,
    /// « Bot Altim » report already in the cache (never computed for a decision); None = not computed yet.
    pub bot: Option<&'a BotReport>,
}

// ---------- Formatting ----------

fn usd(v: f64) -> String {
    crate::fx::money(v)
}
fn pct(v: f64) -> String {
    format!("{} %", fr(v, 0, 1))
}
fn signed(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, 1))
}
/// First letter in upper case.
fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}
fn ratio(v: f64) -> String {
    fr(v, 1, 1)
}
fn money(v: f64) -> String {
    let (v, sym) = crate::fx::convert(v);
    if v >= 1e12 {
        format!("{} T{sym}", fr(v / 1e12, 0, 1))
    } else if v >= 1e9 {
        format!("{} Md{sym}", fr(v / 1e9, 0, 1))
    } else if v >= 1e6 {
        format!("{} M{sym}", fr(v / 1e6, 0, 1))
    } else {
        format!("{} {sym}", fr(v, 0, 0))
    }
}
fn r1(v: f64) -> f64 {
    round(v * 10.0) / 10.0
}
fn r2(v: f64) -> f64 {
    round(v * 100.0) / 100.0
}
fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    x.max(lo).min(hi)
}
fn mean(v: &[f64]) -> f64 {
    if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 }
}
fn std(v: &[f64]) -> f64 {
    if v.len() < 2 {
        return 0.0;
    }
    let m = mean(v);
    (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (v.len() - 1) as f64).sqrt()
}
fn last_some(s: &[Option<f64>]) -> Option<f64> {
    s.last().copied().flatten()
}
/// "29/10/2026".
fn date_fr(ms: i64) -> String {
    let d = iso_date(ms);
    format!("{}/{}/{}", &d[8..10], &d[5..7], &d[0..4])
}
fn status_of(score: f64) -> Status {
    if score >= STATUS_LIMIT {
        Status::Positive
    } else if score <= -STATUS_LIMIT {
        Status::Negative
    } else {
        Status::Neutral
    }
}

// ---------- Measures shared by the parts ----------

struct Metrics {
    price: f64,
    d: Vec<Candle>,
    h4: Vec<Candle>,
    /// Long daily history (or the daily candles when longer).
    ld: Vec<Candle>,
    atr_d: Option<f64>,
    atr_pct: Option<f64>,
    /// Percentile of today's ATR % in the asset's history.
    atr_rank: Option<f64>,
    sma50: Option<f64>,
    sma200: Option<f64>,
    sma50_prev: Option<f64>,
    adx_d: Option<f64>,
    rsi_d: Option<f64>,
    regime: Regime,
    sigd: Option<Signal>,
    sig4: Option<Signal>,
    /// Bars per month (30 for cryptos, 21 sessions for stocks).
    month: usize,
    div_d: bool,
    div_4h: bool,
    /// Technical structure of the daily candles.
    st: Structure,
}

fn change(c: &[Candle], price: f64, bars: usize) -> Option<f64> {
    if c.len() <= bars {
        return None;
    }
    let base = c[c.len() - 1 - bars].close;
    if base > 0.0 { Some((price / base - 1.0) * 100.0) } else { None }
}

fn metrics(inp: &DecisionInput) -> Metrics {
    let d = sanitize(inp.daily);
    let h4 = sanitize(inp.h4);
    let h1 = sanitize(inp.h1);
    let long = sanitize(inp.long);
    let ld = if long.len() > d.len() { long } else { d.clone() };
    let last_close = h1.last().or(h4.last()).or(d.last()).map(|c| c.close).unwrap_or(0.0);
    let price = inp.price.filter(|p| p.is_finite() && *p > 0.0).unwrap_or(last_close);
    let atr_d = last_some(&atr(&d, 14));
    let atr_pct = atr_d.filter(|_| price > 0.0).map(|a| a / price * 100.0);
    let atr_rank = {
        let a = atr(&ld, 14);
        let hist: Vec<f64> = a.iter().zip(&ld).filter_map(|(a, c)| a.map(|a| a / c.close * 100.0)).collect();
        match (hist.len() >= 100, atr_pct) {
            (true, Some(now)) => Some(percentile_rank(&hist, now)),
            _ => None,
        }
    };
    let closes: Vec<f64> = ld.iter().map(|c| c.close).collect();
    let s50 = sma(&closes, 50);
    let s200 = sma(&closes, 200);
    let sma50_prev = if s50.len() > 20 { s50[s50.len() - 21] } else { None };
    let dcloses: Vec<f64> = d.iter().map(|c| c.close).collect();
    let rsi_series = rsi(&dcloses, 14);
    let reg = regime(&ld, &h4);
    let now = Some(inp.now);
    let weeks = weekly(&ld);
    let sigd = analyze(&d, &AnalyzeOptions { higher: Some(&weeks), interval_ms: Some(Interval::D1.step()), now })
        .map(|s| gate(&s, &inp.reliability, &inp.quality_issues));
    let sig4 = analyze(&h4, &AnalyzeOptions { higher: Some(&d), interval_ms: Some(Interval::H4.step()), now })
        .map(|s| gate(&s, &inp.reliability, &inp.quality_issues));
    let div_d = d.len() > 60 && divergence(&d, &rsi_series, Direction::Down, d.len() as i64 - 1, 60);
    let div_4h = h4.len() > 60 && {
        let c4: Vec<f64> = h4.iter().map(|c| c.close).collect();
        divergence(&h4, &rsi(&c4, 14), Direction::Down, h4.len() as i64 - 1, 60)
    };
    let st = structure(&d, price, inp.symbol, inp.kind, &inp.benchmarks);
    Metrics {
        price,
        st,
        atr_d,
        atr_pct,
        atr_rank,
        sma50: last_some(&s50),
        sma200: last_some(&s200),
        sma50_prev,
        adx_d: last_some(&adx(&d, 14)),
        rsi_d: last_some(&rsi_series),
        regime: reg,
        sigd,
        sig4,
        month: if inp.kind == Kind::Crypto { 30 } else { 21 },
        div_d,
        div_4h,
        d,
        h4,
        ld,
    }
}

fn factor_score(s: Option<&Signal>, name: &str) -> Option<(f64, String)> {
    s?.factors.iter().find(|f| f.name == name).map(|f| (f.score, f.detail.clone()))
}

// ---------- Plan (zone, stop, targets, risk/reward) ----------

struct PlanCalc {
    zone: FibZone,
    /// Candles of the zone's horizon (the swing indexes refer to them).
    hc: Vec<Candle>,
    swing: Swing,
    atr: f64,
    zone_from: f64,
    zone_to: f64,
    stop: f64,
    target1: f64,
    target2: Option<f64>,
    entry: f64,
    rr: f64,
    /// Risk/reward from the current price (None when the price is under the stop).
    rr_now: Option<f64>,
    /// Highest entry giving a risk/reward of 2.
    max_entry: f64,
    /// Lowest low since the high (current candle included) and its index.
    low_since: f64,
    low_idx: usize,
    /// Depth of the pull-back, share of the move (0.382 = 38.2 %).
    depth: f64,
    unit: &'static str,
}

/// The buy zone the plan is built on: medium term (daily candles) first, else long term, else short term.
fn plan_calc(m: &Metrics) -> Option<PlanCalc> {
    let zones = fib_zones(&m.h4, &m.d, &m.ld, Some(m.price), false);
    let order = [Horizon::Medium, Horizon::Long, Horizon::Short];
    for h in order {
        let Some(z) = zones.iter().find(|z| z.horizon == h) else { continue };
        let (Some(band), Some(s)) = (z.zone, z.swing) else { continue };
        if s.trend != SwingTrend::Up || matches!(z.status, ZoneStatus::Downtrend | ZoneStatus::None) {
            continue;
        }
        let hc = match h {
            Horizon::Medium => m.d.clone(),
            Horizon::Short => m.h4.clone(),
            Horizon::Long => weekly(&m.ld),
        };
        if s.high_index >= hc.len() {
            continue;
        }
        let a = last_some(&atr(&hc, 14)).unwrap_or(0.0);
        let last = hc.len() - 1;
        let (mut low_since, mut low_idx) = (s.high, s.high_index);
        for (i, c) in hc.iter().enumerate().skip(s.high_index + 1) {
            if c.low < low_since {
                low_since = c.low;
                low_idx = i;
            }
        }
        // The current price counts too (the candles are closed ones: a fall today is not in them yet).
        if m.price < low_since {
            low_since = m.price;
            low_idx = last;
        }
        let prior_low = hc[s.high_index + 1..last.max(s.high_index + 1)].iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
        let stop = (level(&s, 0.786).min(prior_low) - 0.25 * a).max(s.low * 0.5);
        let target1 = s.high;
        let target2 = z.targets.get(1).copied();
        // Inside the zone: the current price. Otherwise the highest price of the zone that still gives the minimum
        // risk/reward (the plan is then acceptable by construction, unless the stop is too far for any price of it).
        let max_entry = (target1 + MIN_RISK_REWARD * stop) / (1.0 + MIN_RISK_REWARD);
        let entry = if m.price <= band.to && m.price > stop { m.price } else { max_entry.min(band.to).max(band.from) };
        let rr = if entry > stop { (target1 - entry) / (entry - stop) } else { 0.0 };
        let rr_now = if m.price > stop { Some(((target1 - m.price) / (m.price - stop)).max(0.0)) } else { None };
        let depth = if s.high > s.low { (s.high - low_since) / (s.high - s.low) } else { 0.0 };
        return Some(PlanCalc {
            zone: z.clone(),
            hc,
            swing: s,
            atr: a,
            zone_from: band.from,
            zone_to: band.to,
            stop,
            target1,
            target2,
            entry,
            rr,
            rr_now,
            max_entry,
            low_since,
            low_idx,
            depth,
            unit: match h {
                Horizon::Medium => "journalière",
                Horizon::Short => "de 4 h",
                Horizon::Long => "hebdomadaire",
            },
        });
    }
    None
}

/// Target 3: the nearest level (2 touches or more) above target 2, capped by the projection target 2 + (target 2 −
/// target 1).
fn target3(p: &PlanCalc, st: &Structure) -> Option<(f64, String)> {
    let t2 = p.target2.filter(|t| *t > p.target1)?;
    let projection = t2 + (t2 - p.target1);
    let level = st.levels.iter().filter(|l| l.price > t2 * 1.001 && l.price < projection).min_by(|a, b| a.price.total_cmp(&b.price));
    Some(match level {
        Some(l) => (l.price, format!("Niveau touché {} fois au-dessus de l'objectif 2", l.touches)),
        None => (projection, "Projection : objectif 2 + (objectif 2 − objectif 1), aucun niveau touché avant".into()),
    })
}

fn plan_out(p: &PlanCalc, st: &Structure) -> Plan {
    let e = p.entry;
    let t3 = target3(p, st);
    Plan {
        zone_from: p.zone_from,
        zone_to: p.zone_to,
        entry: e,
        stop: p.stop,
        target1: p.target1,
        target2: p.target2,
        risk_pct: r1((e - p.stop) / e * 100.0),
        reward1_pct: r1((p.target1 - e) / e * 100.0),
        reward2_pct: p.target2.map(|t| r1((t - e) / e * 100.0)),
        risk_reward: r2(p.rr),
        min_risk_reward: MIN_RISK_REWARD,
        acceptable: p.rr >= MIN_RISK_REWARD - 1e-9,
        horizon: format!("{} · {}", p.zone.label.to_lowercase(), p.zone.holding),
        target3: t3.as_ref().map(|t| t.0),
        reward3_pct: t3.as_ref().map(|t| r1((t.0 - e) / e * 100.0)),
        target3_source: t3.map(|t| t.1),
    }
}

// ---------- Families ----------

fn family(key: &str, label: &str, score: Option<f64>, summary: String, points: Vec<String>, source: &str) -> Family {
    let score = score.map(|s| round(clamp(s, -100.0, 100.0)));
    Family {
        key: key.into(),
        label: label.into(),
        status: score.map_or(Status::Unavailable, status_of),
        score,
        summary,
        points,
        source: source.into(),
    }
}

fn unavailable(key: &str, label: &str, why: &str) -> Family {
    family(key, label, None, why.into(), vec![], "—")
}

fn candles_source(inp: &DecisionInput) -> String {
    format!("Bougies consensus ({} plateforme(s) en accord sur {})", inp.agreeing, inp.sources)
}

fn trend_family(inp: &DecisionInput, m: &Metrics) -> Family {
    let Some(s200) = m.sma200 else {
        return unavailable("trend", "Tendance", "Moins de 200 jours d'historique : tendance de fond non mesurable");
    };
    let above = m.price > s200;
    let gap = (m.price / s200 - 1.0) * 100.0;
    let mut score = if above { 30.0 } else { -30.0 };
    let mut points = vec![format!("{} de la moyenne 200 jours ({}, {})", if above { "Au-dessus" } else { "En dessous" }, usd(s200), signed(gap))];
    if let Some(s50) = m.sma50 {
        score += if s50 > s200 { 25.0 } else { -25.0 };
        points.push(format!("Moyenne 50 jours {} de la 200 jours ({})", if s50 > s200 { "au-dessus" } else { "en dessous" }, usd(s50)));
        if let Some(prev) = m.sma50_prev {
            let slope = (s50 / prev - 1.0) * 100.0;
            score += if slope > 0.0 { 15.0 } else { -15.0 };
            points.push(format!("Moyenne 50 jours {} sur un mois", signed(slope)));
        }
    }
    score += match m.regime.trend {
        Trend::Up => 30.0 * (0.5 + m.regime.strength / 200.0),
        Trend::Down => -30.0 * (0.5 + m.regime.strength / 200.0),
        Trend::Range => 0.0,
    };
    points.push(m.regime.text.clone());
    // Ichimoku and Supertrend (daily) agreeing confirm the direction: ±10 points.
    match (m.st.ichimoku.as_ref().map(|i| i.bias), m.st.supertrend.as_ref().map(|s| s.bias)) {
        (Some(Bias::Bullish), Some(Bias::Bullish)) => {
            score += 10.0;
            points.push("Ichimoku et Supertrend haussiers : ils confirment la hausse".into());
        }
        (Some(Bias::Bearish), Some(Bias::Bearish)) => {
            score -= 10.0;
            points.push("Ichimoku et Supertrend baissiers : ils confirment la baisse".into());
        }
        (Some(_), Some(_)) => points.push("Ichimoku et Supertrend pas d'accord : direction à confirmer".into()),
        _ => {}
    }
    // A weak ADX: the direction is there but the trend is not marked, the score is scaled down (×0.6 at ADX ≤ 15).
    if let Some(a) = m.adx_d {
        score *= 0.6 + 0.4 * clamp((a - 15.0) / 15.0, 0.0, 1.0);
        if a < 20.0 {
            points.push(format!("ADX {} : tendance peu marquée", fr(a, 0, 0)));
        }
    }
    let adx = m.adx_d.map(|a| format!(", ADX {}", fr(a, 0, 0))).unwrap_or_default();
    let summary = match m.regime.trend {
        Trend::Up => format!("Tendance de fond haussière{adx}"),
        Trend::Down => format!("Tendance de fond baissière{adx}"),
        Trend::Range if score <= -STATUS_LIMIT => format!("Pas de tendance de fond nette, orientée à la baisse (sous la moyenne 200 jours){adx}"),
        Trend::Range if score >= STATUS_LIMIT => format!("Pas de tendance de fond nette, orientée à la hausse{adx}"),
        Trend::Range => format!("Pas de tendance de fond nette{adx}"),
    };
    family("trend", "Tendance", Some(score), summary, points, &candles_source(inp))
}

fn momentum_family(inp: &DecisionInput, m: &Metrics) -> Family {
    let macd = factor_score(m.sigd.as_ref(), "MACD");
    let rsi_f = factor_score(m.sigd.as_ref(), "RSI");
    let month = change(&m.d, m.price, m.month);
    if macd.is_none() && rsi_f.is_none() && month.is_none() {
        return unavailable("momentum", "Momentum", "Historique journalier trop court");
    }
    let mut score = 0.0;
    let mut parts = vec![];
    let mut points = vec![];
    if let Some((s, detail)) = &macd {
        score += 40.0 * s;
        parts.push(format!("MACD : {}", detail.to_lowercase()));
    }
    if let Some((s, _)) = &rsi_f {
        score += 30.0 * s;
    }
    if let Some(r) = m.rsi_d {
        parts.push(format!("RSI {}", fr(r, 0, 0)));
        if r > 70.0 {
            points.push("RSI en surachat (au-dessus de 70) : hausse déjà étirée".into());
        } else if r < 30.0 {
            points.push("RSI en survente (sous 30)".into());
        }
    }
    if let Some(c) = month {
        score += 30.0 * clamp(c / 15.0, -1.0, 1.0);
        points.insert(0, format!("{} sur 1 mois", signed(c)));
    }
    if let Some(c) = change(&m.d, m.price, m.month * 3) {
        points.insert(1.min(points.len()), format!("{} sur 3 mois", signed(c)));
    }
    family("momentum", "Momentum", Some(score), parts.join(", "), points, &candles_source(inp))
}

fn volume_family(inp: &DecisionInput, m: &Metrics) -> Family {
    let obv = factor_score(m.sigd.as_ref(), "Volume (OBV)");
    let n = m.d.len();
    if n < 21 || m.d[n - 21..].iter().all(|c| c.volume == 0.0) {
        return unavailable("volume", "Volume", "Volume indisponible sur les bougies");
    }
    let recent = &m.d[n - 20..];
    let up: f64 = recent.iter().filter(|c| c.close >= c.open).map(|c| c.volume).sum();
    let down: f64 = recent.iter().filter(|c| c.close < c.open).map(|c| c.volume).sum();
    let ud = if down > 0.0 { up / down } else { 2.0 };
    let avg = mean(&m.d[n - 21..n - 1].iter().map(|c| c.volume).collect::<Vec<_>>());
    let rel = if avg > 0.0 { m.d[n - 1].volume / avg } else { 1.0 };
    let mut score = 40.0 * clamp(ud - 1.0, -1.0, 1.0);
    let mut points = vec![format!("Volume des séances en hausse ÷ séances en baisse (20 j) : {}", ratio(ud))];
    let mut summary = format!("Volume relatif {}", ratio(rel));
    if let Some((s, detail)) = obv {
        score += 60.0 * s;
        summary = format!("{} (OBV), volume relatif {}", detail, ratio(rel));
        points.insert(0, format!("{} : OBV {}", detail, if s >= 0.0 { "en hausse" } else { "en baisse" }));
    }
    family("volume", "Volume", Some(score), summary, points, &candles_source(inp))
}

fn volatility_family(inp: &DecisionInput, m: &Metrics) -> Family {
    let Some(a) = m.atr_pct else { return unavailable("volatility", "Volatilité / risque", "Historique trop court pour mesurer l'ATR") };
    let mut points = vec![];
    let mut score = match m.atr_rank {
        Some(r) => (50.0 - r) * 1.2,
        None => 0.0,
    };
    if a > MAX_ATR_PCT {
        score -= 40.0;
    }
    let mut summary = match m.atr_rank {
        Some(r) if r >= 50.0 => format!("ATR {} par jour, au-dessus de {} % de son histoire", pct(a), fr(r, 0, 0)),
        Some(r) => format!("ATR {} par jour, sous {} % de son histoire", pct(a), fr(100.0 - r, 0, 0)),
        None => format!("ATR {} par jour", pct(a)),
    };
    if let Some(g) = inp.guard {
        // A shock measured now outweighs a calm history: never favourable while the guard sees agitation.
        match g.shock.level {
            ShockLevel::Shock => {
                score = (score - 40.0).min(-40.0);
                summary = format!("Choc en cours (garde-fou {}/100), {}", fr(g.shock.score, 0, 0), summary);
                points.push(format!("Garde-fou : choc en cours (score {}/100)", fr(g.shock.score, 0, 0)));
            }
            ShockLevel::Agitated => {
                score = (score - 20.0).min(0.0);
                summary = format!("Marché agité (garde-fou {}/100), {}", fr(g.shock.score, 0, 0), summary);
                points.push(format!("Garde-fou : marché agité (score {}/100)", fr(g.shock.score, 0, 0)));
            }
            ShockLevel::Calm => points.push(format!("Garde-fou : marché calme (score de choc {}/100)", fr(g.shock.score, 0, 0))),
        }
        for f in g.shock.factors.iter().filter(|f| f.points > 0.0 && f.code != "macro" && f.code != "macroNews").take(2) {
            points.push(f.text.clone());
        }
    }
    if m.atr_rank.is_some_and(|r| r >= 80.0) {
        points.push("Volatilité élevée : taille de position réduite".into());
    }
    family("volatility", "Volatilité / risque", Some(score), summary, points, &candles_source(inp))
}

fn stock_valuation(f: &StockFundamentals) -> Family {
    let mut comps: Vec<f64> = vec![];
    let mut points = vec![];
    match (f.per, f.eps) {
        (Some(per), _) if per > 0.0 => {
            comps.push(if per < 15.0 {
                0.6
            } else if per < 25.0 {
                0.2
            } else if per < 40.0 {
                -0.2
            } else {
                -0.6
            });
            points.push(format!("PER {}", fr(per, 0, 1)));
        }
        (_, Some(eps)) if eps <= 0.0 => {
            comps.push(-0.5);
            points.push("Pas de bénéfice sur 12 mois : PER non calculable".into());
        }
        _ => {}
    }
    if let Some(peg) = f.peg.filter(|p| *p > 0.0) {
        comps.push(if peg < 1.0 {
            0.6
        } else if peg < 2.0 {
            0.2
        } else if peg < 3.0 {
            -0.2
        } else {
            -0.6
        });
        points.push(format!("PEG {}", fr(peg, 0, 2)));
    }
    if let Some(ev) = f.ev_ebitda.filter(|v| *v > 0.0) {
        comps.push(if ev < 10.0 {
            0.4
        } else if ev < 20.0 {
            0.0
        } else {
            -0.4
        });
        points.push(format!("VE/EBITDA {}", fr(ev, 0, 1)));
    }
    if let Some(y) = f.dividend_yield {
        points.push(format!("Rendement du dividende {}", pct(y)));
    }
    if let Some(ps) = f.ps {
        points.push(format!("Prix ÷ ventes (P/S) {}", fr(ps, 0, 1)));
    }
    // Against its own history: dearer than 80 % of the days → −0.3, cheaper than 80 % → +0.3.
    if let Some(r) = f.valuation_history.as_ref().and_then(|h| h.per.as_ref().or(h.ps.as_ref())) {
        comps.push(grade(r.percentile, &[(80.0, -0.3), (20.0, 0.0)], 0.3));
    }
    if let Some(v) = &f.valuation_verdict {
        points.push(v.clone());
    }
    if comps.is_empty() {
        return family("valuation", "Valorisation", None, "Données de valorisation indisponibles".into(), points, &f.source);
    }
    let score = mean(&comps) * 100.0;
    let summary = format!(
        "{} ({})",
        if score >= STATUS_LIMIT {
            "Valorisation modérée"
        } else if score <= -STATUS_LIMIT {
            "Valorisation exigeante"
        } else {
            "Valorisation dans la moyenne"
        },
        points.iter().take(2).cloned().collect::<Vec<_>>().join(", ")
    );
    points.push(f.sector_note.clone());
    family("valuation", "Valorisation", Some(score), summary, points, &f.source)
}

fn grade(v: f64, steps: &[(f64, f64)], otherwise: f64) -> f64 {
    steps.iter().find(|(limit, _)| v > *limit).map_or(otherwise, |(_, s)| *s)
}

fn stock_fundamentals(f: &StockFundamentals) -> Family {
    let mut comps: Vec<f64> = vec![];
    let mut points = vec![];
    if let Some(g) = f.revenue_growth {
        comps.push(grade(g, &[(10.0, 1.0), (3.0, 0.5), (0.0, 0.0), (-5.0, -0.5)], -1.0));
        points.push(format!("Chiffre d'affaires {} sur un an", signed(g)));
    }
    if let Some(g) = f.eps_growth {
        comps.push(grade(g, &[(15.0, 1.0), (5.0, 0.5), (0.0, 0.0), (-10.0, -0.5)], -1.0));
        points.push(format!("Bénéfice par action {} sur un an", signed(g)));
    }
    if let Some(mg) = f.operating_margin {
        comps.push(grade(mg, &[(20.0, 0.5), (10.0, 0.2), (0.0, 0.0)], -0.7));
        points.push(format!("Marge opérationnelle {}", pct(mg)));
    }
    if let Some(mg) = f.fcf_margin {
        comps.push(grade(mg, &[(15.0, 0.5), (5.0, 0.2), (0.0, 0.0)], -0.7));
        points.push(format!("Marge de flux de trésorerie libre {}", pct(mg)));
    }
    if let Some(nd) = f.net_debt {
        let fcf = f.free_cash_flow.unwrap_or(0.0);
        comps.push(if nd <= 0.0 {
            0.5
        } else if fcf > 0.0 && nd / fcf < 3.0 {
            0.2
        } else {
            -0.5
        });
        points.push(if nd <= 0.0 { format!("Trésorerie nette {}", money(-nd)) } else { format!("Dette nette {}", money(nd)) });
    }
    if let Some(r) = &f.revisions {
        comps.push(grade(r.change_pct, &[(2.0, 0.5), (-2.0, 0.0)], -0.5));
        points.push(format!("Révisions du consensus de BPA sur un mois : {}", signed(r.change_pct)));
    }
    if let Some(s) = f.surprises.first() {
        comps.push(if s.surprise_pct > 0.0 { 0.3 } else { -0.3 });
        points.push(format!("{} : BPA {} vs consensus {} ({})", s.quarter, fr(s.eps, 2, 2), fr(s.consensus, 2, 2), signed(s.surprise_pct)));
    }
    if let Some(sc) = f.share_change {
        points.push(format!("Nombre d'actions {} sur un an{}", signed(sc), if sc < 0.0 { " (rachats)" } else { "" }));
    }
    if let Some(r) = f.roic {
        points.push(format!("Rentabilité du capital investi (ROIC) {}", pct(r)));
    }
    if comps.is_empty() {
        return family("fundamentals", "Fondamentaux", None, "Données financières indisponibles".into(), points, &f.source);
    }
    let score = mean(&comps) * 100.0;
    let summary = format!("{} ({})", f.period, points.iter().take(2).cloned().collect::<Vec<_>>().join(", "));
    family("fundamentals", "Fondamentaux", Some(score), summary, points, &f.source)
}

fn crypto_fundamentals(f: &CryptoFundamentals) -> Family {
    let mut comps: Vec<f64> = vec![];
    let mut points = vec![];
    if let Some(r) = f.mc_fdv {
        comps.push(grade(r, &[(0.9, 0.4), (0.7, 0.1), (0.5, -0.2)], -0.5));
        points.push(format!("Capitalisation ÷ valorisation diluée : {} ({} des jetons déjà en circulation)", fr(r, 2, 2), pct(r * 100.0)));
    } else if let Some(c) = f.circulating_pct {
        comps.push(grade(c, &[(90.0, 0.4), (70.0, 0.1), (50.0, -0.2)], -0.5));
        points.push(format!("Offre en circulation : {} du maximum", pct(c)));
    }
    if let Some(fees) = f.fees30d.filter(|v| *v > 0.0) {
        comps.push(0.2);
        points.push(format!("Frais payés par les utilisateurs sur 30 jours : {}", money(fees)));
    }
    if let Some(tvl) = f.tvl {
        points.push(format!("Valeur déposée (TVL) : {}", money(tvl)));
    }
    if let Some(tx) = f.tx_per_day {
        points.push(format!("{} transactions par jour", fr(tx, 0, 0)));
    }
    if let Some(h) = f.hash_rate {
        points.push(format!("Taux de hachage : {} EH/s", fr(h / 1e18, 0, 0)));
    }
    if let Some(mc) = f.market_cap {
        points.push(format!("Capitalisation : {}", money(mc)));
    }
    points.push(format!("Déblocages de jetons : {}", f.unlocks));
    if comps.is_empty() {
        let summary = if points.len() > 1 { "Chiffres du réseau sans échelle de comparaison" } else { "Données du réseau indisponibles" };
        return family("fundamentals", "Réseau", None, summary.into(), points, &f.source);
    }
    let score = mean(&comps) * 100.0;
    let summary = points.first().cloned().unwrap_or_default();
    family("fundamentals", "Réseau", Some(score), summary, points, &f.source)
}

fn macro_family(inp: &DecisionInput) -> Family {
    let Some(r) = inp.macro_report else { return unavailable("macro", "Macro", "Contexte macro indisponible (sources injoignables)") };
    let mut score = 20.0 - r.score * 1.2;
    let label = match r.level {
        MacroLevel::Calm => "Contexte calme",
        MacroLevel::Tense => "Contexte tendu",
        MacroLevel::High => "Contexte très tendu",
    };
    let mut points: Vec<String> = r.factors.iter().take(3).map(|f| f.text.clone()).collect();
    if let Some(v) = r.values.vix {
        points.push(format!("VIX {} ({} en 5 séances)", fr(v.value, 0, 1), signed(v.change5d)));
    }
    for t in r.themes.iter().take(2) {
        points.push(format!("Actualité : {} ({} titre(s) en 24 h)", t.label, t.count));
    }
    if let Some(e) = inp.macro_evidence {
        if e.samples >= 20.0 && e.lift >= 1.3 && r.level != MacroLevel::Calm {
            score -= 10.0;
        }
        if e.samples >= 20.0 {
            points.push(format!(
                "Sur cet actif, les jours de stress ont été suivis d'une forte baisse {} % du temps (contre {} % en moyenne, {} cas)",
                fr(e.rate, 0, 0),
                fr(e.base, 0, 0),
                fr(e.samples, 0, 0)
            ));
        }
    }
    family(
        "macro",
        "Macro",
        Some(score),
        format!("{label} (score {}/100)", fr(r.score, 0, 0)),
        points,
        "Yahoo Finance (VIX, S&P 500, pétrole, or, dollar, taux), Google Actualités",
    )
}

fn sentiment_family(inp: &DecisionInput) -> Family {
    let s = &inp.market;
    let mut score = 0.0;
    let mut n = 0;
    let mut parts = vec![];
    let mut points = vec![];
    let mut sources = vec![];
    if let Some(v) = s.fear_greed {
        n += 1;
        let (pts, word) = if v >= 75.0 {
            (-35.0, "avidité extrême")
        } else if v > 55.0 {
            (-10.0, "avidité")
        } else if v >= 45.0 {
            (0.0, "neutre")
        } else if v > 25.0 {
            (10.0, "peur")
        } else {
            (30.0, "peur extrême")
        };
        score += pts;
        parts.push(format!("Fear & Greed {} ({word})", fr(v, 0, 0)));
        if v >= 75.0 {
            points.push("Foule déjà très acheteuse : les replis suivent souvent (lecture à contre-courant)".into());
        } else if v <= 25.0 {
            points.push("Peur extrême : historiquement proche des creux (lecture à contre-courant)".into());
        }
        sources.push("alternative.me");
    }
    if let Some(f) = s.funding_rate {
        n += 1;
        score += if f >= 0.0006 {
            -30.0
        } else if f >= 0.0003 {
            -15.0
        } else if f <= -0.0001 {
            15.0
        } else {
            0.0
        };
        points.push(format!("Financement des contrats perpétuels : {} % par 8 h", fr(f * 100.0, 0, 4)));
        sources.push("OKX");
    }
    if let Some(b) = s.social_bullish.filter(|_| s.social_sample >= 20.0) {
        n += 1;
        score += if b >= 85.0 {
            -15.0
        } else if b <= 30.0 {
            10.0
        } else {
            0.0
        };
        points.push(format!("StockTwits : {} % de messages haussiers ({} messages)", fr(b, 0, 0), fr(s.social_sample, 0, 0)));
        sources.push("StockTwits");
    }
    if n == 0 {
        return unavailable("sentiment", "Sentiment", "Aucune mesure de sentiment disponible");
    }
    let summary = if parts.is_empty() { points[0].clone() } else { parts.join(", ") };
    family("sentiment", "Sentiment", Some(score), summary, points, &sources.join(", "))
}

fn news_family(inp: &DecisionInput) -> Family {
    let Some(items) = &inp.market.news else { return unavailable("news", "Actualités", "Actualités indisponibles") };
    let day: Vec<&NewsItem> = items.iter().filter(|n| n.time <= inp.now && n.time >= inp.now - DAY_MS).collect();
    let tone = news_tone(day.iter().copied());
    let total = day.len();
    // Few articles, weak evidence: the tone counts fully from 10 articles in 24 h.
    let mut score = (tone.positive as f64 - tone.negative as f64) / (total.max(3) as f64) * 60.0 * (total as f64 / 10.0).min(1.0);
    let word = if score >= STATUS_LIMIT {
        "ton plutôt positif"
    } else if score <= -STATUS_LIMIT {
        "ton plutôt négatif"
    } else {
        "ton neutre"
    };
    let mut points = vec![format!("{} titre(s) positif(s), {} négatif(s)", tone.positive, tone.negative)];
    if let Some(g) = inp.guard {
        for f in g.shock.factors.iter().filter(|f| f.code == "newsBurst" || f.code == "newsBusy") {
            points.push(f.text.clone());
            score -= 5.0;
        }
    }
    // Examples: the latest headlines that name the asset (the search also returns neighbouring stories).
    let (sym, name) = (inp.symbol.to_lowercase(), inp.name.to_lowercase());
    let mut recent: Vec<&&NewsItem> = day
        .iter()
        .filter(|n| {
            let t = n.title.to_lowercase();
            t.contains(&name) || t.split(|c: char| !c.is_alphanumeric()).any(|w| w == sym)
        })
        .collect();
    recent.sort_by_key(|n| std::cmp::Reverse(n.time));
    for n in recent.iter().take(2) {
        points.push(format!("« {} »", n.title));
    }
    family("news", "Actualités", Some(score), format!("{total} article(s) en 24 h, {word}"), points, "Google Actualités")
}

/// Average daily traded value from the candles (close × volume, 20 sessions): a lower bound for cryptos (one
/// platform's volume), never used to raise a veto, only to clear it.
fn candle_value(m: &Metrics) -> Option<f64> {
    let n = m.d.len();
    if n < 20 {
        return None;
    }
    let v = mean(&m.d[n - 20..].iter().map(|c| c.close * c.volume).collect::<Vec<_>>());
    if v > 0.0 { Some(v) } else { None }
}

fn liquidity_family(inp: &DecisionInput, m: &Metrics) -> Family {
    if let Some(l) = &inp.liquidity {
        let mut comps = vec![];
        let mut parts = vec![];
        if let Some(s) = l.spread_pct {
            comps.push(if s < 0.05 {
                0.8
            } else if s < 0.2 {
                0.3
            } else if s < MAX_SPREAD_PCT {
                -0.3
            } else {
                -1.0
            });
            parts.push(format!("Écart achat/vente {} %", fr(s, 0, 3)));
        }
        if let Some(v) = l.daily_value {
            comps.push(if v >= 1e9 {
                1.0
            } else if v >= 1e8 {
                0.6
            } else if v >= 1e7 {
                0.2
            } else if v >= MIN_DAILY_VALUE {
                -0.3
            } else {
                -1.0
            });
            parts.push(format!("{} échangés par jour", money(v)));
        }
        let points = l.relative_volume.map(|r| vec![format!("Volume du jour : {} fois la moyenne", ratio(r))]).unwrap_or_default();
        if !comps.is_empty() {
            return family("liquidity", "Liquidité", Some(mean(&comps) * 100.0), parts.join(", "), points, &l.source);
        }
    }
    match candle_value(m) {
        Some(v) if v >= 1e8 => family(
            "liquidity",
            "Liquidité",
            Some(40.0),
            format!("Au moins {} échangés par jour", money(v)),
            vec!["Borne basse tirée des bougies (volume de la source principale) ; écart achat/vente non mesuré".into()],
            &candles_source(inp),
        ),
        _ => unavailable("liquidity", "Liquidité", "Données indisponibles (écart achat/vente et volume échangé non mesurés)"),
    }
}

fn families(inp: &DecisionInput, m: &Metrics) -> Vec<Family> {
    let mut out = vec![trend_family(inp, m), momentum_family(inp, m), volume_family(inp, m), volatility_family(inp, m)];
    match (&inp.fundamentals, inp.kind) {
        (Some(Fundamentals::Stock(f)), _) => {
            out.push(stock_valuation(f));
            out.push(stock_fundamentals(f));
        }
        (Some(Fundamentals::Crypto(f)), _) => {
            out.push(unavailable("valuation", "Valorisation", "Pas de valorisation par les flux fiable pour une crypto"));
            out.push(crypto_fundamentals(f));
        }
        (None, Kind::Stock) => {
            out.push(unavailable("valuation", "Valorisation", "Données indisponibles (comptes de la société non chargés)"));
            out.push(unavailable("fundamentals", "Fondamentaux", "Données indisponibles (comptes de la société non chargés)"));
        }
        (None, Kind::Crypto) => {
            out.push(unavailable("valuation", "Valorisation", "Pas de valorisation par les flux fiable pour une crypto"));
            out.push(unavailable("fundamentals", "Réseau", "Données indisponibles (offre, capitalisation, activité du réseau)"));
        }
    }
    out.push(macro_family(inp));
    out.push(sentiment_family(inp));
    out.push(news_family(inp));
    out.push(liquidity_family(inp, m));
    if inp.kind == Kind::Crypto {
        out.push(onchain_family(match &inp.fundamentals {
            Some(Fundamentals::Crypto(f)) => Some(f),
            _ => None,
        }));
    }
    out
}

/// Market-wide liquidity (stablecoins in circulation over 30 days: > +2 % → +0.4, rising → +0.15, falling → −0.15,
/// < −2 % → −0.4; half weight over 7 days) and the project's developer activity (commits over 4 weeks: under 5 on a
/// smart-contract platform → −0.4, 50 or more → +0.2). Exchange flows and active wallets stay uncovered.
fn onchain_family(f: Option<&CryptoFundamentals>) -> Family {
    const NONE: &str = "Pas de source gratuite et vérifiable des flux on-chain (entrées et sorties des plateformes, portefeuilles actifs)";
    let Some(f) = f else { return unavailable("onchain", "On-chain", NONE) };
    let mut comps: Vec<f64> = vec![];
    let mut points = vec![];
    let mut sources = vec![];
    if let Some(s) = &f.stablecoins {
        if let Some(p) = s.change30d_pct {
            comps.push(grade(p, &[(2.0, 0.4), (0.0, 0.15), (-2.0, -0.15)], -0.4));
            points.push(format!(
                "Stablecoins en circulation : {} ({} sur 30 jours{})",
                money(s.total),
                signed(p),
                if p > 0.0 { ", liquidité qui entre sur le marché crypto" } else { ", liquidité qui sort du marché crypto" }
            ));
        }
        if let Some(p) = s.change7d_pct {
            comps.push(grade(p, &[(1.0, 0.2), (0.0, 0.07), (-1.0, -0.07)], -0.2));
            points.push(format!("Stablecoins sur 7 jours : {}", signed(p)));
        }
        sources.push(s.source.as_str());
    }
    if let Some(c) = f.chain_stablecoins.as_ref().and_then(|c| c.change30d_pct.map(|p| (c, p))) {
        points.push(format!("Stablecoins sur le réseau {} : {} ({} sur 30 jours)", c.0.scope, money(c.0.total), signed(c.1)));
    }
    if let Some(d) = &f.dev_activity {
        if let Some(n) = d.commits4w {
            if n < 5.0 && d.smart_contract_platform {
                comps.push(-0.4);
                points
                    .push(format!("Activité de développement quasi nulle : {} commit(s) en 4 semaines pour une plateforme de contrats", fr(n, 0, 0)));
            } else {
                if n >= 50.0 {
                    comps.push(0.2);
                }
                points.push(format!("{} commits en 4 semaines", fr(n, 0, 0)));
            }
        }
        if let Some(n) = d.pull_requests_merged {
            points.push(format!("{} demandes de modification (pull requests) intégrées au total", fr(n, 0, 0)));
        }
        sources.push(d.source.as_str());
    }
    if comps.is_empty() {
        return family("onchain", "On-chain", None, NONE.into(), points, &if sources.is_empty() { "—".to_string() } else { sources.join(", ") });
    }
    points.push(f.not_covered.clone());
    let score = mean(&comps) * 100.0;
    let summary = points.first().cloned().unwrap_or_default();
    family("onchain", "On-chain", Some(score), summary, points, &sources.join(", "))
}

// ---------- Vetoes ----------

fn veto(code: &str, label: &str, active: bool, verifiable: bool, detail: String) -> Veto {
    Veto { code: code.into(), label: label.into(), active: active && verifiable, verifiable, detail }
}

/// Vetoes that keep the asset off-limits for now (AUCUNE POSITION); the others are a matter of timing (ATTENDRE).
fn is_blocking(code: &str) -> bool {
    !matches!(code, "riskReward" | "earnings" | "divergence" | "announcement")
}

static REGULATION: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(
        r"(?i)\b(sec (charges|sues|sued)|lawsuit|sued|sues|bans?|banned|delist(s|ed|ing)?|crackdown|sanctions?|sanctioned|subpoena|indictment|investigation|probe|antitrust|enquête|interdiction|poursuites?)\b",
    )
    .expect("regex")
});

fn vetoes(inp: &DecisionInput, m: &Metrics, p: Option<&PlanCalc>) -> Vec<Veto> {
    let mut v = vec![];
    let na = "Non vérifiable : données indisponibles".to_string();
    // Spread and liquidity.
    match inp.liquidity.as_ref().and_then(|l| l.spread_pct) {
        Some(s) => v.push(veto(
            "spread",
            "Écart achat/vente trop grand",
            s > MAX_SPREAD_PCT,
            true,
            format!("{} % (seuil {} %)", fr(s, 0, 3), fr(MAX_SPREAD_PCT, 0, 1)),
        )),
        None => v.push(veto("spread", "Écart achat/vente trop grand", false, false, na.clone())),
    }
    match (inp.liquidity.as_ref().and_then(|l| l.daily_value), candle_value(m)) {
        (Some(d), _) => v.push(veto(
            "liquidity",
            "Liquidité insuffisante",
            d < MIN_DAILY_VALUE,
            true,
            format!("{} par jour (seuil {})", money(d), money(MIN_DAILY_VALUE)),
        )),
        (None, Some(c)) if c >= MIN_DAILY_VALUE => v.push(veto(
            "liquidity",
            "Liquidité insuffisante",
            false,
            true,
            format!("Au moins {} par jour d'après les bougies (seuil {})", money(c), money(MIN_DAILY_VALUE)),
        )),
        _ => v.push(veto("liquidity", "Liquidité insuffisante", false, false, na.clone())),
    }
    // Volatility.
    match m.atr_pct {
        Some(a) => {
            v.push(veto("volatility", "Volatilité extrême", a > MAX_ATR_PCT, true, format!("ATR {} par jour (seuil {})", pct(a), pct(MAX_ATR_PCT))))
        }
        None => v.push(veto("volatility", "Volatilité extrême", false, false, na.clone())),
    }
    // Major event: shock measured on the asset itself, escalation headlines. No free calendar of announcements.
    match inp.guard {
        Some(g) => {
            let own: f64 = g.shock.factors.iter().filter(|f| f.code != "macro" && f.code != "macroNews").map(|f| f.points).sum();
            let escalation = inp.macro_report.and_then(|r| r.factors.iter().find(|f| f.code == "escalation"));
            // Escalation headlines cannot be checked on history (and a keyword can mislead): they block only when the
            // markets confirm them (macro stress at the "high" level); otherwise they are shown, not blocking.
            let confirmed = inp.macro_report.is_some_and(|r| r.score >= MACRO.high);
            let shock = own >= super::guard::GUARD.shock_level;
            let active = shock || (escalation.is_some() && confirmed);
            let score = fr(own.min(100.0), 0, 0);
            let detail = match escalation {
                Some(e) if confirmed => format!("{} Confirmé par les marchés (stress macro au niveau « très tendu »).", e.text),
                Some(e) => format!(
                    "{} Non confirmé par les marchés (stress macro sous {}/100) : surveillé, pas bloquant{}",
                    e.text,
                    fr(MACRO.high, 0, 0),
                    if shock { format!(" ; secousse mesurée sur l'actif (score {score}/100)") } else { String::new() }
                ),
                None if active => format!("Secousse mesurée sur l'actif (score {score}/100)"),
                None => format!("Aucune secousse ni titre d'escalade (score {}/100)", fr(own.min(100.0), 0, 0)),
            };
            v.push(veto("event", "Événement majeur imminent", active, true, detail));
        }
        None => v.push(veto("event", "Événement majeur imminent", false, false, na.clone())),
    }
    // Scheduled announcements (central banks, inflation, jobs, GDP) within 48 h: the price can jump either way on the
    // figure, whatever the chart says. A matter of timing (wait for the figure), not a reason to stay out.
    let soon = imminent_events(inp);
    let detail = match (&inp.events, soon.first()) {
        (None, _) => "Calendrier des annonces indisponible ou incomplet : non vérifié".to_string(),
        (Some(_), None) => "Aucune annonce majeure (banques centrales, inflation, emploi, PIB) dans les 48 h".to_string(),
        (Some(_), Some(e)) => format!(
            "{}{} : attendre la publication, le prix peut bouger fortement dans un sens ou dans l'autre",
            event_text(e),
            if soon.len() > 1 { format!(" (+{} autre(s))", soon.len() - 1) } else { String::new() }
        ),
    };
    v.push(veto("announcement", "Annonce économique dans les 48 h", !soon.is_empty(), inp.events.is_some(), detail));
    // Earnings (stocks).
    match (inp.kind, &inp.fundamentals) {
        (Kind::Crypto, _) => v.push(veto("earnings", "Résultats imminents", false, true, "Sans objet pour une crypto".into())),
        (Kind::Stock, Some(Fundamentals::Stock(f))) if f.next_earnings.is_some() => {
            let e = f.next_earnings.as_ref().unwrap();
            let days = ((e.date - inp.now) as f64 / DAY_MS as f64).ceil();
            let est = if e.estimated { " (date estimée)" } else { "" };
            let detail = if days >= 0.0 {
                format!("Prochains résultats le {}{est}, dans {} jour(s) (seuil {})", date_fr(e.date), fr(days, 0, 0), fr(EARNINGS_DAYS, 0, 0))
            } else {
                format!("Derniers résultats le {}", date_fr(e.date))
            };
            v.push(veto("earnings", "Résultats imminents", (0.0..=EARNINGS_DAYS).contains(&days), true, detail));
        }
        (Kind::Stock, _) => v.push(veto("earnings", "Résultats imminents", false, false, "Non vérifiable : date des résultats indisponible".into())),
    }
    // Crash without stabilisation.
    let n = m.d.len();
    match (n > 20, m.atr_d) {
        (true, Some(_)) => {
            let before = last_some(&atr(&m.d[..n - 3], 14)).unwrap_or(0.0) / m.d[n - 4].close * 100.0;
            let fall = change(&m.d, m.price, 3).unwrap_or(0.0);
            let limit = -CRASH_ATR * before;
            let crashed = fall <= limit;
            let stabilised = {
                let h = &m.h4;
                if h.len() >= 18 {
                    let w = &h[h.len() - 18..];
                    let (mut lo, mut idx) = (f64::INFINITY, 0);
                    for (i, c) in w.iter().enumerate() {
                        if c.low <= lo {
                            lo = c.low;
                            idx = i;
                        }
                    }
                    idx < 15 && m.price > lo
                } else {
                    false
                }
            };
            let detail = format!(
                "{} en 3 jours (seuil {}){}",
                signed(fall),
                signed(limit),
                if crashed { if stabilised { ", stabilisé depuis 12 h" } else { ", sans stabilisation" } } else { "" }
            );
            v.push(veto("crash", "Chute brutale sans stabilisation", crashed && !stabilised, true, detail));
        }
        _ => v.push(veto("crash", "Chute brutale sans stabilisation", false, false, na.clone())),
    }
    // Abnormal pump: 3-day rise against the asset's own 3-day moves.
    let ld = &m.ld;
    if ld.len() > 120 {
        let r3: Vec<f64> = (3..ld.len() - 1).map(|i| (ld[i].close / ld[i - 3].close).ln()).collect();
        let now = (m.price / ld[ld.len() - 4].close).ln();
        let (mu, sd) = (mean(&r3), std(&r3));
        let z = if sd > 0.0 { (now - mu) / sd } else { 0.0 };
        let limit = ((mu + PUMP_SIGMA * sd).exp() - 1.0) * 100.0;
        v.push(veto(
            "pump",
            "Hausse anormale",
            z >= PUMP_SIGMA && now > 0.0,
            true,
            format!("{} sur 3 jours (seuil : {} écarts-types, soit {})", signed((now.exp() - 1.0) * 100.0), fr(PUMP_SIGMA, 0, 0), signed(limit)),
        ));
    } else {
        v.push(veto("pump", "Hausse anormale", false, false, na.clone()));
    }
    // Artificial volume: huge volume without a price move.
    if n > 21 && m.d[n - 1].volume > 0.0 {
        let mut prev: Vec<f64> = m.d[n - 21..n - 1].iter().map(|c| c.volume).collect();
        prev.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let med = prev[prev.len() / 2];
        let rel = if med > 0.0 { m.d[n - 1].volume / med } else { 0.0 };
        let mv = (m.d[n - 1].close / m.d[n - 2].close - 1.0).abs() * 100.0;
        let quiet = m.atr_pct.is_some_and(|a| mv < 0.5 * a);
        v.push(veto(
            "volumeAnomaly",
            "Volume artificiel",
            rel >= VOLUME_ANOMALY && quiet,
            true,
            format!(
                "Volume du jour : {} fois la médiane de 20 séances pour un mouvement de {} (alerte au-delà de {} fois sans mouvement)",
                ratio(rel),
                pct(mv),
                fr(VOLUME_ANOMALY, 0, 0)
            ),
        ));
    } else {
        v.push(veto("volumeAnomaly", "Volume artificiel", false, false, na.clone()));
    }
    // Regulation: only when headlines give a verifiable signal.
    let reg: Vec<&NewsItem> = inp
        .market
        .news
        .as_ref()
        .map(|items| items.iter().filter(|x| x.time <= inp.now && x.time >= inp.now - DAY_MS && REGULATION.is_match(&x.title)).collect())
        .unwrap_or_default();
    if reg.is_empty() {
        v.push(veto(
            "regulation",
            "Risque réglementaire",
            false,
            false,
            "Non vérifiable : aucune source gratuite des procédures ; aucun titre réglementaire en 24 h".into(),
        ));
    } else {
        v.push(veto(
            "regulation",
            "Risque réglementaire",
            reg.len() >= 2,
            true,
            format!("{} titre(s) réglementaire(s) en 24 h (seuil 2) : « {} »", reg.len(), reg[0].title),
        ));
    }
    // Token unlocks.
    match (inp.kind, &inp.fundamentals) {
        (Kind::Stock, _) => v.push(veto("unlock", "Déblocage de jetons imminent", false, true, "Sans objet pour une action".into())),
        (Kind::Crypto, Some(Fundamentals::Crypto(f))) if f.unlocks.starts_with("Sans objet") => {
            v.push(veto("unlock", "Déblocage de jetons imminent", false, true, f.unlocks.clone()))
        }
        (Kind::Crypto, _) => v.push(veto(
            "unlock",
            "Déblocage de jetons imminent",
            false,
            false,
            "Non vérifiable : aucune source gratuite (calendriers réservés aux offres payantes)".into(),
        )),
    }
    // Divergence.
    if m.d.len() > 60 {
        let detail = match (m.div_d, m.div_4h) {
            (true, _) => "Divergence baissière en journalier : nouveau sommet du prix sans le RSI".to_string(),
            (false, true) => "En 4 h seulement (non bloquante)".into(),
            _ => "Aucune sur 4 h et 1 j".into(),
        };
        v.push(veto("divergence", "Divergence baissière importante", m.div_d, true, detail));
    } else {
        v.push(veto("divergence", "Divergence baissière importante", false, false, na.clone()));
    }
    // Main trend.
    v.push(veto("downtrend", "Tendance principale baissière", m.regime.trend == Trend::Down, m.ld.len() >= 60, m.regime.text.clone()));
    // Risk / reward from the current price.
    match p {
        Some(p) => {
            let detail = match p.rr_now {
                Some(r) => format!(
                    "{} depuis le prix actuel (minimum {}) ; {} atteint en entrant sous {}",
                    ratio(r),
                    fr(MIN_RISK_REWARD, 0, 0),
                    fr(MIN_RISK_REWARD, 0, 0),
                    usd(p.max_entry)
                ),
                None => format!("Prix sous le stop du plan ({})", usd(p.stop)),
            };
            v.push(veto("riskReward", "Rapport gain/risque insuffisant", p.rr_now.is_none_or(|r| r < MIN_RISK_REWARD), true, detail));
        }
        None => v.push(veto("riskReward", "Rapport gain/risque insuffisant", false, false, "Non calculable : aucune zone d'achat tracée".into())),
    }
    // Macro shock.
    match inp.macro_report {
        Some(r) => {
            let vix = r.values.vix.map(|x| x.value);
            let active = r.score >= MACRO.high || vix.is_some_and(|x| x >= MACRO_SHOCK_VIX);
            let word = match r.level {
                MacroLevel::Calm => "Contexte calme",
                MacroLevel::Tense => "Contexte tendu",
                MacroLevel::High => "Contexte très tendu",
            };
            let vix_text = vix.map(|x| format!(", VIX {}", fr(x, 0, 1))).unwrap_or_default();
            v.push(veto(
                "macro",
                "Choc macro",
                active,
                true,
                format!("{word} (score {}/100, choc à partir de {}{vix_text})", fr(r.score, 0, 0), fr(MACRO.high, 0, 0)),
            ));
        }
        None => v.push(veto("macro", "Choc macro", false, false, na.clone())),
    }
    // Reliability.
    let stale = inp.quality_issues.iter().any(|i| i.contains("périmées"));
    let rel = &inp.reliability;
    let active = rel.level == ReliabilityLevel::Low || rel.conflict || stale;
    v.push(veto(
        "reliability",
        "Données peu fiables",
        active,
        true,
        format!(
            "{} ({}/100, {} plateforme(s) en accord sur {}){}",
            rel.level.label(),
            fr(rel.score, 0, 0),
            inp.agreeing,
            inp.sources,
            if rel.conflict {
                " : sources en désaccord"
            } else if stale {
                " : données périmées"
            } else {
                ""
            }
        ),
    ));
    v
}

// ---------- Setup ----------

fn step(label: &str, state: StepState, detail: String) -> Step {
    Step { label: label.into(), state, detail }
}

struct SetupCalc {
    setup: Setup,
    complete: bool,
    in_zone: bool,
}

fn setup(inp: &DecisionInput, m: &Metrics, p: Option<&PlanCalc>) -> SetupCalc {
    use StepState::{No, Ok as Yes, Unknown};
    let mut steps = vec![];
    // 1. Long-term trend.
    steps.push(match m.sma200 {
        Some(s) => {
            let ok = m.price > s && m.regime.trend != Trend::Down && (m.regime.trend == Trend::Up || m.sma50.is_some_and(|f| f > s));
            step(
                "Tendance de long terme positive",
                if ok { Yes } else { No },
                format!(
                    "Prix {} de la moyenne 200 jours ({}), tendance de fond {}",
                    if m.price > s { "au-dessus" } else { "en dessous" },
                    usd(s),
                    match m.regime.trend {
                        Trend::Up => "haussière",
                        Trend::Down => "baissière",
                        Trend::Range => "sans direction",
                    }
                ),
            )
        }
        None => step("Tendance de long terme positive", Unknown, "Moins de 200 jours d'historique".into()),
    });
    let Some(p) = p else {
        let why = match m.regime.trend {
            Trend::Down => "Mouvement baissier en cours : pas de repli à acheter",
            _ => "Pas de mouvement haussier net récent : aucune zone d'achat tracée",
        };
        steps.push(step("Correction en cours", No, why.into()));
        steps.push(step("Retour sur un support (zone d'achat)", No, why.into()));
        steps.push(step("Volume en baisse pendant la correction", Unknown, "Pas de correction mesurable".into()));
        steps.push(step("Signal de retournement", No, "Pas de zone où le chercher".into()));
        steps.push(step("Retour du volume acheteur", Unknown, "Pas de correction mesurable".into()));
        steps.push(step("Confirmation (clôture au-dessus du plus haut de la veille)", No, "Pas de rebond depuis une zone à confirmer".into()));
        steps.push(step("Stop défini", No, "Pas de plan sans zone d'achat".into()));
        steps.push(step("Objectif défini", No, "Pas de plan sans zone d'achat".into()));
        let _ = inp;
        return finish(steps, false);
    };
    let hc = &p.hc;
    let last = hc.len() - 1;
    let s = &p.swing;
    // 2. Correction.
    let corrected = p.depth >= 0.236;
    steps.push(step(
        "Correction en cours",
        if corrected { Yes } else { No },
        format!(
            "Repli de {} depuis le plus haut ({}), soit {} du mouvement (seuil 23,6 %)",
            pct((1.0 - p.low_since / s.high) * 100.0),
            usd(s.high),
            pct(p.depth * 100.0)
        ),
    ));
    // 3. Support.
    let touched = p.low_since <= p.zone_to;
    let holding = m.price > p.stop;
    let near = m.price <= p.zone_to + p.atr;
    // The support is "reached" when the pull-back touched the zone and the price is still within one ATR of it (the
    // rebound may have started); the price itself is "in the zone" only under its top (ZONE D'ACHAT).
    let on_support = touched && holding && near && p.zone.status != ZoneStatus::Broken;
    let in_zone = on_support && m.price <= p.zone_to;
    let detail3 = if p.zone.status == ZoneStatus::Broken || !holding {
        format!("Plus bas du mouvement ou stop cassé ({})", usd(p.stop))
    } else if !touched {
        format!("Zone {} – {} à {} sous le prix", usd(p.zone_from), usd(p.zone_to), pct((1.0 - p.zone_to / m.price) * 100.0))
    } else if !near {
        format!("Rebond déjà parti : prix {} au-dessus du haut de la zone", pct((m.price / p.zone_to - 1.0) * 100.0))
    } else if !in_zone {
        format!(
            "Zone {} – {} touchée (plus bas {}), prix revenu {} au-dessus",
            usd(p.zone_from),
            usd(p.zone_to),
            usd(p.low_since),
            pct((m.price / p.zone_to - 1.0) * 100.0)
        )
    } else {
        format!("Prix dans la zone {} – {} (plus bas du repli {})", usd(p.zone_from), usd(p.zone_to), usd(p.low_since))
    };
    steps.push(step("Retour sur un support (zone d'achat)", if on_support { Yes } else { No }, detail3));
    // 4. Volume falling during the correction.
    let corr: Vec<f64> = hc[s.high_index + 1..].iter().map(|c| c.volume).collect();
    let rise: Vec<f64> = hc[s.low_index..=s.high_index].iter().map(|c| c.volume).collect();
    let (vc, vr) = (mean(&corr), mean(&rise));
    steps.push(if !corrected || corr.len() < 3 || rise.len() < 3 || vr <= 0.0 {
        step("Volume en baisse pendant la correction", Unknown, "Pas de correction mesurable".into())
    } else {
        step(
            "Volume en baisse pendant la correction",
            if vc < vr { Yes } else { No },
            format!("Volume moyen du repli : {} de celui de la hausse", pct(vc / vr * 100.0)),
        )
    });
    // 5. Reversal signal around the low.
    let mut signs: Vec<String> = vec![];
    if touched {
        let from = last.saturating_sub(2).max(s.high_index + 1);
        for i in from..=last {
            let c = &hc[i];
            let body = (c.close - c.open).abs().max(c.close * 1e-6);
            let lower = c.open.min(c.close) - c.low;
            if lower >= 2.0 * body && c.close >= (c.high + c.low) / 2.0 {
                signs.push("marteau (longue mèche basse)".into());
            }
            let pv = &hc[i - 1];
            if pv.close < pv.open && c.close > c.open && c.close >= pv.open && c.open <= pv.close {
                signs.push("avalement haussier".into());
            }
        }
        let closes: Vec<f64> = hc.iter().map(|c| c.close).collect();
        let r = rsi(&closes, 14);
        if (last.saturating_sub(3)..last).any(|i| r[i].is_some_and(|a| a < 30.0) && r[i + 1].is_some_and(|b| b >= 30.0)) {
            signs.push("RSI qui sort de la survente".into());
        }
        if divergence(hc, &r, Direction::Up, last as i64, 60) {
            signs.push("divergence haussière".into());
        }
        if factor_score(m.sig4.as_ref(), "MACD").is_some_and(|(sc, _)| sc >= 1.0) {
            signs.push("croisement haussier du MACD en 4 h".into());
        }
        signs.dedup();
    }
    steps.push(if !touched {
        step("Signal de retournement", No, "Prix pas encore sur la zone".into())
    } else if signs.is_empty() {
        step("Signal de retournement", No, "Aucun (marteau, avalement haussier, RSI qui sort de survente, divergence haussière)".into())
    } else {
        step("Signal de retournement", Yes, signs.join(", "))
    });
    // 6. Buyer volume returning.
    let tail = &hc[last.saturating_sub(2)..=last];
    let strong_up = corrected && vc > 0.0 && tail.iter().any(|c| c.close > c.open && c.volume > 1.2 * vc);
    let obv5: f64 = (last.saturating_sub(4)..=last).filter(|i| *i > 0).map(|i| (hc[i].close - hc[i - 1].close).signum() * hc[i].volume).sum();
    let buyers = strong_up || obv5 > 0.0;
    steps.push(if hc.iter().all(|c| c.volume == 0.0) {
        step("Retour du volume acheteur", Unknown, "Volume indisponible".into())
    } else {
        step(
            "Retour du volume acheteur",
            if buyers { Yes } else { No },
            if strong_up {
                "Séance haussière sur un volume supérieur de 20 % à celui du repli".into()
            } else if obv5 > 0.0 {
                "Volume net acheteur sur 5 bougies (OBV en hausse)".into()
            } else {
                "Volume net vendeur sur 5 bougies (OBV en baisse)".into()
            },
        )
    });
    // 7. Confirmation.
    let confirm = touched
        && (last.saturating_sub(2)..=last).any(|j| j > p.low_idx.max(s.high_index) && j >= 1 && hc[j].close > hc[j - 1].high && m.price > hc[j].low);
    steps.push(step(
        "Confirmation (clôture au-dessus du plus haut de la veille)",
        if confirm { Yes } else { No },
        if confirm {
            format!("Clôture {} au-dessus du plus haut de la bougie précédente", p.unit)
        } else if touched {
            format!("Attendre une clôture {} au-dessus de {}", p.unit, usd(hc[last].high))
        } else {
            "Pas de rebond depuis la zone à confirmer".into()
        },
    ));
    // 8-9. Stop and target.
    steps.push(step("Stop défini", Yes, format!("Sous le 78,6 % du mouvement ou le plus bas du repli : {}", usd(p.stop))));
    steps.push(step(
        "Objectif défini",
        if p.target1 > p.entry { Yes } else { No },
        format!("Ancien plus haut : {}{}", usd(p.target1), p.target2.map(|t| format!(", puis {} (extension 127,2 %)", usd(t))).unwrap_or_default()),
    ));
    finish(steps, in_zone)
}

fn finish(steps: Vec<Step>, in_zone: bool) -> SetupCalc {
    let required = [0, 1, 2, 6, 7, 8];
    let complete = required.iter().all(|i| steps[*i].state == StepState::Ok) && steps.iter().all(|s| s.state != StepState::No);
    let met = steps.iter().filter(|s| s.state == StepState::Ok).count();
    let total = steps.len();
    SetupCalc { setup: Setup { name: "Achat sur repli en tendance haussière".into(), steps, met, total }, complete, in_zone }
}

// ---------- Exposure ----------

fn closes_by_day(c: &[Candle]) -> std::collections::BTreeMap<String, f64> {
    let mut m = std::collections::BTreeMap::new();
    for x in sanitize(c) {
        m.insert(iso_date(x.time), x.close);
    }
    m
}

/// Daily returns over the last `days` shared calendar days, the price carried forward when a market is closed
/// (`alignedReturns` of web/src/engine/holdings.ts).
pub fn aligned_returns(series: &[&[Candle]], days: usize) -> Vec<Vec<f64>> {
    let maps: Vec<_> = series.iter().map(|s| closes_by_day(s)).collect();
    let mut all: Vec<&String> = maps.iter().flat_map(|m| m.keys()).collect();
    all.sort();
    all.dedup();
    let Some(start) = all.iter().position(|d| maps.iter().all(|m| m.keys().next().is_some_and(|k| k <= *d))) else {
        return series.iter().map(|_| vec![]).collect();
    };
    let dates: Vec<&String> = all[start..].iter().rev().take(days + 1).rev().copied().collect();
    maps.iter()
        .map(|m| {
            let mut last = m.range(..=dates[0].clone()).next_back().map(|(_, v)| *v).unwrap_or(0.0);
            let closes: Vec<f64> = dates
                .iter()
                .map(|d| {
                    if let Some(v) = m.get(*d) {
                        last = *v;
                    }
                    last
                })
                .collect();
            closes.windows(2).map(|w| if w[0] > 0.0 { w[1] / w[0] - 1.0 } else { 0.0 }).collect()
        })
        .collect()
}

/// Pearson correlation of the last common returns (at least 10), `correlation` of holdings.ts.
pub fn correlation(a: &[f64], b: &[f64]) -> Option<f64> {
    let n = a.len().min(b.len());
    if n < 10 {
        return None;
    }
    let (x, y) = (&a[a.len() - n..], &b[b.len() - n..]);
    let (mx, my) = (mean(x), mean(y));
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        sxy += (x[i] - mx) * (y[i] - my);
        sxx += (x[i] - mx).powi(2);
        syy += (y[i] - my).powi(2);
    }
    if sxx > 0.0 && syy > 0.0 { Some(sxy / (sxx * syy).sqrt()) } else { None }
}

fn corr_to(factor: &[Candle], c: &[Candle]) -> Option<f64> {
    let r = aligned_returns(&[c, factor], 90);
    correlation(&r[0], &r[1])
}

fn exposure(inp: &DecisionInput, e: &ExposureInput, bullish: bool) -> Exposure {
    let mut weight = 0.0;
    let mut assets = vec![];
    for h in &e.holdings {
        let c = if h.symbol == e.factor_symbol && h.kind == inp.kind { Some(1.0) } else { corr_to(&e.factor_daily, &h.daily) };
        if c.is_some_and(|c| c > EXPOSURE_CORRELATION) {
            weight += h.weight;
            assets.push(h.symbol.clone());
        }
    }
    let own = if inp.symbol == e.factor_symbol {
        Some(1.0)
    } else {
        corr_to(&e.factor_daily, if inp.long.len() > inp.daily.len() { inp.long } else { inp.daily })
    };
    let warning = if weight >= EXPOSURE_WARNING {
        Some(if bullish {
            format!(
                "Signal favorable mais exposition déjà élevée au même facteur de risque ({}) : {} du portefeuille y est corrélé à plus de {}.",
                e.factor,
                pct(weight),
                fr(EXPOSURE_CORRELATION, 1, 1)
            )
        } else {
            format!("{} du portefeuille suit le même facteur de risque ({}).", pct(weight), e.factor)
        })
    } else {
        None
    };
    Exposure { factor: e.factor.clone(), weight: r1(weight), assets, correlation: own.map(r2), warning }
}

// ---------- Position (sell side) ----------

struct PositionCalc {
    position: Position,
    /// Full exit met now (plan's stop, move broken, background trend down).
    defensive_now: Option<String>,
    /// Half exit met now (recent support broken).
    partial_now: Option<String>,
    profit_now: Option<String>,
    macro_now: Option<String>,
    pnl: f64,
    /// Plan's stop (full exit) and recent support (half), when known.
    stop: Option<f64>,
    support: Option<f64>,
}

fn position(inp: &DecisionInput, m: &Metrics, p: Option<&PlanCalc>, fams: &[Family], vetoes: &[Veto], cost: f64) -> PositionCalc {
    let pnl = (m.price / cost - 1.0) * 100.0;
    let atr_d = m.atr_d.unwrap_or(0.0);
    let n = m.d.len();
    let last_close = m.d.last().map(|c| c.close).unwrap_or(m.price);
    // Protective stop: the plan's (under the 78.6 % retracement), else under the 20-day low before today.
    let stop = p.map(|p| p.stop).or_else(|| (n > 21).then(|| m.d[n - 21..n - 1].iter().map(|c| c.low).fold(f64::INFINITY, f64::min) - 0.25 * atr_d));
    let mut exhaustion = vec![];
    if let Some(r) = m.rsi_d.filter(|r| *r >= 70.0) {
        exhaustion.push(format!("RSI {}", fr(r, 0, 0)));
    }
    if m.div_d || m.div_4h {
        exhaustion.push(format!("divergence baissière en {}", if m.div_d { "journalier" } else { "4 h" }));
    }
    if fams.iter().any(|f| f.key == "volume" && f.status == Status::Negative) {
        exhaustion.push("volume vendeur".into());
    }
    if fams.iter().any(|f| f.key == "momentum" && f.status == Status::Negative) {
        exhaustion.push("momentum qui ralentit".into());
    }
    let mut exits = vec![];
    let mut profit_now = None;
    match p {
        Some(p) => {
            let at_t1 = m.price >= p.target1 - 0.25 * atr_d;
            let now1 = pnl > 0.0 && at_t1 && !exhaustion.is_empty();
            if now1 {
                profit_now = Some(format!("Objectif 1 atteint ({}, ancien plus haut) avec {}", usd(p.target1), exhaustion.join(", ")));
            }
            exits.push(Exit {
                kind: ExitKind::Profit,
                share: 20.0,
                trigger: format!(
                    "Objectif 1 ({}, ancien plus haut et résistance) avec essoufflement (RSI au-dessus de 70, divergence ou volume vendeur)",
                    usd(p.target1)
                ),
                price: Some(p.target1),
                now: now1,
            });
            let sell_signal = m.sigd.as_ref().is_some_and(|s| is_sell(s.action));
            let now2 = pnl > 0.0 && (p.target2.is_some_and(|t| m.price >= t) || sell_signal);
            if now2 && profit_now.is_none() {
                profit_now = Some(if sell_signal { "Signal journalier passé vendeur".into() } else { "Objectif 2 atteint".into() });
            }
            exits.push(Exit {
                kind: ExitKind::Profit,
                share: 30.0,
                trigger: format!(
                    "{}ou signal journalier qui passe vendeur",
                    p.target2.map(|t| format!("Objectif 2 ({}, extension 127,2 %) ", usd(t))).unwrap_or_default()
                ),
                price: p.target2,
                now: now2,
            });
        }
        None => {
            let target = cost * 1.25;
            let now1 = pnl >= 25.0 && !exhaustion.is_empty();
            if now1 {
                profit_now = Some(format!("Gain de {} avec {}", signed(pnl), exhaustion.join(", ")));
            }
            exits.push(Exit {
                kind: ExitKind::Profit,
                share: 20.0,
                trigger: "Gain de 25 % avec essoufflement (RSI au-dessus de 70, divergence ou volume vendeur)".into(),
                price: Some(target),
                now: now1,
            });
        }
    }
    let broken = stop.is_some_and(|s| last_close < s) || p.is_some_and(|p| p.zone.status == ZoneStatus::Broken);
    let trend_down = m.regime.trend == Trend::Down;
    // Recent support (lowest low of the 20 sessions before the last one): its break halves the position, before the
    // plan's stop takes the rest.
    let support = (n > 21)
        .then(|| m.d[n - 21..n - 1].iter().map(|c| c.low).fold(f64::INFINITY, f64::min) - 0.25 * atr_d)
        .filter(|s| stop.is_some_and(|full| *s > full));
    let mut partial_now = None;
    if let Some(s) = support {
        let now = last_close < s && !broken;
        if now {
            partial_now = Some(format!("Support récent cassé (clôture {} sous {}, plus bas de 20 séances)", usd(last_close), usd(s)));
        }
        exits.push(Exit {
            kind: ExitKind::Defensive,
            share: 50.0,
            trigger: format!("Clôture sous {} (plus bas des 20 dernières séances) : réduire de moitié", usd(s)),
            price: Some(s),
            now,
        });
    }
    exits.push(Exit {
        kind: ExitKind::Defensive,
        share: 100.0,
        trigger: match stop {
            Some(s) => format!("Clôture sous {} (support cassé) : sortir de ce qui reste", usd(s)),
            None => "Support cassé : sortir de ce qui reste".into(),
        },
        price: stop,
        now: broken,
    });
    exits.push(Exit {
        kind: ExitKind::Defensive,
        share: 100.0,
        trigger: "Tendance de fond qui devient baissière (moyennes 50 et 200 jours) : sortir de ce qui reste".into(),
        price: m.sma200,
        now: trend_down,
    });
    let macro_v = vetoes.iter().find(|v| v.code == "macro" && v.active);
    exits.push(Exit {
        kind: ExitKind::Macro,
        share: 50.0,
        trigger: format!(
            "Choc de marché (VIX au-dessus de {} ou stress macro de {}/100 et plus) : réduire de moitié",
            fr(MACRO_SHOCK_VIX, 0, 0),
            fr(MACRO.high, 0, 0)
        ),
        price: None,
        now: macro_v.is_some(),
    });
    let defensive_now = if broken {
        Some(format!("Support cassé (clôture {} sous {})", usd(last_close), usd(stop.unwrap_or(0.0))))
    } else if trend_down {
        Some("Tendance de fond devenue baissière".into())
    } else {
        None
    };
    let macro_now = macro_v.map(|v| v.detail.clone());
    let advice = if defensive_now.is_some() {
        format!("{} sur la position : scénario invalidé, sortir.", signed(pnl))
    } else if partial_now.is_some() {
        format!("{} sur la position : support récent cassé, réduire de moitié et garder le stop.", signed(pnl))
    } else if macro_now.is_some() {
        format!("{} sur la position : choc de marché, réduire le risque de moitié.", signed(pnl))
    } else if profit_now.is_some() {
        format!("Plus-value de {} : prendre une partie des gains et protéger le reste.", pct(pnl))
    } else if pnl >= 0.0 {
        format!("Plus-value de {} : conserver, aucune condition de sortie atteinte.", pct(pnl))
    } else {
        format!("Moins-value de {} : conserver tant que le stop tient, aucune condition de sortie atteinte.", pct(-pnl))
    };
    let _ = inp;
    PositionCalc {
        position: Position { cost, pnl_pct: Some(r1(pnl)), advice, exits },
        defensive_now,
        partial_now,
        profit_now,
        macro_now,
        pnl,
        stop,
        support,
    }
}

// ---------- Confidence ----------

/// Confidence ceiling when the signal's own history on the asset lost money or did worse than holding.
pub const TRACK_CAP: f64 = 55.0;
/// Trades needed before the signal's history counts.
const TRACK_MIN_TRADES: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    /// Made money and did at least as well as holding.
    Beats,
    /// Made money but less than holding.
    Lags,
    /// Lost money.
    Loses,
}

fn track_edge(t: Option<&Track>) -> Option<Edge> {
    let t = t.filter(|t| t.trades >= TRACK_MIN_TRADES)?;
    Some(if t.total_return < 0.0 {
        Edge::Loses
    } else if t.total_return < t.buy_and_hold {
        Edge::Lags
    } else {
        Edge::Beats
    })
}

fn track_sentence(t: &Track, edge: Edge) -> String {
    let versus = format!(
        "{} contre {} en simple détention ({} % de trades gagnants sur {}, {}, frais et glissement inclus)",
        signed(t.total_return),
        signed(t.buy_and_hold),
        fr(t.win_rate, 0, 1),
        t.trades,
        t.period
    );
    match edge {
        Edge::Beats => format!("sur cet actif, le signal technique a fait {versus}"),
        Edge::Lags => format!("sur cet actif, le signal technique a fait {versus} : pas d'avance prouvée sur la détention"),
        Edge::Loses => format!("sur cet actif, le signal technique a perdu de l'argent ({versus}) : aucune avance prouvée"),
    }
}

fn confidence(inp: &DecisionInput, fams: &[Family], verdict: Verdict) -> (f64, String, usize, usize) {
    let avail: Vec<&Family> = fams.iter().filter(|f| f.status != Status::Unavailable).collect();
    let pos = avail.iter().filter(|f| f.status == Status::Positive).count();
    let neg = avail.iter().filter(|f| f.status == Status::Negative).count();
    let agreeing = match verdict {
        Verdict::Buy | Verdict::BuyZone => pos,
        Verdict::Sell | Verdict::Trim | Verdict::NoPosition => neg,
        Verdict::Wait => pos.max(neg),
    };
    let n = avail.len().max(1);
    let a = agreeing as f64 / n as f64;
    let avail_ratio = avail.len() as f64 / fams.len().max(1) as f64;
    let r = inp.reliability.score / 100.0 * (0.5 + 0.5 * avail_ratio);
    let (t, track_text) = match (&inp.track, track_edge(inp.track.as_ref())) {
        (Some(t), Some(edge)) => {
            let pf = t.profit_factor.map(|p| clamp((p - 0.5) / 1.5, 0.0, 1.0)).unwrap_or(0.5);
            let base = clamp(0.6 * t.win_rate / 100.0 + 0.4 * pf, 0.0, 1.0);
            // A signal that lost money, or did worse than simply holding, is no evidence of an edge.
            let score = match edge {
                Edge::Beats => base,
                Edge::Lags => base.min(0.35),
                Edge::Loses => base.min(0.2),
            };
            (score, track_sentence(t, edge))
        }
        (Some(t), None) => {
            (0.5, format!("seulement {} trade(s) passé(s) du signal sur cet actif : historique non concluant (compté neutre)", t.trades))
        }
        (None, _) => (0.5, "historique du signal sur cet actif non disponible (compté neutre)".into()),
    };
    // Fewer families measured, less confidence: × 0.6 with none, × 1 with all.
    let mut c = round(100.0 * (0.45 * a + 0.35 * r + 0.20 * t) * (0.6 + 0.4 * avail_ratio));
    if matches!(track_edge(inp.track.as_ref()), Some(Edge::Lags | Edge::Loses)) {
        c = c.min(TRACK_CAP);
    }
    let families = match verdict {
        Verdict::Buy | Verdict::BuyZone => format!("{agreeing} famille(s) favorable(s) sur {} mesurées", avail.len()),
        Verdict::Sell | Verdict::Trim | Verdict::NoPosition => format!("{agreeing} famille(s) défavorable(s) sur {} mesurées", avail.len()),
        Verdict::Wait => format!(
            "lecture {} : {pos} famille(s) favorable(s), {neg} défavorable(s) sur {} mesurées (attendre porte sur le point d'entrée, pas sur l'actif)",
            if a >= 0.6 { "cohérente" } else { "partagée" },
            avail.len()
        ),
    };
    let text = format!(
        "{} ({} sans données) ; {} ({} plateforme(s) en accord sur {}) ; {track_text}.",
        cap(&families),
        fams.len() - avail.len(),
        inp.reliability.level.label().to_lowercase(),
        inp.agreeing,
        inp.sources
    );
    (c, text, agreeing, avail.len())
}

// ---------- Guidance inputs ----------

/// Average daily volume of the 20 sessions before the last one, and the last one ÷ that average (as the volume
/// family); None without volume.
fn volume_ratio(m: &Metrics) -> Option<(f64, f64)> {
    let n = m.d.len();
    if n < 21 || m.d[n - 21..].iter().all(|c| c.volume == 0.0) {
        return None;
    }
    let avg = mean(&m.d[n - 21..n - 1].iter().map(|c| c.volume).collect::<Vec<_>>());
    (avg > 0.0).then(|| (avg, m.d[n - 1].volume / avg))
}

/// What the scenario conditions are checked on: the current price and the last closed daily candle.
fn observed(m: &Metrics) -> Observed {
    let n = m.d.len();
    Observed {
        price: m.price,
        last_close: m.d.last().map(|c| c.close),
        rsi: m.rsi_d,
        adx: m.adx_d,
        volume_ratio: volume_ratio(m).map(|v| r2(v.1)),
        up_day: (n >= 2).then(|| m.d[n - 1].close > m.d[n - 2].close),
    }
}

/// Latest headline of 24 h naming the asset, a toned one (positive or negative) first.
fn top_news(inp: &DecisionInput) -> Option<SnapshotNews> {
    let items = inp.market.news.as_ref()?;
    let (sym, name) = (inp.symbol.to_lowercase(), inp.name.to_lowercase());
    let mut named: Vec<(&NewsItem, &'static str)> = items
        .iter()
        .filter(|n| n.time <= inp.now && n.time >= inp.now - DAY_MS)
        .filter(|n| {
            let t = n.title.to_lowercase();
            t.contains(&name) || t.split(|c: char| !c.is_alphanumeric()).any(|w| w == sym)
        })
        .map(|n| {
            let t = news_tone(std::iter::once(n));
            (
                n,
                if t.negative > 0 {
                    "negative"
                } else if t.positive > 0 {
                    "positive"
                } else {
                    "neutral"
                },
            )
        })
        .collect();
    named.sort_by_key(|(n, tone)| (*tone == "neutral", std::cmp::Reverse(n.time)));
    named.first().map(|(n, tone)| SnapshotNews { title: n.title.clone(), tone: (*tone).into(), time: n.time })
}

fn snapshot(inp: &DecisionInput, m: &Metrics, fams: &[Family], score: Option<f64>) -> DecisionSnapshot {
    let fam = |k: &str| fams.iter().find(|f| f.key == k).and_then(|f| f.score);
    let level = |l: &Option<super::structure::SrLevel>| l.as_ref().map(|l| SnapshotLevel { price: l.price, touches: l.touches });
    DecisionSnapshot {
        price: (m.price > 0.0).then_some(m.price),
        composite: score,
        families: fams.iter().map(|f| SnapshotFamily { key: f.key.clone(), label: f.label.clone(), score: f.score }).collect(),
        momentum: fam("momentum"),
        relative_volume: volume_ratio(m).map(|v| r2(v.1)),
        rsi: m.rsi_d.map(r1),
        adx: m.adx_d.map(r1),
        nearest_support: level(&m.st.nearest_support),
        nearest_resistance: level(&m.st.nearest_resistance),
        news_score: fam("news"),
        top_news: top_news(inp),
    }
}

// ---------- Decision ----------

/// High-importance macro and central bank announcements from one hour ago to 48 h ahead (the stock's earnings
/// have their own veto).
fn imminent_events<'a>(inp: &'a DecisionInput) -> Vec<&'a CalendarEvent> {
    inp.events
        .iter()
        .flatten()
        .filter(|e| matches!(e.kind, EventKind::Macro | EventKind::CentralBank) && e.importance == Importance::High)
        .filter(|e| e.date >= inp.now - 3_600_000 && e.date <= inp.now + 2 * DAY_MS)
        .collect()
}

/// "Décision de la Fed, États-Unis, le 30/09 à 20:00".
fn event_text(e: &CalendarEvent) -> String {
    let day = e.day.get(8..10).zip(e.day.get(5..7)).map(|(d, m)| format!("{d}/{m}")).unwrap_or_else(|| e.day.clone());
    let country = e.country.as_ref().map(|c| format!(", {c},")).unwrap_or_default();
    let time = match &e.time {
        Some(t) if t.contains(':') => format!(" à {t}"),
        Some(t) => format!(" {t}"),
        None => String::new(),
    };
    format!("{}{country} le {day}{time}", e.title)
}

pub fn decide(inp: &DecisionInput) -> Decision {
    let m = metrics(inp);
    let p = plan_calc(&m);
    let fams = families(inp, &m);
    let vetoes = vetoes(inp, &m, p.as_ref());
    let blocked = vetoes.iter().any(|v| v.active);
    let blocking: Vec<&Veto> = vetoes.iter().filter(|v| v.active && is_blocking(&v.code)).collect();
    let timing: Vec<&Veto> = vetoes.iter().filter(|v| v.active && !is_blocking(&v.code)).collect();
    let sc = setup(inp, &m, p.as_ref());
    let rr_ok = p.as_ref().and_then(|p| p.rr_now).is_some_and(|r| r >= MIN_RISK_REWARD);

    // Buy side.
    let buy_verdict = if sc.complete && !blocked && rr_ok {
        Verdict::Buy
    } else if !blocking.is_empty() {
        Verdict::NoPosition
    } else if sc.in_zone && timing.iter().all(|v| v.code == "riskReward") {
        Verdict::BuyZone
    } else {
        Verdict::Wait
    };
    // Sell side, when held.
    let pos = inp.cost.map(|c| position(inp, &m, p.as_ref(), &fams, &vetoes, c));
    let verdict = match &pos {
        Some(pc) if pc.defensive_now.is_some() => Verdict::Sell,
        Some(pc) if pc.profit_now.is_some() || pc.macro_now.is_some() || pc.partial_now.is_some() => Verdict::Trim,
        Some(_) => Verdict::Wait,
        None => buy_verdict,
    };
    let bullish = matches!(verdict, Verdict::Buy | Verdict::BuyZone);
    let expo = inp.exposure.as_ref().map(|e| exposure(inp, e, bullish));
    let (mut conf, mut conf_text, _, _) = confidence(inp, &fams, verdict);
    if bullish && expo.as_ref().is_some_and(|e| e.warning.is_some()) {
        conf = (conf - 10.0).max(0.0);
    }
    let regime_candles = if inp.long.len() > inp.daily.len() { inp.long } else { inp.daily };
    // « Bot Altim »: only the side (buy or sell) with an out-of-sample edge moves the confidence of a buy-side verdict,
    // by a few points; the validation's cap below still applies. Never changes the verdict or a veto.
    let bot_market =
        inp.benchmarks.iter().find(|b| b.symbol == super::bot::BotGroup::of(inp.kind).market()).map(|b| b.daily.as_slice()).unwrap_or(&[]);
    let bot = bot_view(inp.bot, inp.symbol, inp.kind, regime_candles, bot_market, inp.now);
    if bullish && bot.nudge() != 0.0 {
        conf = (conf + bot.nudge()).clamp(0.0, 100.0);
        conf_text.push_str(&format!(
            " Bot Altim : {} point{} ({}, avantage hors échantillon sur ce côté).",
            if bot.nudge() > 0.0 { format!("+{}", fr(bot.nudge(), 0, 0)) } else { format!("−{}", fr(-bot.nudge(), 0, 0)) },
            if bot.nudge().abs() >= 2.0 { "s" } else { "" },
            bot.nudge_label()
        ));
    }
    // Cross-asset validation of the signal: may lower the confidence, never raise it.
    let evidence = model_evidence(inp.validation, inp.symbol, inp.kind, regime_candles, inp.now);
    let capped = evidence.cap_confidence(conf);
    if capped < conf {
        conf_text.push_str(&format!(
            " Confiance plafonnée à {} (au lieu de {}) : la validation du modèle ne montre pas d'avantage du signal sur cette classe d'actifs.",
            fr(capped, 0, 0),
            fr(conf, 0, 0)
        ));
        conf = capped;
    }
    let level = match verdict {
        Verdict::Buy
            if conf >= 65.0
                && expo.as_ref().is_none_or(|e| e.warning.is_none())
                && track_edge(inp.track.as_ref()) != Some(Edge::Lags)
                && track_edge(inp.track.as_ref()) != Some(Edge::Loses) =>
        {
            Level::Strong
        }
        Verdict::Buy | Verdict::BuyZone => Level::Moderate,
        Verdict::Wait if vetoes.iter().any(|v| v.active && matches!(v.code.as_str(), "volatility" | "crash" | "macro" | "event")) => Level::HighRisk,
        Verdict::Wait => Level::Waiting,
        Verdict::NoPosition if m.regime.trend == Trend::Down => Level::Exit,
        Verdict::NoPosition => Level::HighRisk,
        Verdict::Trim if pos.as_ref().is_some_and(|p| p.profit_now.is_none()) => Level::HighRisk,
        Verdict::Trim => Level::Moderate,
        Verdict::Sell => Level::Exit,
    };

    let plan = p.as_ref().map(|p| plan_out(p, &m.st));
    // Summaries (they never change the verdict).
    let score = synthesis::composite(&fams, m.st.score, inp.score_weights);
    let unreliable = (inp.reliability.level == ReliabilityLevel::Low || inp.reliability.conflict).then(|| {
        format!(
            "Données peu fiables ({}/100{})",
            fr(inp.reliability.score, 0, 0),
            if inp.reliability.conflict { ", sources de prix en désaccord" } else { "" }
        )
    });
    let lost = match (&inp.track, track_edge(inp.track.as_ref())) {
        (Some(t), Some(Edge::Loses)) => Some(format!(
            "Aucune avance prouvée : sur cet actif, le signal a perdu de l'argent ({}, {}, frais inclus)",
            signed(t.total_return),
            t.period
        )),
        _ => None,
    };
    let degraded = synthesis::degraded(&fams, &score, unreliable, lost);
    let (rating, rating_reason) = evidence.cap_rating(synthesis::rating(verdict, level, conf, m.regime.trend == Trend::Down, degraded.active));
    let market_regime = match inp.benchmarks.first() {
        Some(b) => {
            let closes: Vec<f64> = sanitize(&b.daily).iter().map(|c| c.close).collect();
            synthesis::market_regime(inp.macro_report, &b.name, &closes)
        }
        None => synthesis::market_regime(inp.macro_report, "Indice de référence", &[]),
    };
    let horizon = p.as_ref().and_then(|p| {
        let candles = match p.zone.horizon {
            Horizon::Short => PlanCandles::H4,
            Horizon::Medium => PlanCandles::D1,
            Horizon::Long => PlanCandles::W1,
        };
        synthesis::horizon(candles, p.entry, p.target1, p.atr)
    });
    let texts = Texts { inp, m: &m, p: p.as_ref(), fams: &fams, vetoes: &vetoes, sc: &sc, pos: pos.as_ref(), verdict };
    let headline = texts.headline();
    let why_wait = if verdict == Verdict::Buy { vec![] } else { texts.why_wait() };
    let (to_buy, to_sell) = (texts.to_buy(), texts.to_sell());
    let mut scenarios = texts.scenarios();
    let unfolding = guidance::unfolding(&mut scenarios);
    let (mut pros, mut cons) = texts.pros_cons();
    if let Some(w) = expo.as_ref().and_then(|e| e.warning.clone()) {
        cons.insert(0, w);
    }
    if let Some(c) = evidence.con() {
        cons.push(c);
    }
    if let Some((pro, line)) = bot.line(inp.cost.is_some()) {
        if pro { pros.push(line) } else { cons.push(line) }
    }
    if pros.is_empty() {
        pros.push("Aucun argument favorable net parmi les familles mesurées".into());
    }
    if cons.is_empty() {
        cons.push("Aucun point défavorable mesuré, ce qui ne garantit rien pour la suite".into());
    }
    let mut why_not = texts.why_not(conf);
    if let Some(w) = expo.as_ref().and_then(|e| e.warning.clone()) {
        why_not.risks.insert(0, w);
    }
    // Guidance (never changes the verdict): when not to trade, the plan as a ladder, the counter-argument.
    let calendar_earnings: Vec<i64> =
        inp.events.iter().flatten().filter(|e| e.kind == EventKind::Earnings && e.symbol.as_deref() == Some(inp.symbol)).map(|e| e.date).collect();
    let no_trade = guidance::no_trade(&guidance::NoTradeInput {
        kind: inp.kind,
        now: inp.now,
        vetoes: &vetoes,
        earnings: match &inp.fundamentals {
            Some(Fundamentals::Stock(f)) => f.next_earnings.as_ref(),
            _ => None,
        },
        calendar_earnings,
        adx: m.adx_d,
        swing: m.st.market_structure.as_ref().map(|s| s.trend),
        range: m.regime.trend == Trend::Range,
        score: score.value,
        confidence: conf,
        degraded: &degraded,
        market_open: (inp.kind == Kind::Stock).then(|| crate::jstime::us_market_open(inp.now)),
    });
    let action_zones = plan.as_ref().map(|pl| guidance::action_zones(pl, m.price, m.atr_d, m.st.nearest_resistance.as_ref()));
    let events: Option<Vec<CalendarEvent>> =
        inp.events.as_ref().map(|es| es.iter().filter(|e| e.date >= inp.now - 3_600_000 && e.date <= inp.now + 7 * DAY_MS).cloned().collect());
    let vol = volume_ratio(&m);
    let counter_argument = guidance::counter_argument(&guidance::CounterInput {
        pros: &pros,
        cons: &cons,
        families: &fams,
        invalidation: &why_not.invalidation,
        bullish: matches!(verdict, Verdict::Buy | Verdict::BuyZone) || (verdict == Verdict::Wait && pos.is_some()),
        support: m.st.nearest_support.as_ref(),
        resistance: m.st.nearest_resistance.as_ref(),
        avg_volume: vol.map(|v| v.0),
        volume_ratio: vol.map(|v| r2(v.1)),
        volume_unit: if inp.kind == Kind::Stock { "actions" } else { inp.symbol },
        events: events.iter().flatten().filter(|e| guidance::counter_event(e, inp.symbol)).map(event_text).collect(),
    });
    let snapshot = snapshot(inp, &m, &fams, score.value);
    let personal = inp.cost.is_some() || inp.exposure.is_some();
    let disclaimer = if personal {
        let what = match (inp.cost.is_some(), inp.exposure.is_some()) {
            (true, true) => "votre prix d'achat et vos pondérations",
            (true, false) => "votre prix d'achat",
            _ => "vos pondérations",
        };
        format!(
            "Mode personnel : calcul fait avec {what}, transmis pour ce calcul et jamais conservés. Pas un conseil en investissement réglementé ; Altim ne passe aucun ordre et rien ne garantit l'avenir."
        )
    } else {
        "Mode informationnel : données et scénarios observés, pas une recommandation personnalisée. Altim ne passe aucun ordre ; rien ne garantit l'avenir.".to_string()
    };
    Decision {
        symbol: inp.symbol.to_string(),
        kind: inp.kind,
        name: inp.name.to_string(),
        as_of: inp.now,
        price: (m.price > 0.0).then_some(m.price),
        mode: if personal { "personal" } else { "informational" }.into(),
        verdict,
        label: verdict.label().into(),
        level,
        level_label: level.label().into(),
        confidence: conf,
        confidence_text: conf_text,
        headline,
        families: fams.clone(),
        blocked,
        vetoes: vetoes.clone(),
        setup: sc.setup.clone(),
        plan,
        why_wait,
        to_buy,
        to_sell,
        scenarios,
        pros,
        cons,
        why_not,
        fundamentals: inp.fundamentals.clone(),
        liquidity: inp.liquidity.clone(),
        track: inp.track.clone(),
        position: pos.map(|p| p.position),
        exposure: expo,
        sources: sources(inp),
        disclaimer,
        rating,
        rating_label: rating.label().into(),
        rating_reason,
        score,
        degraded,
        market_regime,
        horizon,
        structure: Some(m.st.clone()),
        events,
        no_trade,
        action_zones,
        unfolding,
        counter_argument,
        snapshot,
        model_evidence: evidence,
        bot,
    }
}

fn sources(inp: &DecisionInput) -> Vec<DataSource> {
    let mut s = vec![DataSource {
        name: "Bougies (consensus)".into(),
        ok: inp.agreeing > 0,
        detail: format!("{} plateforme(s) en accord sur {} ; {}", inp.agreeing, inp.sources, inp.reliability.level.label().to_lowercase()),
    }];
    s.push(DataSource {
        name: "Contexte macro".into(),
        ok: inp.macro_report.is_some(),
        detail: if inp.macro_report.is_some() { "Yahoo Finance, Google Actualités".into() } else { "indisponible".into() },
    });
    s.push(DataSource {
        name: "Garde-fou (chocs, retournements)".into(),
        ok: inp.guard.is_some(),
        detail: if inp.guard.is_some() { "calculé sur les bougies 1 h, 4 h et 1 j".into() } else { "indisponible".into() },
    });
    // Same rule as the sentiment family: StockTwits counts from 20 messages.
    let sent = inp.market.fear_greed.is_some()
        || (inp.market.social_bullish.is_some() && inp.market.social_sample >= 20.0)
        || inp.market.funding_rate.is_some();
    s.push(DataSource {
        name: "Sentiment".into(),
        ok: sent,
        detail: if inp.kind == Kind::Crypto { "alternative.me, OKX, StockTwits".into() } else { "StockTwits".into() },
    });
    s.push(DataSource { name: "Actualités".into(), ok: inp.market.news.is_some(), detail: "Google Actualités".into() });
    s.push(match &inp.fundamentals {
        Some(Fundamentals::Stock(f)) => DataSource { name: "Fondamentaux".into(), ok: true, detail: f.source.clone() },
        Some(Fundamentals::Crypto(f)) => DataSource { name: "Réseau et jetons".into(), ok: true, detail: f.source.clone() },
        None => DataSource {
            name: if inp.kind == Kind::Stock { "Fondamentaux" } else { "Réseau et jetons" }.into(),
            ok: false,
            detail: "indisponible".into(),
        },
    });
    s.push(match &inp.liquidity {
        Some(l) => DataSource { name: "Liquidité".into(), ok: true, detail: l.source.clone() },
        None => DataSource { name: "Liquidité".into(), ok: false, detail: "écart achat/vente indisponible".into() },
    });
    s.push(DataSource {
        name: "Historique du signal".into(),
        ok: inp.track.is_some(),
        detail: inp.track.as_ref().map(|t| t.period.clone()).unwrap_or_else(|| "indisponible".into()),
    });
    if inp.kind == Kind::Crypto {
        s.push(DataSource { name: "Déblocages de jetons".into(), ok: false, detail: "source payante uniquement".into() });
    }
    s
}

// ---------- Texts ----------

struct Texts<'a> {
    inp: &'a DecisionInput<'a>,
    m: &'a Metrics,
    p: Option<&'a PlanCalc>,
    fams: &'a [Family],
    vetoes: &'a [Veto],
    sc: &'a SetupCalc,
    pos: Option<&'a PositionCalc>,
    verdict: Verdict,
}

impl Texts<'_> {
    fn trend_word(&self) -> &'static str {
        match self.m.regime.trend {
            Trend::Up => "Tendance de fond haussière",
            Trend::Down => "Tendance de fond baissière",
            Trend::Range => "Pas de tendance de fond nette",
        }
    }

    fn active(&self) -> Vec<&Veto> {
        self.vetoes.iter().filter(|v| v.active).collect()
    }

    fn missing_steps(&self) -> Vec<&Step> {
        self.sc.setup.steps.iter().filter(|s| s.state == StepState::No).collect()
    }

    fn headline(&self) -> String {
        let p = self.p;
        let stop = p.map(|p| usd(p.stop)).unwrap_or_default();
        match self.verdict {
            Verdict::Buy => {
                let p = p.expect("a Buy has a plan");
                format!(
                    "Configuration complète (repli sur la zone {} – {} puis confirmation), aucun veto, rapport gain/risque {} : achat possible avec un stop à {} et un objectif à {}.",
                    usd(p.zone_from),
                    usd(p.zone_to),
                    ratio(p.rr_now.unwrap_or(p.rr)),
                    stop,
                    usd(p.target1)
                ) + match track_edge(self.inp.track.as_ref()) {
                    Some(Edge::Lags | Edge::Loses) => " Attention : sur cet actif, ce type de signal n'a pas battu la simple détention.",
                    _ => "",
                }
            }
            Verdict::BuyZone => {
                let p = p.expect("a BuyZone has a plan");
                let missing = self.missing_steps().iter().map(|s| s.label.to_lowercase()).take(2).collect::<Vec<_>>().join(", ");
                format!(
                    "Prix dans la zone d'achat {} – {}, mais {} : patience ou ordre limité sous {}, stop à {}.",
                    usd(p.zone_from),
                    usd(p.zone_to),
                    if missing.is_empty() { "rapport gain/risque encore insuffisant".into() } else { format!("il manque : {missing}") },
                    usd(p.max_entry.min(p.zone_to)),
                    stop
                )
            }
            Verdict::Wait if self.pos.is_some() => {
                let pc = self.pos.unwrap();
                format!(
                    "Position à conserver ({} depuis votre achat) : aucune condition de sortie atteinte{}{}{}.",
                    signed(pc.pnl),
                    pc.support.map(|s| format!(", réduire de moitié sous {}", usd(s))).unwrap_or_default(),
                    pc.stop.map(|s| format!(", sortir sous {}", usd(s))).unwrap_or_default(),
                    p.map(|p| format!(", premier objectif {}", usd(p.target1))).unwrap_or_default()
                )
            }
            Verdict::Wait => {
                let mut reasons: Vec<String> = vec![];
                let long_term_ok = self.sc.setup.steps.first().is_none_or(|s| s.state != StepState::No);
                let below_200 = self.m.sma200.filter(|s| self.m.price < *s);
                if !long_term_ok {
                    reasons.push(match below_200 {
                        Some(_) => "prix sous la moyenne 200 jours".into(),
                        None => "tendance de long terme pas encore confirmée (moyenne 50 jours sous la 200 jours)".into(),
                    });
                }
                if let Some(p) = p {
                    if self.m.price > p.zone_to {
                        reasons.push("prix au-dessus de la zone d'achat".into());
                    }
                }
                for v in self.active() {
                    reasons.push(match v.code.as_str() {
                        "riskReward" => format!(
                            "rapport gain/risque insuffisant depuis le prix actuel ({})",
                            p.and_then(|p| p.rr_now).map(ratio).unwrap_or("—".into())
                        ),
                        _ => v.label.to_lowercase(),
                    });
                }
                if reasons.is_empty() {
                    reasons.extend(self.missing_steps().iter().take(2).map(|s| s.label.to_lowercase()));
                }
                reasons.dedup();
                let target = match p {
                    _ if below_200.is_some() && !long_term_ok => format!(
                        "attendre un retour au-dessus de la moyenne 200 jours ({}) avant de chercher un point d'entrée",
                        usd(below_200.unwrap_or_default())
                    ),
                    Some(p) if self.m.price > p.zone_to => format!(
                        "attendre un repli vers {} – {} (entrée sous {} pour un gain/risque de 2)",
                        usd(p.zone_from),
                        usd(p.zone_to),
                        usd(p.max_entry.min(p.zone_to))
                    ),
                    Some(_) => "attendre la confirmation".into(),
                    None => "attendre qu'un mouvement haussier trace une zone d'achat".into(),
                };
                let sep = if self.m.regime.trend == Trend::Range { ", " } else { ", mais " };
                let reasons: Vec<String> = reasons.into_iter().take(3).collect();
                let listed = match reasons.split_last() {
                    Some((last, rest)) if !rest.is_empty() => format!("{} et {last}", rest.join(", ")),
                    _ => reasons.concat(),
                };
                format!("{}{sep}{listed} : {target}.", self.trend_word())
            }
            Verdict::NoPosition => {
                let active = self.active();
                let main = active.iter().find(|v| v.code == "downtrend").or(active.first());
                match main {
                    Some(v) if v.code == "downtrend" => format!(
                        "Tendance de fond baissière : pas de position tant que le prix ne repasse pas durablement au-dessus de la moyenne 200 jours{}.",
                        self.m.sma200.map(|s| format!(" ({})", usd(s))).unwrap_or_default()
                    ),
                    Some(v) => format!("{} ({}) : rester à l'écart tant que ce veto est actif.", v.label, v.detail),
                    None => "Rester à l'écart pour l'instant.".into(),
                }
            }
            Verdict::Trim => {
                let pc = self.pos.expect("a Trim has a position");
                if let Some(why) = &pc.partial_now {
                    return format!(
                        "{why} ({} depuis votre achat) : réduire la position de moitié{}.",
                        signed(pc.pnl),
                        pc.stop.map(|s| format!(", sortir du reste sous {}", usd(s))).unwrap_or_default()
                    );
                }
                match (&pc.profit_now, &pc.macro_now) {
                    (Some(why), _) => format!(
                        "{} ({} depuis votre achat) : alléger 20 %, garder le reste{}.",
                        why,
                        signed(pc.pnl),
                        pc.stop.map(|s| format!(" avec un stop à {}", usd(s))).unwrap_or_default()
                    ),
                    (None, Some(why)) => format!(
                        "Choc de marché ({why}) : réduire la position de moitié{}.",
                        pc.stop.map(|s| format!(", garder un stop à {}", usd(s))).unwrap_or_default()
                    ),
                    _ => "Alléger la position.".into(),
                }
            }
            Verdict::Sell => {
                let pc = self.pos.expect("a Sell has a position");
                format!(
                    "{} : scénario invalidé, sortir de la position ({} depuis votre achat).",
                    pc.defensive_now.clone().unwrap_or_default(),
                    signed(pc.pnl)
                )
            }
        }
    }

    fn why_wait(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        match self.verdict {
            Verdict::Trim => out.push("Position à alléger : pas de renfort maintenant".into()),
            Verdict::Sell => out.push("Scénario invalidé : pas d'achat".into()),
            _ => {}
        }
        if let Some(p) = self.p {
            if self.m.price > p.zone_to {
                out.push(format!("Prix au-dessus de la zone d'achat (repli de {} nécessaire)", pct((1.0 - p.zone_to / self.m.price) * 100.0)));
            }
        } else {
            out.push("Pas de zone d'achat tracée : aucun mouvement haussier net récent".into());
        }
        for v in self.active() {
            out.push(format!("{} : {}", v.label, v.detail));
        }
        for s in self
            .missing_steps()
            .iter()
            .filter(|s| s.label.starts_with("Confirmation") || s.label.starts_with("Signal") || s.label.starts_with("Tendance"))
        {
            out.push(format!("Étape manquante — {} : {}", s.label, s.detail));
        }
        // Structure: a resistance close above the price, a recent fake breakout.
        let st = &self.m.st;
        if let (Some(r), Some(a)) = (&st.nearest_resistance, self.m.atr_d) {
            if r.price - self.m.price <= a {
                out.push(format!(
                    "Résistance proche : {} (touchée {} fois), {} au-dessus du prix, moins d'un ATR journalier",
                    usd(r.price),
                    r.touches,
                    pct(r.distance_pct.max(0.0))
                ));
            }
        }
        if let Some(b) = st.breakout.as_ref().filter(|b| b.kind == BreakoutKind::Fake && b.bias == Bias::Bearish) {
            out.push(b.reading.clone());
        }
        if self.verdict == Verdict::Wait && self.pos.is_some() && self.sc.complete {
            out.push("Configuration d'achat complète, mais la position est déjà détenue : renforcer reste un choix de taille".into());
        }
        out.dedup();
        if out.is_empty() {
            out.push("Configuration d'achat incomplète".into());
        }
        out
    }

    fn to_buy(&self) -> Vec<Condition> {
        let mut out = vec![];
        let m = self.m;
        if m.regime.trend == Trend::Down || m.sma200.is_some_and(|s| m.price < s) {
            if let Some(s) = m.sma200 {
                out.push(Condition { text: format!("Retour durable au-dessus de la moyenne 200 jours ({})", usd(s)), level: Some(s) });
            }
            out.push(Condition { text: "Moyenne 50 jours qui repasse au-dessus de la 200 jours".into(), level: None });
        }
        if let Some(p) = self.p {
            if m.price > p.zone_to {
                out.push(Condition { text: format!("Repli dans la zone {} – {}", usd(p.zone_from), usd(p.zone_to)), level: Some(p.zone_to) });
            }
            if p.max_entry < m.price {
                out.push(Condition {
                    text: format!("Entrée sous {} pour un rapport gain/risque d'au moins {}", usd(p.max_entry), fr(MIN_RISK_REWARD, 0, 0)),
                    level: Some(p.max_entry),
                });
            }
            let confirm = self.sc.setup.steps.get(6).is_some_and(|s| s.state == StepState::Ok);
            if !confirm {
                out.push(Condition { text: format!("Puis une clôture {} au-dessus du plus haut de la veille", p.unit), level: None });
            }
            out.push(Condition {
                text: format!("Ou une cassure au-dessus de {} avec un volume supérieur à 1,5 fois la moyenne", usd(p.target1)),
                level: Some(p.target1),
            });
        }
        for v in self.active().iter().filter(|v| !matches!(v.code.as_str(), "downtrend" | "riskReward")) {
            out.push(Condition {
                text: match v.code.as_str() {
                    "earnings" => "Résultats publiés et digérés par le marché".into(),
                    "macro" => format!("Stress macro sous {}/100 et VIX sous {}", fr(MACRO.high, 0, 0), fr(MACRO_SHOCK_VIX, 0, 0)),
                    "reliability" => "Sources de prix de nouveau concordantes".into(),
                    "crash" => "Stabilisation : plus de nouveau plus bas pendant au moins 12 h".into(),
                    _ => format!("Fin du veto « {} »", v.label.to_lowercase()),
                },
                level: None,
            });
        }
        if out.is_empty() {
            out.push(Condition { text: "Un mouvement haussier net, puis un repli sur sa zone d'achat".into(), level: None });
        }
        out
    }

    fn to_sell(&self) -> Vec<Condition> {
        let mut out = vec![];
        let stop = self.pos.and_then(|p| p.stop).or(self.p.map(|p| p.stop));
        if let Some(s) = self.pos.and_then(|p| p.support) {
            out.push(Condition { text: format!("Clôture sous {} (plus bas de 20 séances) : réduire de moitié", usd(s)), level: Some(s) });
        }
        if let Some(s) = stop {
            out.push(Condition { text: format!("Clôture sous {} (stop / support)", usd(s)), level: Some(s) });
        }
        if self.m.regime.trend != Trend::Down {
            out.push(Condition { text: "Moyenne 50 jours qui passe sous la 200 jours (tendance de fond qui se retourne)".into(), level: None });
        }
        if let Some(p) = self.p {
            if p.target1 > self.m.price {
                out.push(Condition {
                    text: format!("Prise de profit partielle à l'objectif 1 ({}, ancien plus haut)", usd(p.target1)),
                    level: Some(p.target1),
                });
            }
        }
        out.push(Condition { text: format!("Choc de marché : VIX au-dessus de {}", fr(MACRO_SHOCK_VIX, 0, 0)), level: None });
        out
    }

    fn scenarios(&self) -> Vec<Scenario> {
        let m = self.m;
        let o = observed(m);
        // Each scenario's conditions are checked between its support (stop, or the 60-session low) and the level to
        // break (target 1, or the 200-day average / the 60-session high).
        let (low, high) = match self.p {
            Some(p) => (p.stop, p.target1),
            None => {
                let w = &m.d[m.d.len().saturating_sub(60)..];
                let hi = w.iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
                let lo = w.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
                (lo, m.sma200.filter(|s| *s > m.price).unwrap_or(hi))
            }
        };
        let sc = |kind, title: &str, condition: String, consequence: String, level| Scenario {
            kind,
            title: title.into(),
            condition,
            consequence,
            level,
            conditions: guidance::scenario_checks(kind, low, high, &o),
            met: 0,
            unfolding: false,
        };
        match self.p {
            Some(p) => vec![
                sc(
                    ScenarioKind::Bull,
                    "Scénario haussier",
                    format!("Cassure de {} avec du volume", usd(p.target1)),
                    format!("Objectifs {}{}", usd(p.target1), p.target2.map(|t| format!(" puis {}", usd(t))).unwrap_or_default()),
                    Some(p.target1),
                ),
                sc(
                    ScenarioKind::Neutral,
                    "Scénario neutre",
                    format!("Prix entre {} et {}", usd(p.stop), usd(p.target1)),
                    if m.price > p.zone_to {
                        "Attendre le repli dans la zone d'achat".into()
                    } else {
                        "Attendre la confirmation du rebond dans la zone".into()
                    },
                    None,
                ),
                sc(
                    ScenarioKind::Bear,
                    "Scénario baissier",
                    format!("Clôture sous {}", usd(p.stop)),
                    "Scénario invalidé : plus d'achat, attendre un nouveau point bas".into(),
                    Some(p.stop),
                ),
            ],
            None => {
                let n = m.d.len();
                let w = &m.d[n.saturating_sub(60)..];
                let hi = w.iter().map(|c| c.high).fold(f64::NEG_INFINITY, f64::max);
                let lo = w.iter().map(|c| c.low).fold(f64::INFINITY, f64::min);
                let up = m.sma200.filter(|s| *s > m.price).unwrap_or(hi);
                vec![
                    sc(
                        ScenarioKind::Bull,
                        "Scénario haussier",
                        format!("Retour au-dessus de {}", usd(up)),
                        "Fin de la baisse : chercher une zone d'achat sur le prochain repli".into(),
                        Some(up),
                    ),
                    sc(
                        ScenarioKind::Neutral,
                        "Scénario neutre",
                        format!("Prix entre {} et {}", usd(lo), usd(up)),
                        "Pas de position : attendre un mouvement net".into(),
                        None,
                    ),
                    sc(
                        ScenarioKind::Bear,
                        "Scénario baissier",
                        format!("Clôture sous {} (plus bas de 60 séances)", usd(lo)),
                        "La baisse se poursuit : rester à l'écart".into(),
                        Some(lo),
                    ),
                ]
            }
        }
    }

    fn pros_cons(&self) -> (Vec<String>, Vec<String>) {
        let mut pros = vec![];
        let mut cons = vec![];
        for f in self.fams {
            match f.status {
                Status::Positive => pros.push(format!("{} : {}", f.label, f.summary)),
                Status::Negative => cons.push(format!("{} : {}", f.label, f.summary)),
                _ => {}
            }
        }
        if self.sc.setup.met >= 5 {
            pros.push(format!("Configuration d'achat : {} étapes sur {} validées", self.sc.setup.met, self.sc.setup.total));
        }
        if let Some(p) = self.p {
            if let Some((_, e)) = self.inp.zone_evidence.iter().find(|(h, _)| *h == p.zone.horizon) {
                if e.samples >= 10.0 && e.lift >= 1.2 {
                    pros.push(format!(
                        "Sur cet actif, les entrées dans cette zone ont rejoint le plus haut {} % du temps (contre {} % au hasard, {} cas)",
                        fr(e.rate, 0, 0),
                        fr(e.base, 0, 0),
                        fr(e.samples, 0, 0)
                    ));
                }
            }
        }
        // Breakouts and fake breakouts of the structure (daily candles).
        if let Some(b) = self.m.st.breakout.as_ref().filter(|b| matches!(b.kind, BreakoutKind::Confirmed | BreakoutKind::Fake)) {
            match b.bias {
                Bias::Bullish => pros.push(b.reading.clone()),
                Bias::Bearish => cons.push(b.reading.clone()),
                Bias::Neutral => {}
            }
        }
        match self.inp.reliability.level {
            ReliabilityLevel::High => pros.push(format!("Données recoupées ({} plateformes en accord)", self.inp.agreeing)),
            ReliabilityLevel::Low => cons.push("Données peu fiables : sources en désaccord ou incomplètes".into()),
            ReliabilityLevel::Medium => {}
        }
        for v in self.active() {
            let line = format!("Veto : {}", v.label.to_lowercase());
            if !cons.contains(&line) {
                cons.push(line);
            }
        }
        if let Some(p) = self.p {
            if self.m.price > p.zone_to + p.atr {
                cons.push(format!("Repli de {} nécessaire pour revenir dans la zone d'achat", pct((1.0 - p.zone_to / self.m.price) * 100.0)));
            }
        }
        (pros, cons)
    }

    fn why_not(&self, conf: f64) -> WhyNot {
        let mut risks: Vec<String> = vec![];
        // Keeping a position (held, ATTENDRE) is a bet on the asset like buying: what could make it wrong is the same.
        let bullish = matches!(self.verdict, Verdict::Buy | Verdict::BuyZone) || (self.verdict == Verdict::Wait && self.pos.is_some());
        let m = self.m;
        if bullish {
            for f in self.fams.iter().filter(|f| f.status == Status::Negative) {
                risks.push(format!("{} défavorable : {}", f.label, f.summary));
            }
            if let Some(g) = self.inp.guard {
                if g.reversal.direction == Some(Direction::Down) && g.reversal.score >= 30.0 {
                    risks.push(format!("Risque de retournement à la baisse {}/100", fr(g.reversal.score, 0, 0)));
                    risks.extend(g.reversal.factors.iter().filter(|f| f.points > 0.0).take(2).map(|f| f.text.clone()));
                }
            }
            if m.div_4h && !m.div_d {
                risks.push("Divergence baissière en 4 h : essoufflement possible à court terme".into());
            }
            match (&self.inp.track, track_edge(self.inp.track.as_ref())) {
                (Some(t), Some(e @ (Edge::Lags | Edge::Loses))) => risks.push(cap(&track_sentence(t, e))),
                (Some(t), Some(Edge::Beats)) if t.win_rate < 50.0 => risks.push(format!(
                    "Le signal a perdu plus souvent qu'il n'a gagné sur cet actif ({} % de trades gagnants) : ses gains viennent de quelques gros trades",
                    fr(t.win_rate, 0, 0)
                )),
                (_, None) => risks.push("Pas d'historique concluant du signal sur cet actif : aucune preuve qu'il y fonctionne".into()),
                _ => {}
            }
            if let Some(p) = self.p {
                if let Some((_, e)) = self.inp.zone_evidence.iter().find(|(h, _)| *h == p.zone.horizon) {
                    if e.samples >= 5.0 && e.lift < 1.0 {
                        risks.push(format!(
                            "Sur cet actif, cette zone n'a pas fait mieux que le hasard ({} % contre {} %)",
                            fr(e.rate, 0, 0),
                            fr(e.base, 0, 0)
                        ));
                    }
                }
            }
        } else {
            let favourable: Vec<String> = self.fams.iter().filter(|f| f.status == Status::Positive).map(|f| f.label.to_lowercase()).collect();
            if !favourable.is_empty() {
                risks.push(format!("Familles favorables à l'actif : {}", favourable.join(", ")));
            }
            if let Some(p) = self.p {
                if m.price > p.zone_to && m.regime.trend == Trend::Up && self.pos.is_none() {
                    risks.push("En tendance haussière, le repli attendu peut ne jamais venir : attendre peut faire manquer la hausse".into());
                }
            }
            if let Some(r) = m.rsi_d.filter(|r| *r < 30.0) {
                risks.push(format!("Survente (RSI {}) : un rebond peut partir sans prévenir", fr(r, 0, 0)));
            }
            if let Some(g) = self.inp.guard {
                if g.reversal.direction == Some(Direction::Up) && g.reversal.score >= 30.0 {
                    risks.push(format!("Risque de retournement à la hausse {}/100", fr(g.reversal.score, 0, 0)));
                    risks.extend(g.reversal.factors.iter().filter(|f| f.points > 0.0).take(2).map(|f| f.text.clone()));
                }
            }
            if self.inp.market.fear_greed.is_some_and(|v| v <= 25.0) {
                risks.push("Peur extrême : historiquement proche des creux".into());
            }
            if matches!(self.verdict, Verdict::Trim | Verdict::Sell) {
                if let (Some(t), Some(e @ (Edge::Lags | Edge::Loses))) = (&self.inp.track, track_edge(self.inp.track.as_ref())) {
                    risks.push(format!("Suivre les signaux a fait moins bien que conserver : {}", track_sentence(t, e)));
                }
            }
            if self.verdict == Verdict::Trim {
                risks.push("La hausse peut continuer au-delà de l'objectif : garder une partie limite ce regret".into());
            }
            if self.verdict == Verdict::Sell && m.regime.trend == Trend::Up {
                risks.push("La baisse peut n'être qu'un repli dans une tendance de fond haussière".into());
            }
        }
        if let Some(e) = self.inp.macro_report.and_then(|r| r.factors.iter().find(|f| f.code == "escalation")) {
            if !self.vetoes.iter().any(|v| v.code == "event" && v.active) {
                risks.push(format!("{} Un événement géopolitique peut faire sauter tous les niveaux techniques", e.text));
            }
        }
        let unverifiable: Vec<String> = self.vetoes.iter().filter(|v| !v.verifiable).map(|v| v.label.to_lowercase()).collect();
        if !unverifiable.is_empty() {
            risks.push(format!("Non vérifié faute de source : {}", unverifiable.join(", ")));
        }
        let missing = self.fams.iter().filter(|f| f.status == Status::Unavailable).count();
        if risks.is_empty() {
            risks.push(
                "Aucun contre-argument mesuré : la décision repose sur peu de signaux contraires, rester attentif aux niveaux d'invalidation".into(),
            );
        }
        let uncertainty = if conf >= 70.0 && missing * 3 <= self.fams.len() {
            Uncertainty::Low
        } else if conf < 45.0 || missing * 2 > self.fams.len() {
            Uncertainty::High
        } else {
            Uncertainty::Medium
        };
        let mut invalidation = vec![];
        let p = self.p;
        match self.verdict {
            Verdict::Buy | Verdict::BuyZone => {
                if let Some(p) = p {
                    invalidation.push(format!("Clôture sous {} (stop du plan)", usd(p.stop)));
                }
                invalidation.push("Tendance de fond qui passe baissière".into());
                invalidation.push(format!("Choc macro (VIX au-dessus de {})", fr(MACRO_SHOCK_VIX, 0, 0)));
            }
            Verdict::Wait if self.pos.is_some() => {
                if let Some(s) = self.pos.and_then(|p| p.stop) {
                    invalidation.push(format!("Clôture sous {} : sortir (support cassé)", usd(s)));
                }
                invalidation.push("Tendance de fond qui passe baissière : sortir".into());
                if let Some(p) = p {
                    invalidation.push(format!("Objectif 1 ({}) atteint avec essoufflement : alléger", usd(p.target1)));
                }
            }
            Verdict::Wait => {
                if let Some(p) = p {
                    if m.price > p.zone_to {
                        invalidation.push(format!("Cassure au-dessus de {} avec du volume : le repli attendu n'aura pas lieu", usd(p.target1)));
                    }
                    invalidation.push(format!("Clôture sous {} : la zone ne tient pas", usd(p.stop)));
                } else {
                    invalidation.push("Un mouvement haussier net qui trace une zone d'achat".into());
                }
            }
            Verdict::NoPosition => {
                if let Some(s) = m.sma200 {
                    invalidation.push(format!("Retour durable au-dessus de la moyenne 200 jours ({})", usd(s)));
                }
                for v in self.active().iter().filter(|v| v.code != "downtrend").take(2) {
                    invalidation.push(format!("Fin du veto « {} »", v.label.to_lowercase()));
                }
            }
            Verdict::Trim => {
                if let Some(p) = p {
                    invalidation.push(format!("Cassure franche au-dessus de {} avec du volume : la hausse continue", usd(p.target1)));
                }
                invalidation.push("Retour du momentum (RSI qui se détend sans baisse du prix)".into());
            }
            Verdict::Sell => {
                if let Some(s) = self.pos.and_then(|p| p.stop) {
                    invalidation.push(format!("Retour rapide au-dessus de {} (fausse cassure)", usd(s)));
                }
                if let Some(s) = m.sma200 {
                    invalidation.push(format!("Retour durable au-dessus de la moyenne 200 jours ({})", usd(s)));
                }
            }
        }
        if invalidation.is_empty() {
            invalidation.push("Un changement de tendance de fond".into());
        }
        WhyNot { risks, uncertainty, invalidation }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlation_basics() {
        let a: Vec<f64> = (0..30).map(|i| (i as f64 * 0.7).sin()).collect();
        let b: Vec<f64> = a.iter().map(|x| 2.0 * x + 1.0).collect();
        assert!((correlation(&a, &b).unwrap() - 1.0).abs() < 1e-9);
        assert!(correlation(&a[..5], &b[..5]).is_none());
    }

    #[test]
    fn french_formats() {
        assert_eq!(signed(-3.04), "−3 %");
        assert_eq!(money(38e9), "38 Md$");
        assert_eq!(date_fr(1_792_886_400_000), "25/10/2026");
    }
}
