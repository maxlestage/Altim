//! Automatic trading journal, port of web/src/engine/journal.ts (same JSON, "altim.journal.v1"): every simulated
//! purchase (paper) and every real purchase or sale recorded in « Mes avoirs » is written down with WHY it was taken
//! (the decision shown at that moment), the price, the stop, the targets, the signal used and the market conditions.
//! Everything stays on the device (like the holdings).
//!
//! Later, when the journal is opened, each entry is reviewed on the daily candles AFTER the entry (never the entry
//! day itself: its low may be earlier than the purchase, same rule as `paper`), at 3, 10 and 30 calendar days:
//! max favourable / adverse excursion, whether the stop or a target was reached first (a candle reaching both counts
//! as the stop: the worst case, the order inside the day is not known), result against the plan in R (multiples of the
//! risk taken: entry − stop), facts on what worked or not, and whether the entry was coherent with the data available
//! at that moment. The profile groups the results by rating at entry, plan respected or not, and market regime, in R
//! and drawdown (never in raw returns only: a gain obtained by taking more risk does not count more).
//! Pure, deterministic functions (same inputs, same results).
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{enum_str, js_max, js_min, locale_cmp, round_to, slice_utf16};
use crate::engine::decision_types::Decision;
use crate::engine::signal::atr;
use crate::js::{fr as js_fr, number_to_string};
use crate::types::{Candle, Kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JournalSide {
    Buy,
    Sell,
}

/// "paper" | "real".
pub const SOURCE_PAPER: &str = "paper";
pub const SOURCE_REAL: &str = "real";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStepSnap {
    pub label: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SetupSnap {
    pub name: String,
    pub met: f64,
    pub total: f64,
    pub steps: Vec<SetupStepSnap>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSnap {
    pub zone_from: f64,
    pub zone_to: f64,
    pub entry: f64,
    pub stop: f64,
    pub target1: f64,
    #[serde(default)]
    pub target2: Option<f64>,
    pub risk_reward: f64,
    pub min_risk_reward: f64,
    pub acceptable: bool,
}

/// The decision shown when the entry was written (a snapshot: the full decision is not kept).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalDecision {
    pub as_of: f64,
    pub verdict: String,
    pub label: String,
    #[serde(default)]
    pub rating: Option<String>,
    #[serde(default)]
    pub rating_label: Option<String>,
    #[serde(default)]
    pub level_label: String,
    #[serde(default)]
    pub headline: String,
    #[serde(default)]
    pub confidence: f64,
    /// Composite score /100 (None: not computed or older server).
    #[serde(default)]
    pub score: Option<f64>,
    /// First three pros / cons.
    #[serde(default)]
    pub pros: Vec<String>,
    #[serde(default)]
    pub cons: Vec<String>,
    #[serde(default)]
    pub degraded: bool,
    #[serde(default)]
    pub degraded_headline: Option<String>,
    #[serde(default)]
    pub setup: SetupSnap,
    /// Labels of the buy vetoes active at that moment.
    #[serde(default)]
    pub vetoes: Vec<String>,
    #[serde(default)]
    pub plan: Option<PlanSnap>,
    #[serde(default)]
    pub horizon: Option<String>,
}

/// Market conditions at the entry; None = not known (not loaded, source failed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct JournalMarket {
    /// "riskOn" | "neutral" | "riskOff".
    pub regime: Option<String>,
    pub regime_label: Option<String>,
    /// Macro stress /100 and its level (calm / tense / high), from /api/macro.
    pub macro_score: Option<f64>,
    pub macro_level: Option<String>,
    /// Daily ATR (14) in % of the price, on the daily candles closed before the entry.
    pub atr_pct: Option<f64>,
    /// Volume of the last closed day ÷ average of the 20 before (or the decision's liquidity figure).
    pub relative_volume: Option<f64>,
    /// Events of the next 7 days in the decision (economy, central banks, company).
    pub events: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntry {
    pub id: String,
    pub created_at: f64,
    /// "paper" | "real".
    #[serde(default)]
    pub source: String,
    pub side: JournalSide,
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub name: String,
    /// Price of the purchase or sale (paper: fill price with slippage; real: price entered or live price).
    pub price: f64,
    #[serde(default)]
    pub quantity: Option<f64>,
    #[serde(default)]
    pub amount: Option<f64>,
    /// The user's own stop and target(s) (paper order, holding's stop); the decision's plan is kept apart.
    #[serde(default)]
    pub stop: Option<f64>,
    pub targets: Vec<f64>,
    /// "Signal utilisé" in words.
    #[serde(default)]
    pub signal: String,
    /// "Pourquoi je suis entré" (optional).
    #[serde(default)]
    pub note: String,
    /// Paper position or holding line this entry is about.
    #[serde(default)]
    pub ref_id: Option<String>,
    #[serde(default)]
    pub decision: Option<JournalDecision>,
    pub market: JournalMarket,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalState {
    pub version: u32,
    pub entries: Vec<JournalEntry>,
}

pub const DAY: f64 = 86_400_000.0;
pub const REVIEW_DAYS: [u32; 3] = [3, 10, 30];
/// Under this many entries a group's figures are shown with "échantillon trop faible".
pub const MIN_SAMPLE: usize = 5;
/// Beyond this age the decision is not "the one of the moment" anymore.
pub const STALE_DECISION_MS: f64 = 24.0 * 3_600_000.0;
pub const MAX_ENTRIES: usize = 500;
const BUYING: [&str; 2] = ["buy", "buyZone"];

fn round(v: f64, d: i32) -> f64 {
    round_to(v, d)
}
/// `v.toLocaleString("fr-FR", { minimumFractionDigits: 0, maximumFractionDigits: d })`.
fn fr(v: f64, d: usize) -> String {
    js_fr(v, 0, d)
}
fn signed(v: f64, d: usize) -> String {
    let s = if v > 0.0 {
        "+"
    } else if v < 0.0 {
        "−"
    } else {
        ""
    };
    format!("{s}{}", fr(v.abs(), d))
}
fn plural(n: f64, w: &str) -> String {
    format!("{} {w}{}", number_to_string(n), if n > 1.0 { "s" } else { "" })
}

pub fn empty_journal() -> JournalState {
    JournalState { version: 1, entries: vec![] }
}

// ---------- Writing an entry ----------

/// Snapshot of the decision (only what the journal shows and checks). An older server without the rating gives
/// no rating (its label is empty).
pub fn snapshot_decision(d: &Decision) -> JournalDecision {
    let has_rating = !d.rating_label.is_empty();
    JournalDecision {
        as_of: d.as_of as f64,
        verdict: enum_str(&d.verdict),
        label: d.label.clone(),
        rating: has_rating.then(|| enum_str(&d.rating)),
        rating_label: has_rating.then(|| d.rating_label.clone()),
        level_label: d.level_label.clone(),
        headline: d.headline.clone(),
        confidence: d.confidence,
        score: d.score.value,
        pros: d.pros.iter().take(3).cloned().collect(),
        cons: d.cons.iter().take(3).cloned().collect(),
        degraded: d.degraded.active,
        degraded_headline: d.degraded.active.then(|| d.degraded.headline.clone()),
        setup: SetupSnap {
            name: d.setup.name.clone(),
            met: d.setup.met as f64,
            total: d.setup.total as f64,
            steps: d.setup.steps.iter().map(|s| SetupStepSnap { label: s.label.clone(), state: enum_str(&s.state) }).collect(),
        },
        vetoes: d.vetoes.iter().filter(|v| v.active).map(|v| v.label.clone()).collect(),
        plan: d.plan.as_ref().map(|p| PlanSnap {
            zone_from: p.zone_from,
            zone_to: p.zone_to,
            entry: p.entry,
            stop: p.stop,
            target1: p.target1,
            target2: p.target2,
            risk_reward: p.risk_reward,
            min_risk_reward: p.min_risk_reward,
            acceptable: p.acceptable,
        }),
        horizon: d.horizon.as_ref().map(|h| h.label.clone()).or_else(|| d.plan.as_ref().map(|p| p.horizon.clone())),
    }
}

/// "Signal utilisé": the decision, its level and the setup's progress.
pub fn signal_text(d: Option<&JournalDecision>) -> String {
    let Some(d) = d else { return "Aucune décision chargée pour cet actif à ce moment-là".into() };
    let mut parts = vec![
        format!("{} ({})", d.rating_label.as_deref().unwrap_or(&d.label), d.level_label),
        format!("configuration « {} » {}/{}", d.setup.name, number_to_string(d.setup.met), number_to_string(d.setup.total)),
    ];
    if let Some(h) = d.horizon.as_ref().filter(|h| !h.is_empty()) {
        parts.push(format!("horizon {h}"));
    }
    parts.join(" · ")
}

/// Market conditions the decision already carries (regime, relative volume, events).
pub fn market_from_decision(d: Option<&Decision>) -> JournalMarket {
    let Some(d) = d else { return JournalMarket::default() };
    JournalMarket {
        regime: d.market_regime.as_ref().map(|r| enum_str(&r.kind)),
        regime_label: d.market_regime.as_ref().map(|r| r.label.clone()),
        relative_volume: d.liquidity.as_ref().and_then(|l| l.relative_volume),
        events: d.events.as_ref().map(|e| e.len() as f64),
        ..JournalMarket::default()
    }
}

/// ATR % and relative volume on the daily candles CLOSED before `at` (no look-ahead: the entry day's candle is
/// excluded). None when the history is too short.
pub fn market_from_candles(candles: &[Candle], at: f64) -> (Option<f64>, Option<f64>) {
    let mut before: Vec<Candle> = candles.iter().filter(|c| c.time as f64 + DAY <= at && c.close > 0.0).copied().collect();
    before.sort_by_key(|c| c.time);
    let last = before.last();
    let a = if before.len() >= 15 { atr(&before, 14)[before.len() - 1] } else { None };
    // before.slice(-21, -1)
    let n = before.len();
    let prev: Vec<f64> =
        if n == 0 { vec![] } else { before[n.saturating_sub(21)..n - 1].iter().map(|c| c.volume).filter(|v| v.is_finite() && *v > 0.0).collect() };
    let avg = if prev.len() >= 20 { prev.iter().fold(0.0, |s, v| s + v) / prev.len() as f64 } else { 0.0 };
    (a.zip(last).map(|(a, l)| round((a / l.close) * 100.0, 3)), last.filter(|l| avg > 0.0 && l.volume > 0.0).map(|l| round(l.volume / avg, 3)))
}

/// What the screen knows when an entry is written (the decision already loaded, if any).
#[derive(Debug, Clone, Default)]
pub struct NewEntry<'a> {
    pub id: String,
    pub now: f64,
    /// "paper" | "real".
    pub source: &'a str,
    pub side: Option<JournalSide>,
    pub symbol: String,
    pub kind: Option<Kind>,
    pub name: String,
    pub price: f64,
    pub quantity: Option<f64>,
    pub amount: Option<f64>,
    pub stop: Option<f64>,
    pub targets: Vec<Option<f64>>,
    pub note: String,
    pub ref_id: Option<String>,
    pub decision: Option<&'a Decision>,
}

fn ok(v: Option<f64>) -> Option<f64> {
    v.filter(|v| v.is_finite() && *v > 0.0)
}

/// A new entry from what the screen knows at that moment (`createEntry`).
pub fn create_entry(n: &NewEntry) -> JournalEntry {
    let decision = n.decision.map(snapshot_decision);
    let side = n.side.unwrap_or(JournalSide::Buy);
    let buy = side == JournalSide::Buy;
    let mut targets: Vec<f64> = if buy { n.targets.iter().filter_map(|t| ok(*t)).filter(|t| *t > n.price).collect() } else { vec![] };
    targets.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    JournalEntry {
        id: n.id.clone(),
        created_at: n.now,
        source: n.source.to_string(),
        side,
        symbol: n.symbol.clone(),
        kind: n.kind.unwrap_or(Kind::Crypto),
        name: n.name.clone(),
        price: n.price,
        quantity: ok(n.quantity),
        amount: ok(n.amount),
        stop: if buy { ok(n.stop).filter(|s| *s < n.price) } else { None },
        targets,
        signal: signal_text(decision.as_ref()),
        note: slice_utf16(n.note.trim(), 1000),
        ref_id: n.ref_id.clone(),
        decision,
        market: market_from_decision(n.decision),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoldingChange {
    pub side: JournalSide,
    pub quantity: f64,
    pub price: f64,
    pub implied: bool,
}

/// A holding line edited in « Mes avoirs » → the purchase or sale it records (None: no quantity change, or no usable
/// price). Purchase price: the one implied by the new average cost ((q2 × PRU2 − q1 × PRU1) ÷ (q2 − q1)), else the
/// live price; a sale is priced at the live price (the average cost does not change on a sale). `(quantity, averagePrice)`.
pub fn holding_change(before: (f64, f64), after: (f64, f64), live_price: Option<f64>) -> Option<HoldingChange> {
    let dq = after.0 - before.0;
    if dq.is_nan() || dq.abs() <= 1e-12 {
        return None;
    }
    let live = live_price.filter(|v| v.is_finite() && *v > 0.0);
    if dq < 0.0 {
        return live.map(|price| HoldingChange { side: JournalSide::Sell, quantity: -dq, price, implied: false });
    }
    let implied = (after.0 * after.1 - before.0 * before.1) / dq;
    if after.1 != before.1 && implied.is_finite() && implied > 0.0 {
        return Some(HoldingChange { side: JournalSide::Buy, quantity: dq, price: implied, implied: true });
    }
    live.map(|price| HoldingChange { side: JournalSide::Buy, quantity: dq, price, implied: false })
}

pub fn add_entry(s: &JournalState, e: JournalEntry) -> JournalState {
    let mut entries: Vec<JournalEntry> = s.entries.iter().filter(|x| x.id != e.id).cloned().collect();
    entries.push(e);
    entries.sort_by(|a, b| a.created_at.partial_cmp(&b.created_at).unwrap_or(std::cmp::Ordering::Equal));
    let skip = entries.len().saturating_sub(MAX_ENTRIES);
    JournalState { version: 1, entries: entries.into_iter().skip(skip).collect() }
}

/// `Partial<JournalMarket>`: `Some(v)` sets the field (to a value or to unknown), `None` leaves it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarketPatch {
    pub regime: Option<Option<String>>,
    pub regime_label: Option<Option<String>>,
    pub macro_score: Option<Option<f64>>,
    pub macro_level: Option<Option<String>>,
    pub atr_pct: Option<Option<f64>>,
    pub relative_volume: Option<Option<f64>>,
    pub events: Option<Option<f64>>,
}

impl MarketPatch {
    fn apply(&self, m: &JournalMarket) -> JournalMarket {
        let mut m = m.clone();
        if let Some(v) = &self.regime {
            m.regime = v.clone();
        }
        if let Some(v) = &self.regime_label {
            m.regime_label = v.clone();
        }
        if let Some(v) = self.macro_score {
            m.macro_score = v;
        }
        if let Some(v) = &self.macro_level {
            m.macro_level = v.clone();
        }
        if let Some(v) = self.atr_pct {
            m.atr_pct = v;
        }
        if let Some(v) = self.relative_volume {
            m.relative_volume = v;
        }
        if let Some(v) = self.events {
            m.events = v;
        }
        m
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntryPatch {
    pub note: Option<String>,
    pub market: MarketPatch,
    pub decision: Option<JournalDecision>,
}

/// Completes an entry (market data arrived after it was written, the user's note).
pub fn patch_entry(s: &JournalState, id: &str, patch: &EntryPatch) -> JournalState {
    JournalState {
        version: 1,
        entries: s
            .entries
            .iter()
            .map(|e| {
                if e.id != id {
                    return e.clone();
                }
                let decision = patch.decision.clone().or_else(|| e.decision.clone());
                JournalEntry {
                    note: patch.note.as_ref().map(|n| slice_utf16(n.trim(), 1000)).unwrap_or_else(|| e.note.clone()),
                    market: patch.market.apply(&e.market),
                    signal: if patch.decision.is_some() { signal_text(decision.as_ref()) } else { e.signal.clone() },
                    decision,
                    ..e.clone()
                }
            })
            .collect(),
    }
}

pub fn remove_entry(s: &JournalState, id: &str) -> JournalState {
    JournalState { version: 1, entries: s.entries.iter().filter(|e| e.id != id).cloned().collect() }
}

/// `isJournalState`: the checks of the TypeScript on a saved value.
pub fn is_journal_state(v: &Value) -> bool {
    let finite = |e: &Value, k: &str| e.get(k).and_then(Value::as_f64).is_some_and(f64::is_finite);
    v.get("version").and_then(Value::as_f64) == Some(1.0)
        && v.get("entries").and_then(Value::as_array).is_some_and(|es| {
            es.iter().all(|e| {
                e.is_object()
                    && e.get("id").is_some_and(Value::is_string)
                    && e.get("symbol").is_some_and(Value::is_string)
                    && matches!(e.get("kind").and_then(Value::as_str), Some("crypto" | "stock"))
                    && matches!(e.get("side").and_then(Value::as_str), Some("buy" | "sell"))
                    && finite(e, "createdAt")
                    && e.get("price").and_then(Value::as_f64).is_some_and(|p| p.is_finite() && p > 0.0)
                    && e.get("market").is_some_and(|m| !m.is_null() && m != &Value::Bool(false))
                    && e.get("targets").and_then(Value::as_array).is_some_and(|t| t.iter().all(|x| x.as_f64().is_some_and(f64::is_finite)))
            })
        })
}

/// A saved state checked by `is_journal_state` and read (None when it fails either).
pub fn parse_journal_state(v: &Value) -> Option<JournalState> {
    if !is_journal_state(v) {
        return None;
    }
    crate::web::json::from_value(v).ok()
}

/// `JSON.stringify(state)` (integral numbers without ".0").
pub fn journal_json(s: &JournalState) -> String {
    crate::js::to_value(s).to_string()
}

// ---------- Review ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelSource {
    User,
    Plan,
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Levels {
    pub stop: Option<f64>,
    pub stop_source: LevelSource,
    pub targets: Vec<f64>,
    pub targets_source: LevelSource,
}

/// Stop and targets the review uses: the user's own, else the plan of the decision shown at the entry.
pub fn levels_of(e: &JournalEntry) -> Levels {
    let p = e.decision.as_ref().and_then(|d| d.plan.as_ref());
    let stop = e.stop.or_else(|| p.filter(|p| p.stop < e.price).map(|p| p.stop));
    let plan_targets: Vec<f64> = p.map(|p| [Some(p.target1), p.target2].into_iter().flatten().filter(|t| *t > e.price).collect()).unwrap_or_default();
    let targets = if !e.targets.is_empty() { e.targets.clone() } else { plan_targets };
    Levels {
        stop,
        stop_source: if e.stop.is_some() {
            LevelSource::User
        } else if stop.is_some() {
            LevelSource::Plan
        } else {
            LevelSource::None
        },
        targets_source: if !e.targets.is_empty() {
            LevelSource::User
        } else if !targets.is_empty() {
            LevelSource::Plan
        } else {
            LevelSource::None
        },
        targets,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstHit {
    Stop,
    Target1,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStatus {
    /// Not reached yet.
    Pending,
    /// No candle after the entry in the history.
    NoData,
    Ready,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HorizonReview {
    pub days: u32,
    pub status: ReviewStatus,
    pub available_at: f64,
    pub candles: usize,
    pub last_close: Option<f64>,
    /// Price change since the entry (a sale: since the sale), %.
    pub return_pct: Option<f64>,
    /// Max favourable / adverse excursion since the entry, % (highs and lows of the window).
    pub mfe_pct: Option<f64>,
    pub mae_pct: Option<f64>,
    pub mfe_r: Option<f64>,
    pub mae_r: Option<f64>,
    /// First level reached and after how many days (calendar days since the entry).
    pub first: FirstHit,
    pub first_days: Option<f64>,
    /// Target 2 reached before the stop within the window.
    pub target2: bool,
    /// Result following the plan (exit at the first level reached, else the last close), in R; None without stop.
    pub result_r: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub code: &'static str,
    pub label: String,
    pub ok: Option<bool>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClosedInfo {
    pub at: f64,
    pub price: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryReview {
    pub entry: JournalEntry,
    pub levels: Levels,
    /// Risk per unit (entry − stop), None without stop.
    pub risk: Option<f64>,
    /// Planned reward at target 1, in R.
    pub plan_r: Option<f64>,
    pub horizons: Vec<HorizonReview>,
    /// Most advanced ready horizon (the facts are built on it).
    pub latest: Option<HorizonReview>,
    pub worked: Vec<String>,
    pub failed: Vec<String>,
    pub coherence: Vec<Check>,
    /// true: every verifiable check passes; false: at least one fails; None: nothing verifiable.
    pub coherent: Option<bool>,
    /// Plan respected at the entry (same checks).
    pub plan_respected: Option<bool>,
    pub closed: Option<ClosedInfo>,
    pub realized_r: Option<f64>,
}

fn check(code: &'static str, label: impl Into<String>, ok: Option<bool>, detail: impl Into<String>) -> Check {
    Check { code, label: label.into(), ok, detail: detail.into() }
}

/// Checks of the entry against the data available at that moment (a sale is checked against the decision only).
pub fn coherence_checks(e: &JournalEntry, levels: &Levels) -> Vec<Check> {
    let Some(d) = &e.decision else {
        return vec![check(
            "decision",
            "Décision disponible",
            None,
            "Aucune décision chargée pour cet actif à ce moment-là : cohérence non vérifiable.",
        )];
    };
    let mut out = Vec::new();
    let age = e.created_at - d.as_of;
    out.push(if age > STALE_DECISION_MS {
        check(
            "fresh",
            "Décision récente",
            Some(false),
            format!("La décision affichée datait de {} : les données avaient pu changer.", plural(crate::js::round(age / 3_600_000.0), "heure")),
        )
    } else {
        check("fresh", "Décision récente", Some(true), "Décision calculée moins de 24 h avant.")
    });
    if e.side == JournalSide::Sell {
        let selling = ["trim", "sell"].contains(&d.verdict.as_str());
        let detail = if selling {
            format!("Décision « {} » : la vente allait dans son sens.", d.label)
        } else {
            format!("Décision « {} » : la vente allait contre elle.", d.label)
        };
        out.push(check("verdict", "Vente conforme à la décision", Some(selling), detail));
        return out;
    }
    let buying = BUYING.contains(&d.verdict.as_str());
    let detail = if buying { format!("Décision « {} ».", d.label) } else { format!("Achat alors que la décision était « {} ».", d.label) };
    out.push(check("verdict", "Achat conforme à la décision", Some(buying), detail));
    out.push(if !d.vetoes.is_empty() {
        let what = if d.vetoes.len() > 1 { "des interdictions d'achat actives" } else { "une interdiction d'achat active" };
        check("vetoes", "Aucune interdiction d'achat active", Some(false), format!("Entrée malgré {what} : {}.", d.vetoes.join(", ")))
    } else {
        check("vetoes", "Aucune interdiction d'achat active", Some(true), "Aucune interdiction d'achat active.")
    });
    out.push(if d.degraded {
        check(
            "degraded",
            "Signal non dégradé",
            Some(false),
            d.degraded_headline.clone().unwrap_or_else(|| "Le signal était dégradé à l'entrée.".into()),
        )
    } else {
        check("degraded", "Signal non dégradé", Some(true), "Signal complet à l'entrée.")
    });
    match &d.plan {
        None => {
            out.push(check("rr", "Rapport gain / risque ≥ 2", None, "Pas de plan chiffré dans la décision."));
            out.push(check("zone", "Entrée dans la zone d'achat", None, "Pas de zone d'achat dans la décision."));
        }
        Some(p) => {
            let min = if p.min_risk_reward > 0.0 { p.min_risk_reward } else { 2.0 };
            let label = format!("Rapport gain / risque ≥ {}", fr(min, 1));
            out.push(if p.risk_reward >= min {
                check("rr", label, Some(true), format!("Rapport du plan : {}.", fr(p.risk_reward, 1)))
            } else {
                check("rr", label, Some(false), format!("Rapport du plan : {}, sous le minimum de {}.", fr(p.risk_reward, 1), fr(min, 1)))
            });
            let lo = js_min(p.zone_from, p.zone_to);
            let hi = js_max(p.zone_from, p.zone_to);
            let zone = "Entrée dans la zone d'achat";
            out.push(if e.price > hi {
                check(
                    "zone",
                    zone,
                    Some(false),
                    format!("Entrée hors zone d'achat : {} % au-dessus du haut de la zone.", signed((e.price / hi - 1.0) * 100.0, 1)),
                )
            } else if e.price < lo {
                check(
                    "zone",
                    zone,
                    Some(false),
                    format!("Entrée hors zone d'achat : {} % sous la zone (support peut-être cassé).", fr((1.0 - e.price / lo) * 100.0, 1)),
                )
            } else {
                check("zone", zone, Some(true), "Entrée dans la zone d'achat.")
            });
        }
    }
    out.push(if levels.stop.is_none() {
        check("stop", "Stop défini", Some(false), "Aucun stop : risque non borné, résultat en R non mesurable.")
    } else {
        let detail = if levels.stop_source == LevelSource::User {
            "Stop fixé à l'entrée."
        } else {
            "Pas de stop saisi : celui du plan de la décision est utilisé."
        };
        check("stop", "Stop défini", Some(true), detail)
    });
    let label = "Pas contre le régime de marché";
    out.push(match e.market.regime.as_deref() {
        None => check("regime", label, None, "Régime de marché inconnu à l'entrée."),
        Some("riskOff") => check(
            "regime",
            label,
            Some(false),
            format!("Achat en régime {} : à contre-courant du marché.", e.market.regime_label.as_deref().unwrap_or("risk-off")),
        ),
        Some(r) => check("regime", label, Some(true), format!("Régime {}.", e.market.regime_label.as_deref().unwrap_or(r))),
    });
    out
}

fn days_since(t: i64, from: f64) -> f64 {
    js_max(1.0, ((t as f64 - from) / DAY).ceil())
}

fn horizon_review(e: &JournalEntry, days: u32, after: &[Candle], now: f64, levels: &Levels, risk: Option<f64>) -> HorizonReview {
    let available_at = e.created_at + days as f64 * DAY;
    let base = HorizonReview {
        days,
        status: ReviewStatus::Pending,
        available_at,
        candles: 0,
        last_close: None,
        return_pct: None,
        mfe_pct: None,
        mae_pct: None,
        mfe_r: None,
        mae_r: None,
        first: FirstHit::None,
        first_days: None,
        target2: false,
        result_r: None,
    };
    if now < available_at {
        return base;
    }
    let w: Vec<&Candle> = after.iter().filter(|c| c.time as f64 <= available_at).collect();
    let Some(last) = w.last() else { return HorizonReview { status: ReviewStatus::NoData, ..base } };
    // Excursions are measured from the entry: never below it for the favourable one, never above it for the adverse one.
    let hi = w.iter().fold(e.price, |m, c| js_max(m, c.high));
    let lo = w.iter().fold(e.price, |m, c| js_min(m, c.low));
    let r = |p: f64| risk.filter(|r| *r != 0.0).map(|risk| round((p - e.price) / risk, 3));
    let mut first = FirstHit::None;
    let mut first_days = None;
    let mut exit = None;
    let mut target2 = false;
    if e.side == JournalSide::Buy {
        let t1 = levels.targets.first().copied();
        let t2 = levels.targets.get(1).copied();
        for c in &w {
            let stop_hit = levels.stop.is_some_and(|s| c.low <= s);
            if stop_hit {
                if first == FirstHit::None {
                    first = FirstHit::Stop;
                    first_days = Some(days_since(c.time, e.created_at));
                    exit = Some(js_min(levels.stop.unwrap_or(f64::NAN), c.open));
                }
                break;
            }
            if let Some(t1) = t1 {
                if c.high >= t1 && first == FirstHit::None {
                    first = FirstHit::Target1;
                    first_days = Some(days_since(c.time, e.created_at));
                    exit = Some(t1);
                }
            }
            if t2.is_some_and(|t2| c.high >= t2) {
                target2 = true;
            }
        }
    }
    let mark = exit.unwrap_or(last.close);
    HorizonReview {
        status: ReviewStatus::Ready,
        candles: w.len(),
        last_close: Some(last.close),
        return_pct: Some(round((last.close / e.price - 1.0) * 100.0, 3)),
        mfe_pct: Some(round((hi / e.price - 1.0) * 100.0, 3)),
        mae_pct: Some(round((lo / e.price - 1.0) * 100.0, 3)),
        mfe_r: r(hi),
        mae_r: r(lo),
        first,
        first_days,
        target2,
        result_r: if e.side == JournalSide::Buy { r(mark) } else { None },
        ..base
    }
}

/// Review of one entry on its asset's daily candles (any order). `closed`: the position was actually closed (paper
/// trade, recorded sale) — its realized result in R is added.
pub fn review_entry(e: &JournalEntry, candles: &[Candle], now: f64, closed: Option<ClosedInfo>) -> EntryReview {
    let levels = levels_of(e);
    let risk = levels.stop.filter(|s| e.price > *s).map(|s| e.price - s);
    let truthy_risk = risk.filter(|r| *r != 0.0);
    let plan_r = truthy_risk.zip(levels.targets.first()).map(|(risk, t)| round((t - e.price) / risk, 3));
    // Candles starting after the entry (the entry day's own candle may predate it).
    // A history starting after the entry would leave a hole at its start: nothing is computed then.
    let covers = candles.iter().any(|c| c.time as f64 <= e.created_at);
    let mut after: Vec<Candle> =
        if covers { candles.iter().filter(|c| c.time as f64 > e.created_at && c.low > 0.0 && c.high >= c.low).copied().collect() } else { vec![] };
    after.sort_by_key(|c| c.time);
    let horizons: Vec<HorizonReview> = REVIEW_DAYS.iter().map(|d| horizon_review(e, *d, &after, now, &levels, risk)).collect();
    let latest = horizons.iter().rev().find(|h| h.status == ReviewStatus::Ready).cloned();
    let coherence = coherence_checks(e, &levels);
    let verifiable: Vec<&Check> = coherence.iter().filter(|c| c.ok.is_some()).collect();
    let coherent = (!verifiable.is_empty()).then(|| verifiable.iter().all(|c| c.ok == Some(true)));
    let realized_r = closed.as_ref().zip(truthy_risk).map(|(c, risk)| round((c.price - e.price) / risk, 3));
    let (worked, failed) = facts(e, latest.as_ref(), &coherence, &levels, plan_r, closed.as_ref(), realized_r);
    EntryReview {
        entry: e.clone(),
        levels,
        risk,
        plan_r,
        horizons,
        latest,
        worked,
        failed,
        coherence,
        coherent,
        plan_respected: if e.side == JournalSide::Buy { coherent } else { None },
        closed,
        realized_r,
    }
}

fn r_text(r: f64) -> String {
    format!("{} R", signed(r, 1))
}

/// "Qu'est-ce qui a fonctionné ? / Qu'est-ce qui n'a pas fonctionné ?" — facts only, from the review.
fn facts(
    e: &JournalEntry,
    h: Option<&HorizonReview>,
    checks: &[Check],
    levels: &Levels,
    plan_r: Option<f64>,
    closed: Option<&ClosedInfo>,
    realized_r: Option<f64>,
) -> (Vec<String>, Vec<String>) {
    let mut worked = Vec::new();
    let mut failed = Vec::new();
    let failing = |code: &str| checks.iter().find(|c| c.code == code && c.ok == Some(false));
    if e.side == JournalSide::Sell {
        if let Some(h) = h {
            if let Some(ret) = h.return_pct {
                let txt = format!(
                    "{} jours après la vente, le cours est à {} % du prix de vente (plus haut {} %, plus bas {} %).",
                    h.days,
                    signed(ret, 1),
                    signed(h.mfe_pct.unwrap_or(f64::NAN), 1),
                    signed(h.mae_pct.unwrap_or(f64::NAN), 1)
                );
                if ret <= 0.0 { worked.push(txt) } else { failed.push(txt) }
            }
        }
        if let Some(v) = failing("verdict") {
            failed.push(v.detail.clone());
        }
        return (worked, failed);
    }
    if let Some(h) = h {
        let days = number_to_string(h.first_days.unwrap_or(f64::NAN));
        let fd = h.first_days.unwrap_or(f64::NAN);
        let in_r = h.result_r.map(|r| format!(" ({})", r_text(r))).unwrap_or_default();
        match h.first {
            FirstHit::Target1 => {
                worked.push(format!(
                    "L'objectif 1 a été atteint en {} {}{in_r}{}.",
                    days,
                    if fd > 1.0 { "jours" } else { "jour" },
                    if h.target2 { ", puis l'objectif 2" } else { "" }
                ));
            }
            FirstHit::Stop => {
                let why: Vec<&str> = [
                    failing("degraded").map(|_| "le signal était dégradé à l'entrée"),
                    failing("vetoes").map(|_| "des interdictions d'achat étaient actives"),
                    failing("zone").map(|_| "l'entrée était hors zone d'achat"),
                ]
                .into_iter()
                .flatten()
                .collect();
                let because = if why.is_empty() { String::new() } else { format!(" alors que {}", why.join(" et que ")) };
                failed.push(format!("Le stop a été touché en {} {}{in_r}{because}.", days, if fd > 1.0 { "jours" } else { "jour" }));
            }
            FirstHit::None => {
                if let Some(rr) = h.result_r {
                    let txt = format!("Ni stop ni objectif en {} jours : {} au dernier cours.", h.days, r_text(rr));
                    if rr > 0.0 { worked.push(txt) } else { failed.push(txt) }
                } else if let Some(ret) = h.return_pct {
                    let txt = format!("{} % en {} jours (sans stop, résultat en R non mesurable).", signed(ret, 1), h.days);
                    if ret > 0.0 { worked.push(txt) } else { failed.push(txt) }
                }
            }
        }
        if h.first != FirstHit::Target1 && h.mfe_r.is_some_and(|m| m >= 1.0) && h.result_r.unwrap_or(0.0) < 0.0 {
            failed.push(format!(
                "Le cours est monté jusqu'à {} avant de repasser sous l'entrée : le gain latent n'a pas été conservé.",
                r_text(h.mfe_r.unwrap_or(f64::NAN))
            ));
        }
        if let Some(mae) = h.mae_r.filter(|m| h.first != FirstHit::Stop && *m <= -0.8) {
            failed.push(format!("Recul jusqu'à {} : le stop a failli être touché.", r_text(mae)));
        }
        if let Some(mae) = h.mae_r.filter(|m| h.first != FirstHit::Stop && *m > -0.3 && h.result_r.is_some_and(|r| r > 0.0)) {
            worked.push(format!("Recul limité à {} depuis l'entrée.", r_text(mae)));
        }
    }
    if let (Some(c), Some(rr)) = (closed, realized_r) {
        let planned = plan_r.map(|p| format!(" pour {} prévu à l'objectif 1", r_text(p))).unwrap_or_default();
        let txt = format!("Position clôturée ({}) : {} réalisé{planned}.", c.reason, r_text(rr));
        if rr > 0.0 { worked.push(txt) } else { failed.push(txt) }
    }
    for code in ["zone", "rr", "regime", "verdict", "stop"] {
        if let Some(c) = failing(code) {
            failed.push(c.detail.clone());
        }
    }
    let zone = checks.iter().any(|c| c.code == "zone" && c.ok == Some(true));
    if zone && checks.iter().all(|c| c.ok != Some(false)) {
        worked.push("Entrée dans la zone d'achat, plan respecté (aucune interdiction, signal complet, rapport gain / risque suffisant).".into());
    }
    if levels.stop_source == LevelSource::Plan {
        failed.push("Aucun stop saisi : la revue utilise le stop du plan de la décision.".into());
    }
    (worked, failed)
}

// ---------- Profile ----------

#[derive(Debug, Clone, PartialEq)]
pub struct GroupStat {
    pub key: String,
    pub label: String,
    /// Entries reviewed at this horizon.
    pub n: usize,
    /// Entries with a stop (results in R).
    pub with_stop: usize,
    pub low_sample: bool,
    pub avg_r: Option<f64>,
    pub median_r: Option<f64>,
    /// Share of results above 0 R (or above 0 % without stop), %.
    pub win_rate: Option<f64>,
    /// Average and worst adverse excursion (drawdown during the trade), in R.
    pub avg_mae_r: Option<f64>,
    pub worst_mae_r: Option<f64>,
    pub avg_return_pct: Option<f64>,
    pub avg_mae_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JournalProfile {
    pub horizon: u32,
    /// Purchases reviewed at this horizon / all purchases.
    pub reviewed: usize,
    pub purchases: usize,
    pub pending: usize,
    pub all: Option<GroupStat>,
    pub by_rating: Vec<GroupStat>,
    pub by_plan: Vec<GroupStat>,
    pub by_regime: Vec<GroupStat>,
}

fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| round(v.iter().fold(0.0, |a, b| a + b) / v.len() as f64, 3))
}

fn median(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let m = s.len() / 2;
    Some(round(if s.len() % 2 == 1 { s[m] } else { (s[m - 1] + s[m]) / 2.0 }, 3))
}

fn stat(key: &str, label: &str, rows: &[&HorizonReview]) -> GroupStat {
    let in_r: Vec<&&HorizonReview> = rows.iter().filter(|h| h.result_r.is_some()).collect();
    let rs: Vec<f64> = in_r.iter().filter_map(|h| h.result_r).collect();
    let wins = rows.iter().filter(|h| h.result_r.or(h.return_pct).unwrap_or(0.0) > 0.0).count();
    let maes: Vec<f64> = in_r.iter().filter_map(|h| h.mae_r).collect();
    GroupStat {
        key: key.into(),
        label: label.into(),
        n: rows.len(),
        with_stop: in_r.len(),
        low_sample: rows.len() < MIN_SAMPLE,
        avg_r: mean(&rs),
        median_r: median(&rs),
        win_rate: (!rows.is_empty()).then(|| round((wins as f64 / rows.len() as f64) * 100.0, 2)),
        avg_mae_r: mean(&maes),
        worst_mae_r: maes.iter().copied().reduce(js_min),
        avg_return_pct: mean(&rows.iter().filter_map(|h| h.return_pct).collect::<Vec<_>>()),
        avg_mae_pct: mean(&rows.iter().filter_map(|h| h.mae_pct).collect::<Vec<_>>()),
    }
}

pub fn regime_label(k: &str) -> String {
    match k {
        "riskOn" => "Risk-on".into(),
        "neutral" => "Neutre".into(),
        "riskOff" => "Risk-off".into(),
        "unknown" => "Régime inconnu".into(),
        other => other.into(),
    }
}

const RATING_ORDER: [&str; 6] = ["strongBuy", "buy", "hold", "reduce", "sell", "strongSell"];

/// `[...].indexOf(k)` (−1 when absent).
fn index_of(list: &[&str], k: &str) -> i64 {
    list.iter().position(|x| *x == k).map(|i| i as i64).unwrap_or(-1)
}

/// Results of the purchases at one horizon, grouped by rating at entry, plan respected or not, and market regime.
/// Groups are in R and drawdown; ordered by a fixed scale, never by performance.
pub fn journal_profile(reviews: &[EntryReview], horizon: u32) -> JournalProfile {
    let buys: Vec<&EntryReview> = reviews.iter().filter(|r| r.entry.side == JournalSide::Buy).collect();
    let rows: Vec<(&EntryReview, &HorizonReview)> = buys
        .iter()
        .filter_map(|r| r.horizons.iter().find(|x| x.days == horizon).filter(|h| h.status == ReviewStatus::Ready).map(|h| (*r, h)))
        .collect();
    let group = |key_of: &dyn Fn(&EntryReview) -> (String, String), order: &dyn Fn(&str) -> i64| {
        let mut m: Vec<(String, String, Vec<&HorizonReview>)> = Vec::new();
        for (r, h) in &rows {
            let (k, label) = key_of(r);
            match m.iter_mut().find(|(g, ..)| *g == k) {
                Some((_, _, hs)) => hs.push(h),
                None => m.push((k, label, vec![h])),
            }
        }
        let mut out: Vec<GroupStat> = m.iter().map(|(k, label, hs)| stat(k, label, hs)).collect();
        out.sort_by(|a, b| order(&a.key).cmp(&order(&b.key)).then_with(|| locale_cmp(&a.key, &b.key)));
        out
    };
    let all_rows: Vec<&HorizonReview> = rows.iter().map(|(_, h)| *h).collect();
    JournalProfile {
        horizon,
        reviewed: rows.len(),
        purchases: buys.len(),
        pending: buys.iter().filter(|r| r.horizons.iter().find(|x| x.days == horizon).is_some_and(|h| h.status == ReviewStatus::Pending)).count(),
        all: (!rows.is_empty()).then(|| stat("all", "Tous les achats", &all_rows)),
        by_rating: group(
            &|r| match &r.entry.decision {
                Some(d) => (d.rating.clone().unwrap_or_else(|| d.verdict.clone()), d.rating_label.clone().unwrap_or_else(|| d.label.clone())),
                None => ("none".into(), "Sans décision".into()),
            },
            &|k| {
                let i = index_of(&RATING_ORDER, k);
                if i >= 0 {
                    i
                } else if k == "none" {
                    99
                } else {
                    50
                }
            },
        ),
        by_plan: group(
            &|r| match r.plan_respected {
                None => ("unknown".into(), "Non vérifiable".into()),
                Some(true) => ("yes".into(), "Plan respecté".into()),
                Some(false) => ("no".into(), "Plan non respecté".into()),
            },
            &|k| index_of(&["yes", "no", "unknown"], k),
        ),
        by_regime: group(
            &|r| {
                let k = r.entry.market.regime.clone().unwrap_or_else(|| "unknown".into());
                let label = regime_label(&k);
                (k, label)
            },
            &|k| index_of(&["riskOn", "neutral", "riskOff", "unknown"], k),
        ),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::jstime::date_utc;
    use serde_json::json;

    pub fn t0() -> f64 {
        date_utc(2026.0, 5.0, 1.0, 0.0, 0.0)
    }
    /// Entry at 10:00 UTC on day 0.
    pub fn at() -> f64 {
        t0() + 10.0 * 3_600_000.0
    }

    const DECISION: &str = r#"{
        "symbol": "BTC", "kind": "crypto", "name": "Bitcoin", "asOf": 0, "price": 100, "mode": "informational",
        "verdict": "buy", "label": "Acheter", "level": "strong", "levelLabel": "Signal fort", "confidence": 72, "confidenceText": "", "headline": "Rebond sur support",
        "families": [], "vetoes": [{ "code": "x", "label": "Volatilité extrême", "active": false, "verifiable": true, "detail": "" }], "blocked": false,
        "setup": { "name": "Rebond", "steps": [{ "label": "Support", "state": "ok", "detail": "" }, { "label": "Volume", "state": "no", "detail": "" }], "met": 1, "total": 2 },
        "plan": { "zoneFrom": 98, "zoneTo": 102, "entry": 100, "stop": 95, "target1": 110, "target2": 120, "riskPct": 5, "reward1Pct": 10, "reward2Pct": 20, "riskReward": 2, "minRiskReward": 2, "acceptable": true, "horizon": "swing" },
        "whyWait": [], "toBuy": [], "toSell": [], "scenarios": [], "pros": ["a", "b", "c", "d"], "cons": ["x"], "whyNot": { "risks": [], "uncertainty": "low", "invalidation": [] },
        "fundamentals": null, "liquidity": { "spreadPct": null, "dailyValue": null, "relativeVolume": 1.4, "source": "" }, "track": null, "position": null, "exposure": null, "sources": [], "disclaimer": "d",
        "rating": "buy", "ratingLabel": "Achat", "score": { "value": 64, "label": "", "factors": [], "missing": [], "custom": false, "text": "" },
        "degraded": { "active": false, "headline": "", "reasons": [] }, "marketRegime": { "kind": "riskOn", "label": "Risk-on", "benchmark": null, "reasons": [] },
        "horizon": { "kind": "swing", "label": "Swing", "atrDistance": 1, "detail": "" }, "events": []
    }"#;

    /// A decision with what the journal reads (journal.test.ts), `over` replaces fields.
    pub fn decision(over: Value) -> Decision {
        let mut d: Value = serde_json::from_str(DECISION).unwrap();
        d["asOf"] = json!((at() - 3_600_000.0) as i64);
        if let (Some(o), Some(over)) = (d.as_object_mut(), over.as_object()) {
            for (k, v) in over {
                if v == &json!("__delete") {
                    o.remove(k);
                } else {
                    o.insert(k.clone(), v.clone());
                }
            }
        }
        serde_json::from_value(d).expect("decision")
    }

    pub fn new_entry(d: Option<&Decision>) -> NewEntry<'_> {
        NewEntry {
            id: "e1".into(),
            now: at(),
            source: "paper",
            side: Some(JournalSide::Buy),
            symbol: "BTC".into(),
            kind: Some(Kind::Crypto),
            name: "Bitcoin".into(),
            price: 100.0,
            stop: Some(95.0),
            targets: vec![Some(110.0)],
            decision: d,
            ..Default::default()
        }
    }

    /// Daily candles from day 0 (the entry day) with (low, high, close) per day.
    pub fn days(rows: &[(f64, f64, f64)]) -> Vec<Candle> {
        rows.iter()
            .enumerate()
            .map(|(i, (low, high, close))| Candle {
                time: (t0() + i as f64 * DAY) as i64,
                open: if i > 0 { rows[i - 1].2 } else { 100.0 },
                high: *high,
                low: *low,
                close: *close,
                volume: 10.0,
            })
            .collect()
    }

    fn close_to(a: f64, b: f64, digits: i32) {
        assert!((a - b).abs() < 10f64.powi(-digits) / 2.0, "{a} ≉ {b}");
    }

    #[test]
    fn snapshot_and_market_from_decision() {
        let d = decision(json!({}));
        let e = create_entry(&NewEntry { note: "  rebond  ".into(), ..new_entry(Some(&d)) });
        let s = e.decision.as_ref().unwrap();
        assert_eq!(s.pros, ["a", "b", "c"]);
        assert!(s.vetoes.is_empty());
        assert_eq!(
            s.setup.steps,
            vec![SetupStepSnap { label: "Support".into(), state: "ok".into() }, SetupStepSnap { label: "Volume".into(), state: "no".into() }]
        );
        assert_eq!(s.score, Some(64.0));
        assert_eq!(e.market.regime.as_deref(), Some("riskOn"));
        assert_eq!(e.market.relative_volume, Some(1.4));
        assert_eq!(e.market.events, Some(0.0));
        assert_eq!(e.market.macro_score, None);
        assert_eq!(e.market.atr_pct, None);
        assert_eq!(e.signal, "Achat (Signal fort) · configuration « Rebond » 1/2 · horizon Swing");
        assert_eq!(e.note, "rebond");
    }

    #[test]
    fn levels_dropped() {
        let d = decision(json!({}));
        let e = create_entry(&NewEntry { stop: Some(105.0), targets: vec![Some(90.0), Some(130.0), None], ..new_entry(Some(&d)) });
        assert_eq!((e.stop, e.targets), (None, vec![130.0]));
        let e = create_entry(&NewEntry { side: Some(JournalSide::Sell), ..new_entry(Some(&d)) });
        assert_eq!((e.stop, e.targets.len()), (None, 0));
    }

    #[test]
    fn older_decisions() {
        let d = decision(json!({
            "rating": "__delete", "ratingLabel": "__delete", "score": "__delete", "degraded": "__delete",
            "marketRegime": "__delete", "horizon": "__delete", "events": "__delete",
        }));
        let e = create_entry(&NewEntry { id: "x".into(), source: "real", stop: None, targets: vec![], ..new_entry(Some(&d)) });
        let s = e.decision.as_ref().unwrap();
        assert_eq!((s.rating.clone(), s.score, s.degraded, s.horizon.as_deref()), (None, None, false, Some("swing")));
        assert_eq!(e.market.regime, None);
        assert_eq!(e.market.events, None);
    }

    /// A real decision saved from the server (older, without the rating and the added fields) gives an entry.
    #[test]
    fn real_decision_sample() {
        let d: Decision = serde_json::from_str(include_str!("../../../../backend/tests/samples/decision-btc.json")).unwrap();
        let s = snapshot_decision(&d);
        assert_eq!(s.rating, None);
        assert!(s.pros.len() <= 3);
        assert!(!signal_text(Some(&s)).is_empty());
    }

    #[test]
    fn state_helpers() {
        let d = decision(json!({}));
        let entry = create_entry(&new_entry(Some(&d)));
        let s = add_entry(&empty_journal(), entry.clone());
        let s = add_entry(&s, entry);
        assert_eq!(s.entries.len(), 1);
        let patch = EntryPatch {
            market: MarketPatch { macro_score: Some(Some(31.0)), macro_level: Some(Some("tense".into())), ..Default::default() },
            note: Some("x".into()),
            decision: None,
        };
        let s = patch_entry(&s, "e1", &patch);
        let m = &s.entries[0].market;
        assert_eq!((m.macro_score, m.macro_level.as_deref(), m.regime.as_deref()), (Some(31.0), Some("tense"), Some("riskOn")));
        assert_eq!(s.entries[0].note, "x");
        let saved: Value = serde_json::from_str(&journal_json(&s)).unwrap();
        assert!(is_journal_state(&saved));
        assert_eq!(parse_journal_state(&saved), Some(s.clone()));
        assert!(!is_journal_state(&json!({ "version": 1, "entries": [{ "id": 1 }] })));
        assert_eq!(remove_entry(&s, "e1").entries.len(), 0);
    }

    #[test]
    fn market_from_candles_before_entry() {
        let mut c: Vec<Candle> = (0..30)
            .map(|i| Candle {
                time: (t0() - (30 - i) as f64 * DAY) as i64,
                open: 100.0,
                high: 102.0,
                low: 98.0,
                close: 100.0,
                volume: if i == 29 { 30.0 } else { 10.0 },
            })
            .collect();
        // The entry day's own candle (huge range, huge volume) must not count.
        c.push(Candle { time: t0() as i64, open: 100.0, high: 200.0, low: 50.0, close: 150.0, volume: 1000.0 });
        let (atr_pct, rel) = market_from_candles(&c, at());
        close_to(atr_pct.unwrap(), 4.0, 5);
        close_to(rel.unwrap(), 3.0, 5);
    }

    #[test]
    fn holding_changes() {
        let hc = |side, quantity, price, implied| Some(HoldingChange { side, quantity, price, implied });
        assert_eq!(holding_change((1.0, 100.0), (2.0, 110.0), Some(130.0)), hc(JournalSide::Buy, 1.0, 120.0, true));
        assert_eq!(holding_change((1.0, 100.0), (3.0, 100.0), Some(130.0)), hc(JournalSide::Buy, 2.0, 130.0, false));
        assert_eq!(holding_change((2.0, 100.0), (0.5, 100.0), Some(90.0)), hc(JournalSide::Sell, 1.5, 90.0, false));
        assert_eq!(holding_change((2.0, 100.0), (1.0, 100.0), None), None);
        assert_eq!(holding_change((2.0, 100.0), (2.0, 90.0), Some(90.0)), None);
    }

    fn entry_with(d: Option<&Decision>, f: impl FnOnce(NewEntry) -> NewEntry) -> JournalEntry {
        create_entry(&f(new_entry(d)))
    }

    #[test]
    fn target_reached_on_day_6() {
        // Day 0 (entry day) touches the stop before the purchase: ignored.
        let c = days(&[
            (90.0, 101.0, 100.0),
            (99.0, 103.0, 102.0),
            (98.0, 104.0, 103.0),
            (99.0, 105.0, 104.0),
            (100.0, 106.0, 105.0),
            (101.0, 107.0, 106.0),
            (104.0, 111.0, 109.0),
            (105.0, 108.0, 107.0),
            (104.0, 108.0, 106.0),
            (104.0, 107.0, 106.0),
            (103.0, 106.0, 105.0),
        ]);
        let d = decision(json!({}));
        let r = review_entry(&entry_with(Some(&d), |n| n), &c, at() + 40.0 * DAY, None);
        let h10 = r.horizons.iter().find(|h| h.days == 10).unwrap();
        assert_eq!(h10.status, ReviewStatus::Ready);
        assert_eq!(h10.first, FirstHit::Target1);
        assert_eq!(h10.first_days, Some(6.0));
        assert_eq!(h10.result_r, Some(2.0));
        close_to(h10.mfe_pct.unwrap(), 11.0, 6);
        close_to(h10.mae_r.unwrap(), -0.4, 6);
        assert_eq!(r.plan_r, Some(2.0));
        assert!(r.worked.contains(&"L'objectif 1 a été atteint en 6 jours (+2 R).".to_string()), "{:?}", r.worked);
        assert_eq!(r.horizons.iter().find(|h| h.days == 30).unwrap().status, ReviewStatus::Ready);
    }

    #[test]
    fn stop_touched_while_degraded() {
        let d = decision(json!({ "degraded": { "active": true, "headline": "Données incomplètes", "reasons": [] } }));
        let e = entry_with(Some(&d), |n| n);
        let c = days(&[(99.0, 101.0, 100.0), (99.0, 102.0, 101.0), (94.0, 112.0, 96.0), (95.0, 97.0, 96.0)]);
        let r = review_entry(&e, &c, at() + 5.0 * DAY, None);
        let h3 = &r.horizons[0];
        assert_eq!(h3.first, FirstHit::Stop);
        assert_eq!(h3.first_days, Some(2.0));
        assert_eq!(h3.result_r, Some(-1.0));
        assert_eq!(r.failed[0], "Le stop a été touché en 2 jours (−1 R) alors que le signal était dégradé à l'entrée.");
        assert_eq!(r.coherent, Some(false));
        assert_eq!(r.horizons[1].status, ReviewStatus::Pending);
    }

    #[test]
    fn gap_below_the_stop() {
        let mut c = days(&[(99.0, 101.0, 100.0), (88.0, 92.0, 90.0)]);
        c[1].open = 90.0;
        let d = decision(json!({}));
        let h = &review_entry(&entry_with(Some(&d), |n| n), &c, at() + 4.0 * DAY, None).horizons[0];
        assert_eq!(h.result_r, Some(-2.0));
    }

    #[test]
    fn coherence() {
        let base = decision(json!({}));
        let mut plan = serde_json::to_value(base.plan.as_ref().unwrap()).unwrap();
        plan["riskReward"] = json!(1.2);
        let d = decision(json!({
            "verdict": "wait", "label": "Attendre",
            "vetoes": [{ "code": "v", "label": "Annonce macro dans les 48 h", "active": true, "verifiable": true, "detail": "" }],
            "plan": plan,
            "marketRegime": { "kind": "riskOff", "label": "Risk-off", "benchmark": null, "reasons": [] },
        }));
        let e = entry_with(Some(&d), |n| NewEntry { price: 104.08, ..n });
        let checks = coherence_checks(&e, &levels_of(&e));
        let get = |code: &str| checks.iter().find(|c| c.code == code).unwrap();
        assert_eq!(get("vetoes").ok, Some(false));
        assert!(get("vetoes").detail.contains("Annonce macro dans les 48 h"));
        assert_eq!(get("rr").ok, Some(false));
        assert_eq!(get("zone").detail, "Entrée hors zone d'achat : +2 % au-dessus du haut de la zone.");
        assert_eq!(get("regime").ok, Some(false));
        assert_eq!(get("verdict").ok, Some(false));
        assert_eq!(get("degraded").ok, Some(true));
    }

    #[test]
    fn without_decision_or_stop() {
        let bare = entry_with(None, |n| NewEntry { stop: None, targets: vec![], ..n });
        let r = review_entry(
            &bare,
            &days(&[(99.0, 101.0, 100.0), (100.0, 104.0, 103.0), (101.0, 105.0, 104.0), (102.0, 106.0, 105.0)]),
            at() + 4.0 * DAY,
            None,
        );
        assert_eq!(r.coherent, None);
        assert_eq!(r.risk, None);
        assert_eq!(r.horizons[0].result_r, None);
        assert_eq!(r.worked[0], "+5 % en 3 jours (sans stop, résultat en R non mesurable).");
        let d = decision(json!({}));
        let from_plan = review_entry(&entry_with(Some(&d), |n| NewEntry { stop: None, targets: vec![], ..n }), &[], at(), None);
        assert_eq!(
            from_plan.levels,
            Levels { stop: Some(95.0), stop_source: LevelSource::Plan, targets: vec![110.0, 120.0], targets_source: LevelSource::Plan }
        );
    }

    #[test]
    fn history_after_the_entry() {
        let c: Vec<Candle> = days(&[(99.0, 101.0, 100.0); 4]).into_iter().skip(2).collect();
        let d = decision(json!({}));
        assert_eq!(review_entry(&entry_with(Some(&d), |n| n), &c, at() + 5.0 * DAY, None).horizons[0].status, ReviewStatus::NoData);
    }

    #[test]
    fn closed_paper_trade() {
        let d = decision(json!({}));
        let r = review_entry(
            &entry_with(Some(&d), |n| n),
            &[],
            at() + DAY,
            Some(ClosedInfo { at: at() + DAY, price: 105.0, reason: "vente manuelle".into() }),
        );
        assert_eq!(r.realized_r, Some(1.0));
    }

    #[test]
    fn a_sale() {
        let d = decision(json!({ "verdict": "trim", "label": "Alléger" }));
        let e = entry_with(Some(&d), |n| NewEntry { side: Some(JournalSide::Sell), ..n });
        let r = review_entry(&e, &days(&[(99.0, 101.0, 100.0), (95.0, 99.0, 96.0), (94.0, 97.0, 95.0), (93.0, 96.0, 94.0)]), at() + 4.0 * DAY, None);
        assert!(r.worked[0].contains("3 jours après la vente, le cours est à −6 %"), "{}", r.worked[0]);
        assert_eq!(r.plan_respected, None);
        assert_eq!(r.coherent, Some(true));
    }

    #[test]
    fn profile() {
        let win = days(&[(99.0, 101.0, 100.0), (100.0, 111.0, 110.0), (104.0, 108.0, 107.0), (104.0, 108.0, 107.0)]);
        let loss = days(&[(99.0, 101.0, 100.0), (94.0, 100.0, 95.0), (95.0, 97.0, 96.0), (95.0, 97.0, 96.0)]);
        let d = decision(json!({}));
        let off = decision(
            json!({ "marketRegime": { "kind": "riskOff", "label": "Risk-off", "benchmark": null, "reasons": [] }, "rating": "hold", "ratingLabel": "Conserver" }),
        );
        let mut reviews: Vec<EntryReview> =
            (0..5).map(|i| review_entry(&entry_with(Some(&d), |n| NewEntry { id: format!("w{i}"), ..n }), &win, at() + 5.0 * DAY, None)).collect();
        reviews.push(review_entry(&entry_with(Some(&off), |n| NewEntry { id: "l".into(), ..n }), &loss, at() + 5.0 * DAY, None));
        let p = journal_profile(&reviews, 3);
        assert_eq!(p.reviewed, 6);
        let all = p.all.as_ref().unwrap();
        assert_eq!((all.n, all.avg_r, all.low_sample), (6, Some(1.5), false));
        let by_rating: Vec<(&str, usize, bool)> = p.by_rating.iter().map(|g| (g.key.as_str(), g.n, g.low_sample)).collect();
        assert_eq!(by_rating, [("buy", 5, false), ("hold", 1, true)]);
        let by_plan: Vec<(&str, usize)> = p.by_plan.iter().map(|g| (g.key.as_str(), g.n)).collect();
        assert_eq!(by_plan, [("yes", 5), ("no", 1)]);
        let by_regime: Vec<(&str, Option<f64>)> = p.by_regime.iter().map(|g| (g.key.as_str(), g.avg_r)).collect();
        assert_eq!(by_regime, [("riskOn", Some(2.0)), ("riskOff", Some(-1.0))]);
        assert_eq!(p.by_regime[1].worst_mae_r, Some(-1.2));
        assert_eq!(journal_profile(&reviews, 10).reviewed, 0);
    }

    #[test]
    fn snapshot_without_plan() {
        assert_eq!(snapshot_decision(&decision(json!({ "plan": null }))).plan, None);
    }
}
