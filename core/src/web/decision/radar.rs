//! The Radar's pure helpers (Radar.tsx): the order of the list (the user's own, the largest moves first, or the
//! strongest decisions first, remembered in "altim.radar.sort"), the rows kept when a refresh fails, and the
//! "Opportunités détectées" list.
use std::collections::HashMap;

use super::cache::CachedDecision;
use super::format::rating_rank;
use super::reports::RadarRow;
use crate::engine::decision_types::Verdict;
use crate::web::store::WatchItem;

pub const SORT_KEY: &str = "altim.radar.sort";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadarSort {
    #[default]
    Mine,
    Change,
    Signal,
}

impl RadarSort {
    /// The remembered choice ("mine" when absent or unknown).
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("change") => RadarSort::Change,
            Some("signal") => RadarSort::Signal,
            _ => RadarSort::Mine,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            RadarSort::Mine => "mine",
            RadarSort::Change => "change",
            RadarSort::Signal => "signal",
        }
    }
}

/// The watchlist in the chosen order. `change` gives the live change of an asset key (else the radar row's);
/// `decision` its fresh cached decision (under 12 h).
pub fn sorted<'a>(
    watchlist: &'a [WatchItem],
    sort: RadarSort,
    change: impl Fn(&WatchItem) -> Option<f64>,
    decision: impl Fn(&WatchItem) -> Option<&'a CachedDecision>,
) -> Vec<&'a WatchItem> {
    let mut out: Vec<&WatchItem> = watchlist.iter().collect();
    match sort {
        RadarSort::Mine => {}
        RadarSort::Change => {
            let key = |w: &WatchItem| change(w).unwrap_or(-1.0).abs();
            let keys: HashMap<String, f64> = watchlist.iter().map(|w| (w.key(), key(w))).collect();
            out.sort_by(|a, b| keys[&b.key()].total_cmp(&keys[&a.key()]));
        }
        RadarSort::Signal => {
            // The full decision's rating (buy side first), then its confidence.
            let key = |w: &WatchItem| {
                let d = decision(w);
                (rating_rank(d.and_then(|c| c.decision.rating())), d.map(|c| c.decision.d.confidence).unwrap_or(0.0))
            };
            let keys: HashMap<String, (u8, f64)> = watchlist.iter().map(|w| (w.key(), key(w))).collect();
            out.sort_by(|a, b| {
                let (ra, ca) = keys[&a.key()];
                let (rb, cb) = keys[&b.key()];
                ra.cmp(&rb).then(cb.total_cmp(&ca))
            });
        }
    }
    out
}

/// A refresh's rows merged into the previous ones: an asset in error keeps its last valid data (with the error).
pub fn merge_rows(prev: &HashMap<String, RadarRow>, data: Vec<RadarRow>) -> HashMap<String, RadarRow> {
    let mut next = prev.clone();
    for r in data {
        let k = crate::web::store::asset_key(&r.symbol, r.kind);
        let row = match (&r.error, prev.get(&k)) {
            (Some(e), Some(p)) if p.signal.is_some() => RadarRow { error: Some(e.clone()), ..p.clone() },
            _ => r,
        };
        next.insert(k, row);
    }
    next
}

/// Assets whose full decision is ACHETER or ZONE D'ACHAT (not the 4 h technical signal alone), most confident first,
/// three at most.
pub fn opportunities<'a>(
    watchlist: &'a [WatchItem],
    decision: impl Fn(&WatchItem) -> Option<&'a CachedDecision>,
) -> Vec<(&'a WatchItem, &'a CachedDecision)> {
    let mut out: Vec<(&WatchItem, &CachedDecision)> = watchlist
        .iter()
        .filter_map(|w| decision(w).map(|d| (w, d)))
        .filter(|(_, d)| matches!(d.decision.d.verdict, Verdict::Buy | Verdict::BuyZone))
        .collect();
    out.sort_by(|a, b| b.1.decision.d.confidence.total_cmp(&a.1.decision.d.confidence));
    out.truncate(3);
    out
}

#[cfg(test)]
mod tests {
    use super::super::cache::CachedDecision;
    use super::super::doc::tests::{btc, with};
    use super::*;
    use crate::types::Kind;
    use serde_json::json;

    fn w(s: &str) -> WatchItem {
        WatchItem { symbol: s.into(), kind: Kind::Crypto, name: s.into() }
    }

    #[test]
    fn sort_choice() {
        assert_eq!(RadarSort::parse(Some("signal")), RadarSort::Signal);
        assert_eq!(RadarSort::parse(Some("x")), RadarSort::Mine);
        assert_eq!(RadarSort::parse(None).as_str(), "mine");
    }

    #[test]
    fn orders_and_lists() {
        let list = vec![w("A"), w("B"), w("C")];
        let change = |x: &WatchItem| match x.symbol.as_str() {
            "A" => Some(1.0),
            "B" => Some(-5.0),
            _ => None,
        };
        let by_change: Vec<&str> = sorted(&list, RadarSort::Change, change, |_| None).iter().map(|x| x.symbol.as_str()).collect();
        assert_eq!(by_change, ["B", "A", "C"]);
        let c = |over: serde_json::Value| CachedDecision { at: 0.0, personal: Some(false), decision: with(&btc(), over) };
        let (a, b) = (
            c(json!({ "rating": "hold", "verdict": "wait", "confidence": 80 })),
            c(json!({ "rating": "buy", "verdict": "buyZone", "confidence": 40 })),
        );
        let dec = |x: &WatchItem| match x.symbol.as_str() {
            "A" => Some(&a),
            "B" => Some(&b),
            _ => None,
        };
        let by_signal: Vec<&str> = sorted(&list, RadarSort::Signal, |_| None, dec).iter().map(|x| x.symbol.as_str()).collect();
        assert_eq!(by_signal, ["B", "A", "C"]);
        let opp = opportunities(&list, dec);
        assert_eq!(opp.len(), 1);
        assert_eq!(opp[0].0.symbol, "B");
    }

    #[test]
    fn failed_rows_keep_their_data() {
        let row = |error: Option<&str>| RadarRow {
            symbol: "BTC".into(),
            kind: Kind::Crypto,
            name: "Bitcoin".into(),
            price: error.is_none().then_some(10.0),
            change: None,
            price_sources: None,
            signal: error.is_none().then_some(super::super::reports::RadarSignal {
                action: crate::engine::signal::Action::Buy,
                score: 1.0,
                confidence: 2.0,
            }),
            reliability: None,
            sparkline: vec![],
            error: error.map(String::from),
        };
        let first = merge_rows(&HashMap::new(), vec![row(None)]);
        let next = merge_rows(&first, vec![row(Some("indisponible"))]);
        let r = &next["crypto:BTC"];
        assert_eq!((r.price, r.error.as_deref()), (Some(10.0), Some("indisponible")));
        assert!(merge_rows(&HashMap::new(), vec![row(Some("x"))])["crypto:BTC"].signal.is_none());
    }
}
