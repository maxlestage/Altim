//! Pure part of web/src/webapp/journal-store.ts: the journal kept in this browser only (localStorage
//! "altim.journal.v1"); nothing is sent to the server. Unreadable saved data is ignored (kept aside as
//! "altim.journal.v1.invalid" at the next write). Entries are written when a simulated purchase is confirmed or a
//! real purchase / sale is saved in « Mes avoirs », then completed in the background with the market data of that
//! moment (macro stress, ATR, relative volume, and the decision when none was loaded yet); a failed fetch only leaves
//! the field unknown. The browser side (storage, fetches) is `frontend/src/app/journal/store.rs`.
use serde::Deserialize;
use serde_json::Value;

use super::journal::{
    EntryPatch, JournalEntry, JournalSide, JournalState, MarketPatch, NewEntry, SOURCE_REAL, create_entry, empty_journal, market_from_candles,
    market_from_decision, parse_journal_state, snapshot_decision,
};
use super::positive;
use crate::engine::decision_types::Decision;
use crate::types::{Candle, Kind};

pub const JOURNAL_KEY: &str = "altim.journal.v1";
pub const JOURNAL_INVALID_KEY: &str = "altim.journal.v1.invalid";
/// A decision cached on the device is used for an entry when it is younger than this.
pub const DECISION_MAX_AGE: f64 = 6.0 * 3_600_000.0;

pub const BAD_JOURNAL_MESSAGE: &str = "Le journal enregistré dans ce navigateur est illisible : il a été ignoré.";

#[derive(Debug, Clone, PartialEq)]
pub struct SavedJournal {
    pub state: JournalState,
    pub error: Option<String>,
}

impl Default for SavedJournal {
    fn default() -> Self {
        SavedJournal { state: empty_journal(), error: None }
    }
}

pub fn parse_saved_journal(raw: Option<&str>) -> SavedJournal {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else { return SavedJournal::default() };
    match serde_json::from_str::<Value>(raw).ok().as_ref().and_then(parse_journal_state) {
        Some(state) => SavedJournal { state, error: None },
        None => SavedJournal { state: empty_journal(), error: Some(BAD_JOURNAL_MESSAGE.into()) },
    }
}

/// The decision seen on this device for this asset is "the one of the moment" (`recentDecision`): cached less than
/// 6 h ago and computed less than 6 h ago.
pub fn is_recent_decision(cached_at: f64, as_of: f64, now: f64) -> bool {
    now - cached_at <= DECISION_MAX_AGE && now - as_of <= DECISION_MAX_AGE
}

/// A real purchase or sale saved in « Mes avoirs » (`recordRealTrade`).
#[derive(Debug, Clone, PartialEq)]
pub struct RealTrade {
    pub side: JournalSide,
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    /// USD.
    pub price: f64,
    pub quantity: f64,
    /// USD, the holding's stop.
    pub stop: Option<f64>,
    pub note: String,
    pub ref_id: Option<String>,
}

/// The entry a real trade writes (None: no positive price or quantity, nothing is written). `decision`: the recent
/// decision cached on the device for this asset (`is_recent_decision`), if any.
pub fn real_trade_entry(t: &RealTrade, id: String, now: f64, decision: Option<&Decision>) -> Option<JournalEntry> {
    if !positive(t.price) || !positive(t.quantity) {
        return None;
    }
    Some(create_entry(&NewEntry {
        id,
        now,
        source: SOURCE_REAL,
        side: Some(t.side),
        symbol: t.symbol.clone(),
        kind: Some(t.kind),
        name: t.name.clone(),
        price: t.price,
        quantity: Some(t.quantity),
        amount: Some(t.price * t.quantity),
        stop: t.stop,
        targets: vec![],
        note: t.note.clone(),
        ref_id: t.ref_id.clone(),
        decision,
    }))
}

/// Macro regime of `/api/macro` (the fields the journal reads).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MacroRegime {
    pub kind: String,
    pub label: String,
}

/// `/api/macro` (MacroInfo): stress score /100, level, market regime; only what the journal keeps.
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(default)]
pub struct MacroLite {
    pub score: Option<f64>,
    pub level: Option<String>,
    pub regime: Option<MacroRegime>,
}

/// The market data fetched right after an entry is written, as a patch of that entry (`enrich`): macro stress,
/// ATR % and relative volume on the daily candles closed before the entry (the decision's relative volume is kept
/// when it had one), the decision's own market fields when it arrives now, and the macro regime as a fallback.
pub fn enrich_patch(
    entry: Option<&JournalEntry>,
    at: f64,
    macro_: Option<&MacroLite>,
    candles: Option<&[Candle]>,
    decision: Option<&Decision>,
) -> EntryPatch {
    let mut m = MarketPatch::default();
    if let Some(x) = macro_ {
        m.macro_score = Some(x.score);
        m.macro_level = Some(x.level.clone());
    }
    if let Some(c) = candles.filter(|c| !c.is_empty()) {
        let (atr_pct, rel) = market_from_candles(c, at);
        m.atr_pct = Some(atr_pct);
        if entry.is_none_or(|e| e.market.relative_volume.is_none()) {
            m.relative_volume = Some(rel);
        }
    }
    if let Some(d) = decision {
        // Object.assign of the decision's known fields only.
        let f = market_from_decision(Some(d));
        if f.regime.is_some() {
            m.regime = Some(f.regime);
        }
        if f.regime_label.is_some() {
            m.regime_label = Some(f.regime_label);
        }
        if f.relative_volume.is_some() {
            m.relative_volume = Some(f.relative_volume);
        }
        if f.events.is_some() {
            m.events = Some(f.events);
        }
    }
    let regime_unknown = entry.is_some_and(|e| e.market.regime.is_none());
    if regime_unknown && m.regime.as_ref().is_none_or(|r| r.is_none()) {
        if let Some(r) = macro_.and_then(|x| x.regime.as_ref()) {
            m.regime = Some(Some(r.kind.clone()));
            m.regime_label = Some(Some(r.label.clone()));
        }
    }
    EntryPatch { note: None, market: m, decision: decision.map(snapshot_decision) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::trading::journal::tests::{at, decision, new_entry};
    use crate::web::trading::journal::{add_entry, journal_json, patch_entry};
    use serde_json::json;

    #[test]
    fn saved_journal() {
        assert_eq!(parse_saved_journal(None), SavedJournal::default());
        let d = decision(json!({}));
        let s = add_entry(&empty_journal(), create_entry(&new_entry(Some(&d))));
        assert_eq!(parse_saved_journal(Some(&journal_json(&s))).state.entries.len(), 1);
        assert!(parse_saved_journal(Some("{oops")).error.unwrap().contains("illisible"));
        assert!(parse_saved_journal(Some(r#"{"version":2,"entries":[]}"#)).error.unwrap().contains("illisible"));
    }

    #[test]
    fn real_trades() {
        let t = RealTrade {
            side: JournalSide::Buy,
            symbol: "AAPL".into(),
            kind: Kind::Stock,
            name: "Apple".into(),
            price: 200.0,
            quantity: 2.0,
            stop: Some(180.0),
            note: " ok ".into(),
            ref_id: Some("h1".into()),
        };
        let e = real_trade_entry(&t, "r1".into(), at(), None).unwrap();
        assert_eq!((e.source.as_str(), e.amount, e.stop, e.note.as_str()), ("real", Some(400.0), Some(180.0), "ok"));
        assert_eq!(e.signal, "Aucune décision chargée pour cet actif à ce moment-là");
        assert_eq!(real_trade_entry(&RealTrade { quantity: 0.0, ..t }, "r2".into(), at(), None), None);
        assert!(is_recent_decision(at(), at() - 3_600_000.0, at() + 3_600_000.0));
        assert!(!is_recent_decision(at(), at() - 7.0 * 3_600_000.0, at()));
    }

    #[test]
    fn enrichment() {
        let e = create_entry(&new_entry(None));
        let s = add_entry(&empty_journal(), e.clone());
        let m = MacroLite {
            score: Some(31.0),
            level: Some("tense".into()),
            regime: Some(MacroRegime { kind: "neutral".into(), label: "Neutre".into() }),
        };
        let p = enrich_patch(Some(&e), e.created_at, Some(&m), None, None);
        let s2 = patch_entry(&s, &e.id, &p);
        let mk = &s2.entries[0].market;
        assert_eq!(
            (mk.macro_score, mk.macro_level.as_deref(), mk.regime.as_deref(), mk.regime_label.as_deref()),
            (Some(31.0), Some("tense"), Some("neutral"), Some("Neutre"))
        );
        // A decision arriving now: its regime wins over the macro one, and the entry gets its snapshot.
        let d = decision(json!({}));
        let p = enrich_patch(Some(&e), e.created_at, Some(&m), Some(&[]), Some(&d));
        let s3 = patch_entry(&s, &e.id, &p);
        assert_eq!(s3.entries[0].market.regime.as_deref(), Some("riskOn"));
        assert!(s3.entries[0].decision.is_some());
        assert!(s3.entries[0].signal.starts_with("Achat (Signal fort)"));
    }
}
