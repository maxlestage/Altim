//! Offline cache of the decisions (decision.ts): the last good decision per asset, in this browser only
//! ("altim.decision.v1" = `{ "crypto:BTC": { at, personal, decision } }`, the 40 most recent, newest first), shown
//! while loading or when the server is unreachable, read by the Radar's badges and sort. Pure: the frontend passes
//! the stored text and writes the one returned.
use serde_json::{Map, Value, json};

use super::doc::{DecisionDoc, parse_decision};
use crate::engine::decision_types::{Level, Verdict};
use crate::types::Kind;
use crate::web::sorting::Sorting;

pub const CACHE_KEY: &str = "altim.decision.v1";
pub const CACHE_MAX: usize = 40;
/// A cached decision older than this is too old for the Radar (badge, sort, lists).
pub const RECENT_MS: f64 = 12.0 * 3_600_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct CachedDecision {
    /// When it was received (ms; NaN when the stored value is not a number).
    pub at: f64,
    /// Personal mode (with the user's average cost and weights); None when the stored value is not a boolean.
    pub personal: Option<bool>,
    pub decision: DecisionDoc,
}

/// Every stored entry, unparsed (anything unreadable is an empty cache).
pub fn read_all(raw: Option<&str>) -> Map<String, Value> {
    match raw.and_then(|r| serde_json::from_str::<Value>(r).ok()) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

/// One entry of `read_all`, validated like `parseDecision` (a damaged entry is no entry).
pub fn entry(all: &Map<String, Value>, kind: Kind, symbol: &str) -> Option<CachedDecision> {
    let c = all.get(&format!("{}:{symbol}", kind.as_str())).filter(|c| !c.is_null())?;
    let decision = parse_decision(c.get("decision").cloned().unwrap_or(Value::Null)).ok()?;
    Some(CachedDecision {
        at: c.get("at").and_then(Value::as_f64).unwrap_or(f64::NAN),
        personal: c.get("personal").and_then(Value::as_bool),
        decision,
    })
}

/// `cachedDecision`: the last decision kept for this asset.
pub fn cached_decision(raw: Option<&str>, kind: Kind, symbol: &str) -> Option<CachedDecision> {
    entry(&read_all(raw), kind, symbol)
}

/// The new stored text after receiving a decision (`cacheDecision`, without the configuration diff the caller
/// records next to it).
pub fn cache_text(raw: Option<&str>, d: &DecisionDoc, personal: bool, now: f64) -> String {
    let mut all = read_all(raw);
    all.insert(d.key(), json!({ "at": now, "personal": personal, "decision": d.raw }));
    let at = |v: &Value| v.get("at").and_then(Value::as_f64).filter(|a| !a.is_nan()).unwrap_or(f64::NEG_INFINITY);
    let mut keep: Vec<(String, Value)> = all.into_iter().collect();
    keep.sort_by_dyn(|a, b| at(&b.1).total_cmp(&at(&a.1)));
    keep.truncate(CACHE_MAX);
    crate::js::to_value(&Value::Object(keep.into_iter().collect())).to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecentVerdict {
    pub verdict: Verdict,
    pub label: String,
    pub level: Level,
    pub at: f64,
}

/// Verdict seen less than `max_age_ms` ago (Radar badge, no extra request).
pub fn recent_verdict(raw: Option<&str>, kind: Kind, symbol: &str, now: f64, max_age_ms: f64) -> Option<RecentVerdict> {
    let c = cached_decision(raw, kind, symbol)?;
    (now - c.at <= max_age_ms).then(|| RecentVerdict {
        verdict: c.decision.d.verdict,
        label: c.decision.d.label.clone(),
        level: c.decision.d.level,
        at: c.at,
    })
}

/// The cached decision when under 12 h old (`freshDecision` of Radar.tsx, the badge's rule).
pub fn fresh(c: Option<&CachedDecision>, now: f64) -> Option<&CachedDecision> {
    c.filter(|c| now - c.at <= RECENT_MS)
}

#[cfg(test)]
mod tests {
    use super::super::doc::tests::btc;
    use super::*;

    // decision.test.ts "offline cache"
    #[test]
    fn last_decision_per_asset_and_recent_verdict() {
        let t = 1_790_612_000_000.0;
        let s = cache_text(None, &btc(), false, t);
        assert_eq!(cached_decision(Some(&s), Kind::Crypto, "BTC").unwrap().decision.d.label, "ATTENDRE");
        assert_eq!(recent_verdict(Some(&s), Kind::Crypto, "BTC", t + 3_600_000.0, RECENT_MS).unwrap().label, "ATTENDRE");
        assert_eq!(recent_verdict(Some(&s), Kind::Crypto, "BTC", t + 13.0 * 3_600_000.0, RECENT_MS), None);
        assert_eq!(cached_decision(Some(&s), Kind::Stock, "AAPL"), None);
        // Same JSON as the TypeScript: the decision exactly as received, `at` as an integer.
        let v: Value = serde_json::from_str(&s).unwrap();
        let mut diffs = Vec::new();
        super::super::doc::tests::diff(&v["crypto:BTC"]["decision"], &crate::js::to_value(&btc().raw), String::new(), &mut diffs);
        assert!(diffs.is_empty(), "{diffs:?}");
        assert!(s.starts_with(r#"{"crypto:BTC":{"at":1790612000000,"personal":false,"decision":{"symbol":"BTC""#), "{}", &s[..120]);
    }

    #[test]
    fn newest_first_at_most_40_and_damage_tolerated() {
        let mut s: Option<String> = Some("{nope".into());
        for i in 0..45 {
            let mut raw = btc().raw.clone();
            raw["symbol"] = json!(format!("S{i}"));
            let d = parse_decision(raw).unwrap();
            s = Some(cache_text(s.as_deref(), &d, i % 2 == 0, i as f64));
        }
        let all = read_all(s.as_deref());
        assert_eq!(all.len(), CACHE_MAX);
        assert_eq!(all.keys().next().unwrap(), "crypto:S44");
        assert!(cached_decision(s.as_deref(), Kind::Crypto, "S0").is_none());
        let c = cached_decision(s.as_deref(), Kind::Crypto, "S44").unwrap();
        assert_eq!((c.at, c.personal), (44.0, Some(true)));
        // A damaged entry is dropped, never trusted.
        let bad = r#"{"crypto:BTC":{"at":1,"personal":false,"decision":{"symbol":"BTC"}}}"#;
        assert!(cached_decision(Some(bad), Kind::Crypto, "BTC").is_none());
    }
}
