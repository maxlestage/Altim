//! Automatic journal kept in this browser only (localStorage "altim.journal.v1", journal-store.ts); nothing is sent to
//! the server. Unreadable saved data is ignored (kept aside as "altim.journal.v1.invalid" at the next write). Entries
//! are written when a simulated purchase is confirmed or a real purchase / sale is saved in « Mes avoirs », then
//! completed in the background with the market data of that moment (macro stress, ATR, relative volume, and the
//! decision when none was loaded yet); a failed fetch only leaves the field unknown. Pure part:
//! `altim_core::web::trading::journal_store`.
use std::cell::Cell;
use std::rc::Rc;

use altim_core::types::{Candle, Kind};
use altim_core::web::decision::doc::DecisionCore;
use altim_core::web::market::CandlesReply;
use altim_core::web::trading::journal::{
    EntryPatch, JournalEntry, JournalState, NewEntry, add_entry, create_entry, journal_json, patch_entry, remove_entry,
};
use altim_core::web::trading::journal_store::{
    JOURNAL_INVALID_KEY, JOURNAL_KEY, MacroLite, RealTrade, SavedJournal, enrich_patch, is_recent_decision, parse_saved_journal, real_trade_entry,
};
use yew::prelude::*;

use crate::api::{enc, get};
use crate::state::{Store, local_get, local_set, now, random_uuid, use_store};

fn read() -> SavedJournal {
    parse_saved_journal(local_get(JOURNAL_KEY).as_deref())
}

thread_local! {
    pub static JOURNAL: Store<SavedJournal> = Store::new(read());
    static LISTENING: Cell<bool> = const { Cell::new(false) };
}

fn listen() {
    if LISTENING.with(|l| l.replace(true)) {
        return;
    }
    if let Some(w) = web_sys::window() {
        gloo::events::EventListener::new(&w, "storage", |e| {
            if crate::app::simulation::store::storage_key(e).as_deref() == Some(JOURNAL_KEY) {
                JOURNAL.with(|s| s.set(read()));
            }
        })
        .forget();
    }
}

/// The saved journal; the component re-renders when it changes.
#[hook]
pub fn use_journal() -> Rc<SavedJournal> {
    listen();
    use_store(&JOURNAL)
}

fn current() -> Rc<SavedJournal> {
    JOURNAL.with(|s| s.get())
}

fn save(update: impl FnOnce(&JournalState) -> JournalState) {
    let cur = current();
    let state = update(&cur.state);
    if cur.error.is_some() {
        if let Some(bad) = local_get(JOURNAL_KEY) {
            local_set(JOURNAL_INVALID_KEY, &bad);
        }
    }
    local_set(JOURNAL_KEY, &journal_json(&state));
    JOURNAL.with(|s| s.set(SavedJournal { state, error: None }));
}

pub fn set_journal_note(id: &str, note: &str) {
    let patch = EntryPatch { note: Some(note.to_string()), ..Default::default() };
    save(|s| patch_entry(s, id, &patch));
}

pub fn delete_journal_entry(id: &str) {
    save(|s| remove_entry(s, id));
}

/// `/api/candles?symbol=…&kind=…&interval=1d` (daily candles, cached by the server).
pub async fn daily_candles(symbol: &str, kind: Kind) -> Result<Vec<Candle>, crate::api::ApiError> {
    get::<CandlesReply>(&format!("/api/candles?symbol={}&kind={}&interval=1d", enc(symbol), kind.as_str())).await.map(|r| r.candles)
}

/// Market data of the moment, fetched right after the entry is written (cached by the server).
async fn enrich(id: String, symbol: String, kind: Kind, at: f64, has_decision: bool) {
    let decision_url = format!("/api/decision?symbol={}&kind={}{}", enc(&symbol), kind.as_str(), crate::money::cur_param());
    let (macro_, candles, decision) = futures::join!(get::<MacroLite>("/api/macro"), daily_candles(&symbol, kind), async {
        if has_decision { None } else { get::<DecisionCore>(&decision_url).await.ok() }
    });
    let (macro_, candles) = (macro_.ok(), candles.ok());
    let entry = current().state.entries.iter().find(|e| e.id == id).cloned();
    let patch = enrich_patch(entry.as_ref(), at, macro_.as_ref(), candles.as_deref(), decision.as_ref());
    save(|s| patch_entry(s, &id, &patch));
}

/// Writes an entry and completes it in the background. Returns its id.
pub fn record_entry(e: JournalEntry) -> String {
    let (id, symbol, kind, at, has_decision) = (e.id.clone(), e.symbol.clone(), e.kind, e.created_at, e.decision.is_some());
    save(|s| add_entry(s, e));
    let rid = id.clone();
    wasm_bindgen_futures::spawn_local(enrich(rid, symbol, kind, at, has_decision));
    id
}

/// `record`: a new entry from what the screen knows (its id and, when absent, its time are filled here).
pub fn record(n: NewEntry) -> String {
    let now = if n.now > 0.0 { n.now } else { now() };
    record_entry(create_entry(&NewEntry { id: random_uuid(), now, ..n }))
}

/// A real purchase or sale saved in « Mes avoirs » (`recordRealTrade`). None when the price or the
/// quantity is not positive (nothing written).
pub fn record_real_trade(t: &RealTrade) -> Option<String> {
    // The decision seen on this device for this asset, when recent enough to be "the one of the moment"; otherwise the
    // background enrichment fetches the current one.
    let now = now();
    let cached = crate::app::asset::store::cached(t.kind, &t.symbol).filter(|c| is_recent_decision(c.at, c.decision.d.as_of as f64, now));
    let e = real_trade_entry(t, random_uuid(), now, cached.as_ref().map(|c| &c.decision.d))?;
    Some(record_entry(e))
}
