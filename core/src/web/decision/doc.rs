//! The decision of `/api/decision` as the web app reads it (decision.ts): the server's own `Decision`
//! (`engine::decision_types`), the light runtime check of decision.ts (a wrong answer shows an error naming the field
//! instead of a broken card), and what personal mode sends (the average cost and each line's share of the
//! portfolio, never quantities nor amounts).
//!
//! The answer is kept as received next to the typed decision: the cache stores exactly that JSON, and the fields an
//! older server (or an older cached decision) did not send stay "absent" as in the TypeScript (`d.rating`,
//! `d.noTrade`, `d.events !== undefined`…), where the Rust type would otherwise fill in its defaults.
//!
//! The Radar, the configuration changes, the journal and the simulation read the `DecisionCore` (`d`): the server's
//! own types without its heaviest parts. Only the asset screen reads the whole `Decision` (`full`, decoded once, on
//! demand), so the others' .wasm leave its reader out.
use std::cell::OnceCell;

use serde::Deserialize;
use serde_json::Value;

use crate::calendar::CalendarEvent;
use crate::engine::bot::BotView;
use crate::engine::decision_types::{
    CompositeScore, Condition, CounterArgument, Decision, DecisionSnapshot, Degraded, DevActivity, Family, Fundamentals, HorizonClass, Level,
    Liquidity, MarketRegime, ModelEvidence, NoTrade, Plan, Rating, Setup, Verdict, Veto,
};
use crate::engine::structure::SrLevel;
use crate::js::{number_to_string, round};
use crate::types::Kind;
use crate::web::bot::encode_uri_component;
use crate::web::sorting::Sorting;
use crate::web::store::ScoreWeights;

/// The fields of the decision that the Radar, the configuration changes, the journal and the simulation read, with
/// the server's own types and attributes: everything but its fundamentals, track record, structure details, model
/// evidence, bot, calendar, scenarios, exposure, position, zones, sources…, which only the asset screen reads.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionCore {
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    pub as_of: i64,
    pub price: Option<f64>,
    pub mode: String,
    pub verdict: Verdict,
    pub label: String,
    pub level: Level,
    pub level_label: String,
    pub confidence: f64,
    pub headline: String,
    pub families: Vec<Family>,
    pub vetoes: Vec<Veto>,
    pub setup: Setup,
    pub plan: Option<Plan>,
    pub to_buy: Vec<Condition>,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub liquidity: Option<Liquidity>,
    #[serde(default)]
    pub rating: Rating,
    #[serde(default)]
    pub rating_label: String,
    #[serde(default)]
    pub score: CompositeScore,
    #[serde(default)]
    pub degraded: Degraded,
    #[serde(default)]
    pub market_regime: Option<MarketRegime>,
    #[serde(default)]
    pub horizon: Option<HorizonClass>,
    #[serde(default)]
    pub structure: Option<StructureLevels>,
    /// The next 7 days' events, unread (the journal counts them).
    #[serde(default)]
    pub events: Option<Vec<Value>>,
    #[serde(default)]
    pub snapshot: DecisionSnapshot,
}

/// The nearest levels of the decision's `structure`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureLevels {
    pub nearest_support: Option<SrLevel>,
    pub nearest_resistance: Option<SrLevel>,
}

#[derive(Debug, Clone)]
pub struct DecisionDoc {
    pub d: DecisionCore,
    /// The answer as received (what the offline cache keeps).
    pub raw: Value,
    /// The whole decision, decoded on demand (`full`).
    whole: OnceCell<Option<Box<Decision>>>,
}

impl PartialEq for DecisionDoc {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw && self.d == other.d
    }
}

const VERDICTS: [&str; 6] = ["buy", "buyZone", "wait", "noPosition", "trim", "sell"];
const LEVELS: [&str; 5] = ["strong", "moderate", "waiting", "highRisk", "exit"];
const RATINGS: [&str; 6] = ["strongBuy", "buy", "hold", "reduce", "sell", "strongSell"];
/// Added fields the server type defaults: an explicit null is read as "not sent", like the TypeScript.
const DEFAULTED: [&str; 9] = ["rating", "ratingLabel", "score", "degraded", "noTrade", "counterArgument", "snapshot", "modelEvidence", "bot"];

fn is_array(v: Option<&Value>) -> bool {
    v.is_some_and(Value::is_array)
}

/// Set and not null (`x != null`).
fn set(v: Option<&Value>) -> Option<&Value> {
    v.filter(|x| !x.is_null())
}

/// The answer without the added fields sent as null (read as not sent, like the TypeScript).
fn cleaned(raw: &Value) -> Value {
    let mut v = raw.clone();
    if let Some(m) = v.as_object_mut() {
        m.retain(|k, x| !(x.is_null() && DEFAULTED.contains(&k.as_str())));
    }
    v
}

/// `parseDecision` for the asset screen: `parse_decision`, then the whole server's type (`full` is then set).
pub fn parse_decision_full(raw: Value) -> Result<DecisionDoc, String> {
    let doc = parse_decision(raw)?;
    let d: Decision = crate::web::json::from_value(&cleaned(&doc.raw)).map_err(|e| format!("Réponse de décision invalide ({e})"))?;
    let _ = doc.whole.set(Some(Box::new(d)));
    Ok(doc)
}

/// `parseDecision`: checks the fields the card relies on (the French message names the first bad field), then
/// decodes the fields of `DecisionCore` (the whole decision with `parse_decision_full`, or on demand: `full`).
pub fn parse_decision(raw: Value) -> Result<DecisionDoc, String> {
    let bad = |f: &str| format!("Réponse de décision invalide ({f})");
    let o = raw.as_object().ok_or_else(|| bad("corps"))?;
    let s = |k: &str| o.get(k).and_then(Value::as_str);
    if s("symbol").is_none() {
        return Err(bad("symbol"));
    }
    if !matches!(s("kind"), Some("crypto" | "stock")) {
        return Err(bad("kind"));
    }
    if !s("verdict").is_some_and(|v| VERDICTS.contains(&v)) {
        return Err(bad("verdict"));
    }
    if !s("level").is_some_and(|v| LEVELS.contains(&v)) {
        return Err(bad("level"));
    }
    for f in ["confidence", "asOf"] {
        if !o.get(f).is_some_and(Value::is_number) {
            return Err(bad(f));
        }
    }
    for f in ["families", "vetoes", "whyWait", "toBuy", "toSell", "scenarios", "pros", "cons", "sources"] {
        if !is_array(o.get(f)) {
            return Err(bad(f));
        }
    }
    if !is_array(o.get("setup").and_then(|x| x.get("steps"))) {
        return Err(bad("setup"));
    }
    let wn = o.get("whyNot");
    if !is_array(wn.and_then(|x| x.get("risks"))) || !is_array(wn.and_then(|x| x.get("invalidation"))) {
        return Err(bad("whyNot"));
    }
    if let Some(f) = set(o.get("fundamentals")) {
        if !matches!(f.get("kind").and_then(Value::as_str), Some("stock" | "crypto")) {
            return Err(bad("fundamentals.kind"));
        }
    }
    if let Some(p) = set(o.get("position")) {
        if !is_array(p.get("exits")) {
            return Err(bad("position.exits"));
        }
    }
    if s("disclaimer").is_none_or(str::is_empty) {
        return Err(bad("disclaimer"));
    }
    // Added fields: optional, checked when present.
    if let Some(r) = set(o.get("rating")) {
        if !r.as_str().is_some_and(|r| RATINGS.contains(&r)) {
            return Err(bad("rating"));
        }
    }
    let arrays = |k: &str, fields: &[&str]| set(o.get(k)).is_none_or(|x| fields.iter().all(|f| is_array(x.get(*f))));
    if !arrays("score", &["factors"]) {
        return Err(bad("score.factors"));
    }
    if !arrays("degraded", &["reasons"]) {
        return Err(bad("degraded.reasons"));
    }
    if !arrays("structure", &["levels", "relative"]) {
        return Err(bad("structure"));
    }
    if set(o.get("events")).is_some_and(|e| !e.is_array()) {
        return Err(bad("events"));
    }
    if !arrays("noTrade", &["reasons", "unchecked"]) {
        return Err(bad("noTrade"));
    }
    if set(o.get("actionZones")).is_some_and(|z| !is_array(z.get("zones")) || !z.get("price").is_some_and(Value::is_number)) {
        return Err(bad("actionZones"));
    }
    if !arrays("counterArgument", &["invalidators"]) {
        return Err(bad("counterArgument"));
    }
    if !arrays("snapshot", &["families"]) {
        return Err(bad("snapshot"));
    }
    if set(o.get("modelEvidence")).is_some_and(|m| !m.get("available").is_some_and(Value::is_boolean) || !m.get("text").is_some_and(Value::is_string))
    {
        return Err(bad("modelEvidence"));
    }
    if set(o.get("bot")).is_some_and(|b| {
        !b.get("available").is_some_and(Value::is_boolean)
            || !b.get("text").is_some_and(Value::is_string)
            || !b.get("counts").is_some_and(Value::is_boolean)
    }) {
        return Err(bad("bot"));
    }
    let d: DecisionCore = crate::web::json::from_value(&cleaned(&raw)).map_err(|e| bad(&e.to_string()))?;
    Ok(DecisionDoc { d, raw, whole: OnceCell::new() })
}

impl DecisionDoc {
    /// The whole server's decision (the asset screen), decoded once; None when it does not decode (an answer that
    /// `parse_decision_full` refuses).
    pub fn full(&self) -> Option<&Decision> {
        self.whole.get_or_init(|| crate::web::json::from_value(&cleaned(&self.raw)).ok().map(Box::new)).as_deref()
    }

    /// Key sent and not null (`d.x != null`).
    pub fn has(&self, key: &str) -> bool {
        set(self.raw.get(key)).is_some()
    }

    /// The 6-level rating; None for older answers (the verdict is then the headline).
    pub fn rating(&self) -> Option<Rating> {
        self.has("rating").then_some(self.d.rating)
    }
    pub fn score(&self) -> Option<&CompositeScore> {
        self.has("score").then_some(&self.d.score)
    }
    pub fn degraded(&self) -> Option<&Degraded> {
        self.has("degraded").then_some(&self.d.degraded)
    }
    pub fn no_trade(&self) -> Option<&NoTrade> {
        self.has("noTrade").then(|| self.full().map(|f| &f.no_trade)).flatten()
    }
    pub fn counter_argument(&self) -> Option<&CounterArgument> {
        self.has("counterArgument").then(|| self.full().map(|f| &f.counter_argument)).flatten()
    }
    pub fn snapshot(&self) -> Option<&DecisionSnapshot> {
        self.has("snapshot").then_some(&self.d.snapshot)
    }
    pub fn model_evidence(&self) -> Option<&ModelEvidence> {
        self.has("modelEvidence").then(|| self.full().map(|f| &f.model_evidence)).flatten()
    }
    pub fn bot(&self) -> Option<&BotView> {
        self.has("bot").then(|| self.full().map(|f| &f.bot)).flatten()
    }

    /// The next 7 days' events: None when not sent (older answers: no section), `Some(None)` when the calendar could
    /// not be loaded ("non vérifié").
    pub fn events(&self) -> Option<Option<&[CalendarEvent]>> {
        self.raw.get("events").map(|_| self.full().and_then(|f| f.events.as_deref()))
    }

    /// The track record's added details were sent (`t.regimes !== undefined || t.expectancy !== undefined`).
    pub fn track_details(&self) -> bool {
        self.raw.get("track").is_some_and(|t| t.get("regimes").is_some() || t.get("expectancy").is_some())
    }

    /// A field of the track record sent and not null (`t.spreadPct != null`).
    pub fn track_has(&self, key: &str) -> bool {
        set(self.raw.get("track").and_then(|t| t.get(key))).is_some()
    }

    /// Crypto developer activity: None when not sent (no block), `Some(None)` when sent as null ("non disponible").
    pub fn dev_activity(&self) -> Option<Option<&DevActivity>> {
        let Some(Fundamentals::Crypto(f)) = &self.full()?.fundamentals else { return None };
        self.raw.get("fundamentals").and_then(|x| x.get("devActivity")).map(|_| f.dev_activity.as_ref())
    }

    pub fn key(&self) -> String {
        format!("{}:{}", self.d.kind.as_str(), self.d.symbol)
    }
}

// ---------- Personal mode: what is sent (never quantities nor amounts) ----------

#[derive(Debug, Clone, PartialEq)]
pub struct WeightInput {
    pub symbol: String,
    pub kind: Kind,
    pub quantity: f64,
    pub average_price: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weight {
    pub symbol: String,
    pub kind: Kind,
    /// % of the portfolio, rounded to 0.1.
    pub weight: f64,
}

/// Share of each line in the total portfolio value (lines with a known price + cash), %, rounded to 0.1, largest
/// first, at most `max` (the server accepts 20). Lines without a price are left out; the same asset on two lines
/// (e.g. two brokers) gives one weight. `price` gives the price of "kind:SYMBOL".
pub fn portfolio_weights(holdings: &[WeightInput], price: impl Fn(&str) -> Option<f64>, cash: f64, max: usize) -> Vec<Weight> {
    let mut merged: Vec<(String, Kind, f64)> = Vec::new();
    for h in holdings {
        let Some(p) = price(&format!("{}:{}", h.kind.as_str(), h.symbol)).filter(|p| p.is_finite() && *p > 0.0) else { continue };
        if h.quantity <= 0.0 {
            continue;
        }
        match merged.iter_mut().find(|m| m.0 == h.symbol && m.1 == h.kind) {
            Some(m) => m.2 += h.quantity * p,
            None => merged.push((h.symbol.clone(), h.kind, h.quantity * p)),
        }
    }
    let total = merged.iter().map(|m| m.2).sum::<f64>() + if cash.is_finite() && cash > 0.0 { cash } else { 0.0 };
    if total <= 0.0 {
        return Vec::new();
    }
    let mut w: Vec<Weight> = merged
        .into_iter()
        .map(|(symbol, kind, value)| Weight { symbol, kind, weight: round(value / total * 1000.0) / 10.0 })
        .filter(|w| w.weight > 0.0)
        .collect();
    w.sort_by_dyn(|a, b| b.weight.total_cmp(&a.weight).then_with(|| a.symbol.cmp(&b.symbol)));
    w.truncate(max);
    w
}

/// Average cost of an asset over all its lines (weighted by quantity); None when not held.
pub fn average_cost(holdings: &[WeightInput], symbol: &str, kind: Kind) -> Option<f64> {
    let lines: Vec<&WeightInput> = holdings.iter().filter(|h| h.symbol == symbol && h.kind == kind && h.quantity > 0.0).collect();
    let q: f64 = lines.iter().map(|h| h.quantity).sum();
    if q == 0.0 {
        return None;
    }
    let c = lines.iter().map(|h| h.quantity * h.average_price).sum::<f64>() / q;
    (c > 0.0).then_some(c)
}

/// "BTC:crypto:35.2,AAPL:stock:10"
pub fn weights_param(w: &[Weight]) -> String {
    w.iter().map(|x| format!("{}:{}:{}", x.symbol, x.kind.as_str(), number_to_string(x.weight))).collect::<Vec<_>>().join(",")
}

/// Personal inputs of a held asset.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PersonalInput {
    pub cost: Option<f64>,
    pub weights: Vec<Weight>,
}

/// URL of the endpoint; personal inputs only when given (average cost rounded to the cent, weights), score weights
/// when not the defaults.
pub fn decision_url(symbol: &str, kind: Kind, personal: Option<&PersonalInput>, score_weights: Option<&ScoreWeights>) -> String {
    let mut u = format!("/api/decision?symbol={}&kind={}", encode_uri_component(symbol), kind.as_str());
    if let Some(c) = personal.and_then(|p| p.cost).filter(|c| c.is_finite() && *c > 0.0) {
        u.push_str(&format!("&cost={}", number_to_string(round(c * 100.0) / 100.0)));
    }
    if let Some(p) = personal.filter(|p| !p.weights.is_empty()) {
        u.push_str(&format!("&weights={}", encode_uri_component(&weights_param(&p.weights))));
    }
    if let Some(w) = score_weights.and_then(ScoreWeights::param) {
        u.push_str(&format!("&w={}", encode_uri_component(&w)));
    }
    u
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::engine::decision_types::{Level, Verdict};
    use crate::web::store::DEFAULT_SCORE_WEIGHTS;

    pub fn sample(name: &str) -> Value {
        let path = format!("{}/../backend/tests/samples/{name}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }
    pub fn btc() -> DecisionDoc {
        parse_decision(sample("decision-btc.json")).unwrap()
    }
    pub fn aapl() -> DecisionDoc {
        parse_decision(sample("decision-aapl.json")).unwrap()
    }
    pub fn guidance() -> DecisionDoc {
        parse_decision(sample("decision-guidance.json")).unwrap()
    }
    /// `{ ...d, ...over }` then parsed again.
    pub fn with(d: &DecisionDoc, over: Value) -> DecisionDoc {
        let mut raw = d.raw.clone();
        for (k, v) in over.as_object().unwrap() {
            raw[k] = v.clone();
        }
        parse_decision(raw).unwrap()
    }
    fn err(over: Value) -> String {
        let mut raw = sample("decision-btc.json");
        for (k, v) in over.as_object().unwrap() {
            raw[k] = v.clone();
        }
        parse_decision(raw).unwrap_err()
    }

    /// The core fields read the same values as the whole decision, on real answers.
    #[test]
    fn core_reads_as_the_whole_decision() {
        for f in ["decision-btc.json", "decision-aapl.json", "decision-guidance.json"] {
            let doc = parse_decision(sample(f)).unwrap();
            let (c, d) = (&doc.d, doc.full().unwrap());
            assert_eq!((&c.symbol, c.kind, &c.name, c.as_of, c.price, &c.mode), (&d.symbol, d.kind, &d.name, d.as_of, d.price, &d.mode), "{f}");
            assert_eq!(
                (c.verdict, &c.label, c.level, &c.level_label, c.confidence, &c.headline),
                (d.verdict, &d.label, d.level, &d.level_label, d.confidence, &d.headline)
            );
            assert_eq!((&c.families, &c.vetoes, &c.setup, &c.plan, &c.to_buy), (&d.families, &d.vetoes, &d.setup, &d.plan, &d.to_buy));
            assert_eq!((&c.pros, &c.cons, &c.liquidity, c.rating, &c.rating_label), (&d.pros, &d.cons, &d.liquidity, d.rating, &d.rating_label));
            assert_eq!(
                (&c.score, &c.degraded, &c.market_regime, &c.horizon, &c.snapshot),
                (&d.score, &d.degraded, &d.market_regime, &d.horizon, &d.snapshot)
            );
            let s = d.structure.as_ref();
            assert_eq!(
                c.structure.as_ref().map(|x| (&x.nearest_support, &x.nearest_resistance)),
                s.map(|x| (&x.nearest_support, &x.nearest_resistance))
            );
            assert_eq!(c.events.as_ref().map(Vec::len), d.events.as_ref().map(Vec::len));
            // The asset screen's decoding gives the same whole decision.
            assert_eq!(parse_decision_full(sample(f)).unwrap().full(), Some(d));
        }
        // An answer the whole type refuses: the core still reads it, the Décision card refuses it.
        let mut raw = sample("decision-btc.json");
        raw["track"] = serde_json::json!({ "period": 1 });
        let doc = parse_decision(raw.clone()).unwrap();
        assert!(doc.full().is_none() && doc.bot().is_none());
        assert!(parse_decision_full(raw).unwrap_err().starts_with("Réponse de décision invalide ("));
    }

    // decision.test.ts "contract samples decode through the card's own types"
    #[test]
    fn samples_decode() {
        let d = btc();
        assert_eq!((d.d.verdict, d.d.level, d.d.mode.as_str()), (Verdict::Wait, Level::Waiting, "informational"));
        assert!(d.full().unwrap().position.is_none());
        assert!(matches!(d.full().unwrap().fundamentals, Some(Fundamentals::Crypto(_))));
        assert!(!d.d.plan.as_ref().unwrap().acceptable);
        let a = aapl();
        assert_eq!((a.d.verdict, a.d.mode.as_str()), (Verdict::Trim, "personal"));
        assert_eq!(a.full().unwrap().position.as_ref().unwrap().exits.iter().filter(|e| e.now).count(), 1);
        assert!(a.full().unwrap().exposure.as_ref().unwrap().warning.as_ref().unwrap().contains("62"));
        let Some(Fundamentals::Stock(f)) = &a.full().unwrap().fundamentals else { panic!("stock fundamentals expected") };
        assert!(f.next_earnings.as_ref().unwrap().estimated);
    }

    #[test]
    fn outside_the_contract_is_refused_with_the_field_named() {
        assert!(err(serde_json::json!({ "verdict": "hodl" })).contains("(verdict)"));
        assert!(err(serde_json::json!({ "level": "red" })).contains("(level)"));
        assert!(err(serde_json::json!({ "fundamentals": { "kind": "bond" } })).contains("fundamentals.kind"));
        assert!(err(serde_json::json!({ "disclaimer": "" })).contains("disclaimer"));
        assert!(parse_decision(Value::Null).is_err());
        // Added fields: checked when present, older answers still decode.
        assert!(err(serde_json::json!({ "rating": "moon" })).contains("rating"));
        assert!(err(serde_json::json!({ "structure": { "levels": null } })).contains("structure"));
        assert!(err(serde_json::json!({ "modelEvidence": { "available": "yes" } })).contains("modelEvidence"));
        assert_eq!(btc().rating(), None);
        assert_eq!(with(&btc(), serde_json::json!({ "rating": "strongBuy", "ratingLabel": "ACHAT FORT" })).rating(), Some(Rating::StrongBuy));
        // An explicit null is "not sent".
        assert_eq!(with(&btc(), serde_json::json!({ "rating": null, "noTrade": null })).no_trade(), None);
    }

    // guidance.test.ts "guidance fields of the contract"
    #[test]
    fn guidance_fields() {
        let d = guidance();
        let nt = d.no_trade().unwrap();
        assert!(!nt.active && !nt.unchecked.is_empty());
        let z = d.full().unwrap().action_zones.as_ref().unwrap();
        assert_eq!(z.zones.iter().map(|z| z.kind.as_str()).collect::<Vec<_>>(), ["exit", "invalidation", "buy", "wait", "profit"]);
        assert_eq!(z.here.as_deref(), Some("wait"));
        assert_eq!(d.full().unwrap().scenarios.iter().map(|s| s.conditions.len()).collect::<Vec<_>>(), [3, 2, 3]);
        let unfolding: Vec<_> = d.full().unwrap().scenarios.iter().filter(|s| s.unfolding).map(|s| s.kind).collect();
        assert_eq!(unfolding, [d.full().unwrap().unfolding.as_ref().unwrap().kind]);
        let t = &d.counter_argument().unwrap().text;
        assert!(t.starts_with("🟢 Raisons favorables : ") && t.contains(" / 🔴 Raisons défavorables : "), "{t}");
        assert_eq!(d.snapshot().unwrap().families.len(), d.d.families.len());
        let bad = |k: &str, v: Value| {
            let mut raw = sample("decision-guidance.json");
            raw[k] = v;
            parse_decision(raw).unwrap_err()
        };
        assert!(bad("noTrade", serde_json::json!({ "active": true })).contains("noTrade"));
        assert!(bad("actionZones", serde_json::json!({ "zones": [] })).contains("actionZones"));
        assert!(bad("counterArgument", serde_json::json!({})).contains("counterArgument"));
        assert!(bad("snapshot", serde_json::json!({})).contains("snapshot"));
        let old = btc();
        assert!(old.no_trade().is_none() && old.counter_argument().is_none() && old.full().unwrap().action_zones.is_none());
        assert_eq!(old.events(), None);
    }

    /// The server's type reads back exactly what it wrote (a real answer of the server).
    #[test]
    fn json_round_trip() {
        let raw = sample("decision-guidance.json");
        let d = parse_decision(raw.clone()).unwrap();
        let mut diffs = Vec::new();
        let mut out = crate::js::to_value(d.full().unwrap());
        // Fields this sample predates are the type's defaults (the card reads them as absent).
        out.as_object_mut().unwrap().retain(|k, _| raw.get(k).is_some());
        diff(&out, &crate::js::to_value(&raw), String::new(), &mut diffs);
        assert!(diffs.is_empty(), "{diffs:?}");
        assert!(d.model_evidence().is_none() && d.bot().is_none());
        // Live answers of a local server (optional: `ALTIM_DECISION_SAMPLES=<dir>` holding decision-*.json).
        if let Ok(dir) = std::env::var("ALTIM_DECISION_SAMPLES") {
            for e in std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("decision-")) {
                let raw: Value = serde_json::from_str(&std::fs::read_to_string(e.path()).unwrap()).unwrap();
                let d = parse_decision(raw.clone()).unwrap();
                let mut diffs = Vec::new();
                let mut raw = raw;
                // The exchange rate of the server's texts, outside the decision itself.
                raw.as_object_mut().unwrap().remove("fx");
                diff(&crate::js::to_value(d.full().unwrap()), &crate::js::to_value(&raw), String::new(), &mut diffs);
                assert!(diffs.is_empty(), "{:?}: {diffs:?}", e.path());
                assert!(d.model_evidence().is_some() && d.bot().is_some() && d.rating().is_some());
            }
        }
    }

    /// JSON pointers where two values differ.
    pub fn diff(a: &Value, b: &Value, path: String, out: &mut Vec<String>) {
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                    diff(x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), format!("{path}/{k}"), out);
                }
            }
            (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    diff(p, q, format!("{path}/{i}"), out);
                }
            }
            _ if a != b => out.push(format!("{path}: {a} != {b}")),
            _ => {}
        }
    }

    // decision.test.ts "personal mode inputs"
    fn holdings() -> Vec<WeightInput> {
        let h = |symbol: &str, kind, quantity, average_price| WeightInput { symbol: symbol.into(), kind, quantity, average_price };
        vec![
            h("BTC", Kind::Crypto, 0.5, 60_000.0),
            h("AAPL", Kind::Stock, 100.0, 275.0),
            h("AAPL", Kind::Stock, 100.0, 325.0),
            h("XYZ", Kind::Stock, 10.0, 5.0),
        ]
    }
    fn prices(k: &str) -> Option<f64> {
        match k {
            "crypto:BTC" => Some(80_000.0),
            "stock:AAPL" => Some(150.0),
            _ => None,
        }
    }

    #[test]
    fn weights() {
        // BTC 40 000, AAPL 200 × 150 = 30 000, cash 30 000 → total 100 000.
        let w = portfolio_weights(&holdings(), prices, 30_000.0, 20);
        assert_eq!(
            w,
            vec![
                Weight { symbol: "BTC".into(), kind: Kind::Crypto, weight: 40.0 },
                Weight { symbol: "AAPL".into(), kind: Kind::Stock, weight: 30.0 }
            ]
        );
        assert_eq!(weights_param(&w), "BTC:crypto:40,AAPL:stock:30");
        let many: Vec<WeightInput> =
            (0..25).map(|i| WeightInput { symbol: format!("S{i}"), kind: Kind::Stock, quantity: (i + 1) as f64, average_price: 0.0 }).collect();
        let w = portfolio_weights(&many, |_| Some(10.0), 0.0, 20);
        assert_eq!((w.len(), w[0].symbol.as_str()), (20, "S24"));
        assert!(portfolio_weights(&[], prices, 0.0, 20).is_empty());
        assert_eq!(average_cost(&holdings(), "AAPL", Kind::Stock), Some(300.0));
        assert_eq!(average_cost(&holdings(), "ETH", Kind::Crypto), None);
    }

    #[test]
    fn urls() {
        assert_eq!(decision_url("BTC", Kind::Crypto, None, None), "/api/decision?symbol=BTC&kind=crypto");
        let p = PersonalInput {
            cost: Some(275.456),
            weights: vec![
                Weight { symbol: "AAPL".into(), kind: Kind::Stock, weight: 10.0 },
                Weight { symbol: "BTC".into(), kind: Kind::Crypto, weight: 35.2 },
            ],
        };
        assert_eq!(
            decision_url("AAPL", Kind::Stock, Some(&p), None),
            format!("/api/decision?symbol=AAPL&kind=stock&cost=275.46&weights={}", encode_uri_component("AAPL:stock:10,BTC:crypto:35.2"))
        );
        // Composite score weights: the defaults are not sent.
        let w = ScoreWeights { tech: 50, macro_: 0, ..DEFAULT_SCORE_WEIGHTS };
        assert_eq!(
            decision_url("BTC", Kind::Crypto, None, Some(&w)),
            format!("/api/decision?symbol=BTC&kind=crypto&w={}", encode_uri_component("tech:50,mom:18,fund:20,sent:10,news:10,macro:0"))
        );
        assert_eq!(decision_url("BTC", Kind::Crypto, None, Some(&DEFAULT_SCORE_WEIGHTS)), "/api/decision?symbol=BTC&kind=crypto");
    }
}
