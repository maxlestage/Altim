//! Simulated portfolio (paper trading), kept in this browser only (localStorage "altim.paper.v1", paper-store.ts);
//! nothing is sent to the server. A saved state that fails `is_paper_state` is ignored with a message, never a crash;
//! the unreadable text is kept aside ("altim.paper.v1.invalid") the first time a new simulation overwrites it.
use std::cell::Cell;
use std::rc::Rc;

use altim_core::web::trading::paper::{PaperState, new_paper, paper_json};
use altim_core::web::trading::paper_ui::{PAPER_INVALID_KEY, PAPER_KEY, SavedPaper, parse_saved_paper};
use yew::prelude::*;

use crate::state::{Store, local_get, local_set, now, use_store};

fn read() -> SavedPaper {
    parse_saved_paper(local_get(PAPER_KEY).as_deref())
}

thread_local! {
    pub static PAPER: Store<SavedPaper> = Store::new(read());
    static LISTENING: Cell<bool> = const { Cell::new(false) };
}

/// The `key` of a `storage` event (read by name: no StorageEvent binding needed).
pub fn storage_key(e: &web_sys::Event) -> Option<String> {
    js_sys::Reflect::get(e, &"key".into()).ok().and_then(|k| k.as_string())
}

/// Another tab changed the simulation: follow it.
fn listen() {
    if LISTENING.with(|l| l.replace(true)) {
        return;
    }
    if let Some(w) = web_sys::window() {
        gloo::events::EventListener::new(&w, "storage", |e| {
            if storage_key(e).as_deref() == Some(PAPER_KEY) {
                PAPER.with(|s| s.set(read()));
            }
        })
        .forget();
    }
}

/// The saved simulation; the component re-renders when it changes.
#[hook]
pub fn use_paper() -> Rc<SavedPaper> {
    listen();
    use_store(&PAPER)
}

pub fn get_paper() -> Option<PaperState> {
    PAPER.with(|s| s.get().state.clone())
}

/// Saves a new state (the result of an engine function applied to `get_paper()`).
pub fn set_paper(state: PaperState) {
    let had_error = PAPER.with(|s| s.get().error.is_some());
    if had_error {
        if let Some(bad) = local_get(PAPER_KEY) {
            local_set(PAPER_INVALID_KEY, &bad);
        }
    }
    local_set(PAPER_KEY, &paper_json(&state));
    PAPER.with(|s| s.set(SavedPaper { state: Some(state), error: None }));
}

/// Starts (or restarts) the simulation: every position and trade is erased. Capital in dollars.
pub fn reset_paper(capital: f64) {
    set_paper(new_paper(capital, now()));
}
