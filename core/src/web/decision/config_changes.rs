//! Configuration changes of the watched assets ("🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT",
//! config-changes.ts): the last decision seen per asset (verdict, level, unmet setup steps, buy conditions) is kept
//! in this browser ("altim.configChanges.v1") and compared with each new one. The server is stateless: the diff is
//! done here. Each snapshot also keeps the decision's measurements so that a change is explained ("Momentum −18 pts",
//! "Volume en baisse (1,4× → 0,7× la moyenne)", …): `explain_change`.
//!
//! Stored state, version 2: `{ version, last: { "crypto:BTC:i": snapshot }, transitions: [newest first] }`; version 1
//! entries (no measurements, no explanation) are migrated on read, a damaged entry is dropped.
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::doc::DecisionDoc;
use super::format::{num, signed_score, usd as usd_fr};
use crate::engine::decision_types::{Level, Rating, StepState, Verdict};
use crate::js::fr;
use crate::types::Kind;
use crate::web::money::{MoneyDisplay, NBSP};

pub const CONFIG_KEY: &str = "altim.configChanges.v1";
/// Transitions kept (newest first).
pub const MAX_TRANSITIONS: usize = 50;
/// Version of the stored state (1: without measurements nor explanations, migrated on read).
pub const STATE_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricFamily {
    pub key: String,
    pub label: String,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetricLevel {
    pub price: f64,
    pub touches: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopNews {
    pub title: String,
    pub tone: String,
}

/// Measurements of a decision kept to explain a later change (the server's `snapshot`, else read from the decision).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalMetrics {
    pub price: Option<f64>,
    pub composite: Option<f64>,
    pub families: Vec<MetricFamily>,
    /// Last daily volume ÷ 20-day average.
    pub rel_volume: Option<f64>,
    pub rsi: Option<f64>,
    pub support: Option<MetricLevel>,
    pub resistance: Option<MetricLevel>,
    pub news_score: Option<f64>,
    pub top_news: Option<TopNews>,
}

/// Verdict, level and rating of a configuration (`from` / `to` of a transition).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub verdict: Verdict,
    pub level: Level,
    pub label: String,
    pub level_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<Rating>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSnapshot {
    pub verdict: Verdict,
    pub level: Level,
    pub label: String,
    pub level_label: String,
    pub at: f64,
    /// Added in version 2 (absent from migrated entries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<Rating>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<SignalMetrics>,
    /// Setup steps not met yet ("Cassure de la résistance : pas encore").
    pub missing: Vec<String>,
    /// Conditions that would change the decision (the decision's `toBuy`).
    pub triggers: Vec<String>,
}

impl ConfigSnapshot {
    fn pick(&self) -> Config {
        Config {
            verdict: self.verdict,
            level: self.level,
            label: self.label.clone(),
            level_label: self.level_label.clone(),
            rating: self.rating,
            rating_label: self.rating.and(self.rating_label.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigTransition {
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    /// Personal mode (with the user's average cost and weights) or market data only: compared separately.
    pub personal: bool,
    pub at: f64,
    /// When the previous configuration was seen.
    pub since: f64,
    pub from: Config,
    pub to: Config,
    pub missing: Vec<String>,
    pub triggers: Vec<String>,
    /// What changed in the measurements ("Momentum −18 pts (+40 → +22)"); empty when unknown (older entries).
    #[serde(default)]
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfigState {
    pub last: IndexMap<String, ConfigSnapshot>,
    pub transitions: Vec<ConfigTransition>,
}

impl ConfigState {
    /// `JSON.stringify(state)`.
    pub fn to_json(&self) -> String {
        crate::js::to_value(&serde_json::json!({ "version": STATE_VERSION, "last": self.last, "transitions": self.transitions })).to_string()
    }
}

fn fin(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite())
}

/// The server's compact snapshot when given, else the same numbers read from the decision (older servers).
pub fn metrics_of(doc: &DecisionDoc) -> SignalMetrics {
    let d = &doc.d;
    let fam: Vec<MetricFamily> =
        d.families.iter().map(|f| MetricFamily { key: f.key.clone(), label: f.label.clone(), score: fin(f.score) }).collect();
    let level = |p: f64, t: f64| p.is_finite().then_some(MetricLevel { price: p, touches: t });
    if let Some(s) = doc.snapshot() {
        let label_of = |k: &str| d.families.iter().find(|f| f.key == k).map(|f| f.label.clone());
        return SignalMetrics {
            price: fin(s.price),
            composite: fin(s.composite),
            families: s
                .families
                .iter()
                .map(|f| MetricFamily {
                    key: f.key.clone(),
                    label: if f.label.is_empty() {
                        label_of(&f.key).filter(|l| !l.is_empty()).unwrap_or_else(|| f.key.clone())
                    } else {
                        f.label.clone()
                    },
                    score: fin(f.score),
                })
                .collect(),
            rel_volume: fin(s.relative_volume),
            rsi: fin(s.rsi),
            support: s.nearest_support.as_ref().and_then(|l| level(l.price, l.touches as f64)),
            resistance: s.nearest_resistance.as_ref().and_then(|l| level(l.price, l.touches as f64)),
            news_score: fin(s.news_score),
            top_news: s.top_news.as_ref().map(|n| TopNews { title: n.title.clone(), tone: n.tone.clone() }),
        };
    }
    let st = d.structure.as_ref();
    SignalMetrics {
        price: fin(d.price),
        composite: doc.score().and_then(|s| fin(s.value)),
        news_score: fam.iter().find(|f| f.key == "news").and_then(|f| f.score),
        families: fam,
        rel_volume: fin(d.liquidity.as_ref().and_then(|l| l.relative_volume)),
        rsi: None,
        support: st.and_then(|s| s.nearest_support.as_ref()).and_then(|l| level(l.price, l.touches as f64)),
        resistance: st.and_then(|s| s.nearest_resistance.as_ref()).and_then(|l| level(l.price, l.touches as f64)),
        top_news: None,
    }
}

/// Thresholds under which a change is not worth telling.
pub struct ChangeLimits {
    pub composite: f64,
    pub family: f64,
    pub volume: f64,
    pub rsi: f64,
    pub news: f64,
    pub level: f64,
}
pub const CHANGE_LIMITS: ChangeLimits = ChangeLimits { composite: 5.0, family: 10.0, volume: 0.3, rsi: 10.0, news: 15.0, level: 0.005 };

fn x1(v: f64) -> String {
    format!("{}×", num(Some(v), 1))
}

fn same_level(a: f64, b: f64) -> bool {
    (a - b).abs() / a.abs().max(1e-9) <= CHANGE_LIMITS.level
}

/// What changed between two snapshots, most telling first: composite score, families (largest moves first, 4 at
/// most), relative volume, RSI, support / resistance (broken, crossed, confirmed or replaced) and news.
pub fn explain_change(prev: &SignalMetrics, next: &SignalMetrics, m: &MoneyDisplay) -> Vec<String> {
    let usd = |v: f64| usd_fr(Some(v), m);
    let mut out = Vec::new();
    if let (Some(a), Some(b)) = (prev.composite, next.composite) {
        if (b - a).abs() >= CHANGE_LIMITS.composite {
            out.push(format!("Score composite {} → {}", signed_score(a), signed_score(b)));
        }
    }
    let mut moves: Vec<(f64, String)> = Vec::new();
    for f in &next.families {
        let Some(p) = prev.families.iter().rev().find(|x| x.key == f.key) else { continue };
        match (p.score, f.score) {
            (Some(ps), Some(fs)) => {
                let d = fs - ps;
                if d.abs() >= CHANGE_LIMITS.family {
                    moves.push((d.abs(), format!("{} {} pts ({} → {})", f.label, signed_score(d), signed_score(ps), signed_score(fs))));
                }
            }
            (Some(_), None) => moves.push((0.0, format!("{} : plus mesuré(e) (données indisponibles)", f.label))),
            (None, Some(fs)) => moves.push((0.0, format!("{} : de nouveau mesuré(e) ({})", f.label, signed_score(fs)))),
            (None, None) => {}
        }
    }
    moves.sort_by(|a, b| b.0.total_cmp(&a.0));
    out.extend(moves.into_iter().take(4).map(|m| m.1));
    if let (Some(a), Some(b)) = (prev.rel_volume, next.rel_volume) {
        if (b - a).abs() >= CHANGE_LIMITS.volume {
            out.push(format!("Volume en {} ({} → {} la moyenne)", if b > a { "hausse" } else { "baisse" }, x1(a), x1(b)));
        }
    }
    if let (Some(a), Some(b)) = (prev.rsi, next.rsi) {
        if (b - a).abs() >= CHANGE_LIMITS.rsi {
            out.push(format!("RSI {} → {}", num(Some(a), 0), num(Some(b), 0)));
        }
    }
    let (r0, r1) = (prev.resistance, next.resistance);
    match (r0, r1, next.price) {
        (Some(r0), _, Some(p)) if p > r0.price && prev.price.is_none_or(|pp| pp <= r0.price) => {
            out.push(format!("Résistance {} franchie (prix {})", usd(r0.price), usd(p)))
        }
        (Some(r0), Some(r1), _) if same_level(r0.price, r1.price) && r1.touches > r0.touches => {
            out.push(format!("Résistance {} confirmée (touchée {} fois)", usd(r1.price), crate::js::number_to_string(r1.touches)))
        }
        (Some(r0), Some(r1), _) if !same_level(r0.price, r1.price) => {
            out.push(format!("Résistance la plus proche : {} → {}", usd(r0.price), usd(r1.price)))
        }
        _ => {}
    }
    let (s0, s1) = (prev.support, next.support);
    match (s0, s1, next.price) {
        (Some(s0), _, Some(p)) if p < s0.price && prev.price.is_none_or(|pp| pp >= s0.price) => {
            out.push(format!("Support {} cassé (prix {})", usd(s0.price), usd(p)))
        }
        (Some(s0), Some(s1), _) if same_level(s0.price, s1.price) && s1.touches > s0.touches => {
            out.push(format!("Support {} confirmé (touché {} fois)", usd(s1.price), crate::js::number_to_string(s1.touches)))
        }
        (Some(s0), Some(s1), _) if !same_level(s0.price, s1.price) => {
            out.push(format!("Support le plus proche : {} → {}", usd(s0.price), usd(s1.price)))
        }
        _ => {}
    }
    match &next.top_news {
        Some(n) if n.tone != "neutral" && Some(&n.title) != prev.top_news.as_ref().map(|t| &t.title) => {
            out.push(format!("Actualité {} : « {} »", if n.tone == "negative" { "négative" } else { "positive" }, n.title))
        }
        _ => {
            if let (Some(a), Some(b)) = (prev.news_score, next.news_score) {
                if (b - a).abs() >= CHANGE_LIMITS.news {
                    out.push(format!("Ton des actualités {} → {}", signed_score(a), signed_score(b)));
                }
            }
        }
    }
    out
}

/// A condition's level in the display currency, as the triggers write it ("… (64 210,5 €)").
fn trigger_usd(v: f64, m: &MoneyDisplay) -> String {
    m.money_fmt(v, |x| fr(x, 0, if x >= 1.0 { 2 } else { 6 }), NBSP)
}

/// What the decision says is missing and what would change it, with its measurements.
pub fn snapshot_of(doc: &DecisionDoc, at: f64, m: &MoneyDisplay) -> ConfigSnapshot {
    let d = &doc.d;
    let rating = doc.rating();
    ConfigSnapshot {
        verdict: d.verdict,
        level: d.level,
        label: d.label.clone(),
        level_label: d.level_label.clone(),
        at,
        rating,
        rating_label: rating.map(|r| if d.rating_label.is_empty() { super::format::rating_key(r).to_string() } else { d.rating_label.clone() }),
        metrics: Some(metrics_of(doc)),
        missing: d
            .setup
            .steps
            .iter()
            .filter(|s| s.state != StepState::Ok)
            .map(|s| {
                let st = if s.state == StepState::No { "pas encore" } else { "non vérifiable" };
                format!("{} : {st}{}", s.label, if s.detail.is_empty() { String::new() } else { format!(" ({})", s.detail) })
            })
            .collect(),
        triggers: d
            .to_buy
            .iter()
            .map(|c| match c.level {
                Some(l) if !c.text.contains(['$', '€']) => format!("{} ({})", c.text, trigger_usd(l, m)),
                _ => c.text.clone(),
            })
            .collect(),
    }
}

pub fn snapshot_key(kind: Kind, symbol: &str, personal: bool) -> String {
    format!("{}:{symbol}:{}", kind.as_str(), if personal { "p" } else { "i" })
}

/// The asset of a transition.
pub struct AssetRef<'a> {
    pub symbol: &'a str,
    pub kind: Kind,
    pub name: &'a str,
    pub personal: bool,
}

/// A transition when the verdict, the level or the rating (when both snapshots have one) changed; None for the
/// first sighting or the same configuration. `changes` explains it when both snapshots kept their measurements.
pub fn diff_configuration(prev: Option<&ConfigSnapshot>, next: &ConfigSnapshot, asset: AssetRef, m: &MoneyDisplay) -> Option<ConfigTransition> {
    let prev = prev?;
    let rating_changed = matches!((prev.rating, next.rating), (Some(a), Some(b)) if a != b);
    if prev.verdict == next.verdict && prev.level == next.level && !rating_changed {
        return None;
    }
    let changes = match (&prev.metrics, &next.metrics) {
        (Some(a), Some(b)) => explain_change(a, b, m),
        _ => Vec::new(),
    };
    Some(ConfigTransition {
        symbol: asset.symbol.into(),
        kind: asset.kind,
        name: asset.name.into(),
        personal: asset.personal,
        at: next.at,
        since: prev.at,
        from: prev.pick(),
        to: next.pick(),
        missing: next.missing.clone(),
        triggers: next.triggers.clone(),
        changes,
    })
}

// ---------- Validation on read (a damaged or older entry is dropped, never trusted) ----------

const VERDICTS: [&str; 6] = ["buy", "buyZone", "wait", "noPosition", "trim", "sell"];
const LEVELS: [&str; 5] = ["strong", "moderate", "waiting", "highRisk", "exit"];
const RATINGS: [&str; 6] = ["strongBuy", "buy", "hold", "reduce", "sell", "strongSell"];

fn is_strings(v: Option<&Value>) -> bool {
    v.and_then(Value::as_array).is_some_and(|a| a.iter().all(Value::is_string))
}
fn is_finite(v: Option<&Value>) -> bool {
    v.and_then(Value::as_f64).is_some_and(f64::is_finite)
}
fn is_config(x: &Value) -> bool {
    let s = |k: &str| x.get(k).and_then(Value::as_str);
    s("verdict").is_some_and(|v| VERDICTS.contains(&v))
        && s("level").is_some_and(|v| LEVELS.contains(&v))
        && s("label").is_some()
        && s("levelLabel").is_some()
        && x.get("rating").is_none_or(|r| r.as_str().is_some_and(|r| RATINGS.contains(&r)))
}
/// null or a finite number (undefined fails, as in the TypeScript).
fn is_num(v: Option<&Value>) -> bool {
    v.is_some_and(|v| v.is_null() || v.as_f64().is_some_and(f64::is_finite))
}
fn is_level(v: Option<&Value>) -> bool {
    v.is_some_and(|v| v.is_null() || (v.is_object() && is_finite(v.get("price")) && is_finite(v.get("touches"))))
}
fn is_metrics(x: &Value) -> bool {
    x.is_object()
        && ["price", "composite", "relVolume", "rsi", "newsScore"].iter().all(|k| is_num(x.get(*k)))
        && x.get("families").and_then(Value::as_array).is_some_and(|a| {
            a.iter().all(|f| f.get("key").is_some_and(Value::is_string) && f.get("label").is_some_and(Value::is_string) && is_num(f.get("score")))
        })
        && is_level(x.get("support"))
        && is_level(x.get("resistance"))
        && x.get("topNews")
            .is_some_and(|t| t.is_null() || (t.get("title").is_some_and(Value::is_string) && t.get("tone").is_some_and(Value::is_string)))
}
fn is_snapshot(x: &Value) -> bool {
    is_config(x) && is_strings(x.get("missing")) && is_strings(x.get("triggers")) && is_finite(x.get("at"))
}
fn is_transition(x: &Value) -> bool {
    x.get("symbol").is_some_and(Value::is_string)
        && matches!(x.get("kind").and_then(Value::as_str), Some("crypto" | "stock"))
        && x.get("name").is_some_and(Value::is_string)
        && x.get("personal").is_some_and(Value::is_boolean)
        && is_finite(x.get("at"))
        && is_finite(x.get("since"))
        && x.get("from").is_some_and(is_config)
        && x.get("to").is_some_and(is_config)
        && is_strings(x.get("missing"))
        && is_strings(x.get("triggers"))
        && x.get("changes").is_none_or(|c| is_strings(Some(c)))
}

/// Stored state, validated: version 2, or version 1 migrated (its snapshots have no measurements, its transitions
/// no explanation). A damaged entry is dropped (damaged measurements alone are dropped from their snapshot); an
/// unknown version gives an empty state.
pub fn parse_state(raw: Option<&str>) -> ConfigState {
    let Some(p) = raw.and_then(|r| serde_json::from_str::<Value>(r).ok()) else { return ConfigState::default() };
    let version = p.get("version").and_then(Value::as_f64);
    let (Some(last), Some(transitions)) = (p.get("last").and_then(Value::as_object), p.get("transitions").and_then(Value::as_array)) else {
        return ConfigState::default();
    };
    if version != Some(1.0) && version != Some(STATE_VERSION as f64) {
        return ConfigState::default();
    }
    let last = last
        .iter()
        .filter(|(_, v)| is_snapshot(v))
        .filter_map(|(k, v)| {
            let mut v = v.clone();
            if v.get("metrics").is_some_and(|m| !is_metrics(m)) {
                v.as_object_mut()?.remove("metrics");
            }
            crate::web::json::from_value::<ConfigSnapshot>(&v).ok().map(|s| (k.clone(), s))
        })
        .collect();
    let transitions = transitions
        .iter()
        .filter(|t| is_transition(t))
        .take(MAX_TRANSITIONS)
        .filter_map(|t| crate::web::json::from_value::<ConfigTransition>(t).ok())
        .collect();
    ConfigState { last, transitions }
}

/// New state after seeing a decision (pure): baseline replaced, transition prepended when the configuration changed.
pub fn apply_decision(state: &ConfigState, doc: &DecisionDoc, personal: bool, now: f64, m: &MoneyDisplay) -> (ConfigState, Option<ConfigTransition>) {
    let d = &doc.d;
    let key = snapshot_key(d.kind, &d.symbol, personal);
    let next = snapshot_of(doc, now, m);
    let name = if d.name.is_empty() { &d.symbol } else { &d.name };
    let transition = diff_configuration(state.last.get(&key), &next, AssetRef { symbol: &d.symbol, kind: d.kind, name, personal }, m);
    let mut out = state.clone();
    out.last.insert(key, next);
    if let Some(t) = &transition {
        out.transitions.insert(0, t.clone());
        out.transitions.truncate(MAX_TRANSITIONS);
    }
    (out, transition)
}

/// "🚨 BTC — changement de configuration : ATTENDRE → ZONE D'ACHAT" (the level, then the rating, when only it changed).
pub fn transition_title(t: &ConfigTransition) -> String {
    let (a, b) = if t.from.verdict != t.to.verdict {
        (t.from.label.clone(), t.to.label.clone())
    } else if t.from.level != t.to.level
        || t.from.rating_label.as_deref().is_none_or(str::is_empty)
        || t.to.rating_label.as_deref().is_none_or(str::is_empty)
    {
        (t.from.level_label.clone(), t.to.level_label.clone())
    } else {
        (format!("note {}", t.from.rating_label.as_deref().unwrap_or("")), format!("note {}", t.to.rating_label.as_deref().unwrap_or("")))
    };
    format!("🚨 {} — changement de configuration : {a} → {b}", t.symbol)
}

/// The latest transition of this asset if it led to the configuration shown now (same verdict and level), for
/// "Pourquoi le signal a changé depuis …"; None otherwise.
pub fn latest_change<'a>(
    transitions: &'a [ConfigTransition],
    symbol: &str,
    kind: Kind,
    verdict: Verdict,
    level: Level,
    mode: &str,
) -> Option<&'a ConfigTransition> {
    let personal = mode == "personal";
    let t = transitions.iter().find(|x| x.symbol == symbol && x.kind == kind && x.personal == personal)?;
    (t.to.verdict == verdict && t.to.level == level).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::super::doc::tests::{btc, guidance, with};
    use super::super::format::NNBSP;
    use super::*;
    use serde_json::json;

    fn usd_m() -> MoneyDisplay {
        MoneyDisplay::usd()
    }
    fn apply(s: &ConfigState, d: &DecisionDoc, personal: bool, now: f64) -> (ConfigState, Option<ConfigTransition>) {
        apply_decision(s, d, personal, now, &usd_m())
    }

    // config-changes.test.ts "configuration changes"
    #[test]
    fn baseline_then_transition() {
        let d = btc();
        let (s1, t1) = apply(&ConfigState::default(), &d, false, 1000.0);
        assert!(t1.is_none());
        let next = with(&d, json!({ "verdict": "buyZone", "label": "ZONE D'ACHAT", "level": "moderate", "levelLabel": "Signal modéré" }));
        let (s2, t) = apply(&s1, &next, false, 2000.0);
        let t = t.unwrap();
        assert_eq!((t.from.verdict, t.to.verdict, t.since), (d.d.verdict, Verdict::BuyZone, 1000.0));
        assert_eq!(transition_title(&t), format!("🚨 BTC — changement de configuration : {} → ZONE D'ACHAT", d.d.label));
        assert_eq!(t.missing, snapshot_of(&next, 0.0, &usd_m()).missing);
        assert_eq!(t.missing.len(), d.d.setup.steps.iter().filter(|s| s.state != StepState::Ok).count());
        assert_eq!(t.triggers.len(), d.d.to_buy.len());
        // Same configuration again: nothing new; personal mode has its own baseline.
        assert!(apply(&s2, &next, false, 3000.0).1.is_none());
        assert!(apply(&s2, &d, true, 3000.0).1.is_none());
    }

    #[test]
    fn level_change_alone() {
        let s = apply(&ConfigState::default(), &btc(), false, 1.0).0;
        let t = apply(&s, &with(&btc(), json!({ "level": "highRisk", "levelLabel": "Risque élevé" })), false, 2.0).1.unwrap();
        assert!(transition_title(&t).contains("→ Risque élevé"));
    }

    #[test]
    fn at_most_50_newest_first() {
        let mut s = ConfigState::default();
        let (buy, wait) = (with(&btc(), json!({ "verdict": "buy" })), with(&btc(), json!({ "verdict": "wait" })));
        for i in 0..60 {
            s = apply(&s, if i % 2 == 1 { &wait } else { &buy }, false, i as f64).0;
        }
        assert_eq!(s.transitions.len(), MAX_TRANSITIONS);
        assert_eq!(s.transitions[0].at, 59.0);
    }

    #[test]
    fn damaged_storage() {
        assert_eq!(parse_state(Some("{nope")), ConfigState::default());
        assert_eq!(parse_state(Some(r#"{"version":3,"last":{},"transitions":[]}"#)), ConfigState::default());
        let s1 = apply(&ConfigState::default(), &btc(), false, 1.0).0;
        let good = apply(&s1, &with(&btc(), json!({ "verdict": "sell" })), false, 2.0).0;
        let mut v: Value = serde_json::from_str(&good.to_json()).unwrap();
        v["last"]["bad"] = json!({ "verdict": "hodl" });
        v["transitions"].as_array_mut().unwrap().push(json!({ "symbol": 1 }));
        let p = parse_state(Some(&v.to_string()));
        assert_eq!(p.last.keys().collect::<Vec<_>>(), good.last.keys().collect::<Vec<_>>());
        assert_eq!(p.transitions.len(), 1);
        // The text written reads back the same.
        assert_eq!(parse_state(Some(&good.to_json())), good);
    }

    // "why the signal changed"
    fn m(over: Value) -> SignalMetrics {
        let mut base = json!({
            "price": 342, "composite": 24,
            "families": [{ "key": "trend", "label": "Tendance", "score": 50 }, { "key": "momentum", "label": "Momentum", "score": 40 }, { "key": "news", "label": "Actualités", "score": 0 }],
            "relVolume": 1.4, "rsi": 62, "support": { "price": 335.61, "touches": 2 }, "resistance": { "price": 344.95, "touches": 2 }, "newsScore": 0, "topNews": null,
        });
        for (k, v) in over.as_object().unwrap() {
            base[k] = v.clone();
        }
        serde_json::from_value(base).unwrap()
    }

    #[test]
    fn every_measured_change_is_told() {
        let next = m(json!({
            "composite": 6,
            "families": [{ "key": "trend", "label": "Tendance", "score": 45 }, { "key": "momentum", "label": "Momentum", "score": 22 }, { "key": "news", "label": "Actualités", "score": null }],
            "relVolume": 0.7, "rsi": 48, "resistance": { "price": 344.9, "touches": 3 },
            "topNews": { "title": "Apple cuts iPhone orders", "tone": "negative" },
        }));
        assert_eq!(
            explain_change(&m(json!({})), &next, &usd_m()),
            vec![
                "Score composite +24 → +6".to_string(),
                "Momentum −18 pts (+40 → +22)".into(),
                "Actualités : plus mesuré(e) (données indisponibles)".into(),
                "Volume en baisse (1,4× → 0,7× la moyenne)".into(),
                "RSI 62 → 48".into(),
                format!("Résistance 344,90{NNBSP}$ confirmée (touchée 3 fois)"),
                "Actualité négative : « Apple cuts iPhone orders »".into(),
            ]
        );
    }

    #[test]
    fn levels_broken_or_crossed() {
        let e = |over| explain_change(&m(json!({})), &m(over), &usd_m());
        assert_eq!(e(json!({ "price": 330, "composite": 22, "rsi": 58 })), vec![format!("Support 335,61{NNBSP}$ cassé (prix 330{NNBSP}$)")]);
        assert_eq!(
            e(json!({ "price": 346, "resistance": { "price": 360, "touches": 2 } })),
            vec![format!("Résistance 344,95{NNBSP}$ franchie (prix 346{NNBSP}$)")]
        );
        assert!(e(json!({})).is_empty());
    }

    #[test]
    fn transition_carries_the_explanation_and_rating_change() {
        let d = btc();
        let score = |v: i32| json!({ "value": v, "label": "", "factors": [], "missing": [], "custom": false, "text": "" });
        let s = apply(&ConfigState::default(), &with(&d, json!({ "rating": "buy", "ratingLabel": "ACHAT", "score": score(24) })), false, 1.0).0;
        assert_eq!(s.last.values().next().unwrap().metrics.as_ref().unwrap().families.len(), d.d.families.len());
        let t = apply(&s, &with(&d, json!({ "rating": "hold", "ratingLabel": "ATTENDRE", "score": score(-30) })), false, 2.0).1.unwrap();
        assert_eq!(transition_title(&t), "🚨 BTC — changement de configuration : note ACHAT → note ATTENDRE");
        // Older decision without `snapshot`: the measurements are read from the decision itself.
        assert_eq!(t.changes, vec!["Score composite +24 → −30".to_string()]);
    }

    #[test]
    fn version_1_migrated() {
        let v1 = json!({
            "version": 1,
            "last": { "crypto:BTC:i": { "verdict": "wait", "level": "waiting", "label": "ATTENDRE", "levelLabel": "Attente", "missing": [], "triggers": [], "at": 1 } },
            "transitions": [{ "symbol": "BTC", "kind": "crypto", "name": "Bitcoin", "personal": false, "at": 2, "since": 1,
                "from": { "verdict": "buy", "level": "moderate", "label": "ACHETER", "levelLabel": "Signal modéré" },
                "to": { "verdict": "wait", "level": "waiting", "label": "ATTENDRE", "levelLabel": "Attente" }, "missing": [], "triggers": [] }],
        });
        let p = parse_state(Some(&v1.to_string()));
        assert!(p.last["crypto:BTC:i"].metrics.is_none());
        assert!(p.transitions[0].changes.is_empty());
        assert!(p.to_json().starts_with(r#"{"version":2,"#));
        let mut bad = v1.clone();
        bad["version"] = json!(2);
        bad["last"]["crypto:BTC:i"]["metrics"] = json!({ "price": "x" });
        let q = parse_state(Some(&bad.to_string()));
        assert_eq!(q.last.keys().collect::<Vec<_>>(), ["crypto:BTC:i"]);
        assert!(q.last["crypto:BTC:i"].metrics.is_none());
        // A migrated baseline (no measurements) gives a transition without explanation, never a made-up one.
        let t = apply(&p, &with(&btc(), json!({ "verdict": "buy", "label": "ACHETER" })), false, 3.0).1.unwrap();
        assert!(t.changes.is_empty());
    }

    #[test]
    fn the_card_finds_the_change() {
        let d = btc();
        let s = apply(&ConfigState::default(), &d, false, 1.0).0;
        let next = with(&d, json!({ "verdict": "buyZone", "level": "moderate" }));
        let st = apply(&s, &next, false, 2.0).0;
        let lc = |x: &DecisionDoc, mode: &str| latest_change(&st.transitions, &x.d.symbol, x.d.kind, x.d.verdict, x.d.level, mode).map(|t| t.since);
        assert_eq!(lc(&next, "informational"), Some(1.0));
        assert_eq!(lc(&d, "informational"), None);
        assert_eq!(lc(&next, "personal"), None);
    }

    // guidance.test.ts "counter-argument and why the signal changed" (the explanation part)
    #[test]
    fn change_since_with_snapshots() {
        let d = guidance();
        let mut before = with(&d, json!({ "verdict": "buyZone", "label": "ZONE D'ACHAT", "level": "moderate", "levelLabel": "Signal modéré" })).raw;
        before["snapshot"]["composite"] = json!(30);
        before["snapshot"]["relativeVolume"] = json!(1.4);
        let before = super::super::doc::parse_decision(before).unwrap();
        let s1 = apply(&ConfigState::default(), &before, false, crate::jstime::date_utc(2026.0, 8.0, 28.0, 12.0, 2.0)).0;
        let t = apply(&s1, &d, false, crate::jstime::date_utc(2026.0, 8.0, 28.0, 16.0, 0.0)).1.unwrap();
        assert_eq!(super::super::format::short_date_time(t.since), "28/09 à 14:02");
        assert!(transition_title(&t).ends_with("ZONE D'ACHAT → ATTENDRE"));
        assert!(t.changes.contains(&"Score composite +30 → +41".to_string()), "{:?}", t.changes);
        assert!(t.changes.contains(&"Volume en baisse (1,4× → 0,7× la moyenne)".to_string()));
    }
}
