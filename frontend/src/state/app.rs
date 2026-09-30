//! Web app settings ("altim.webapp.v1", store.ts): disclaimer, interval, watchlist, risk, horizon, score weights,
//! display currency. Format and defaults in `altim_core::web::store::AppState`.
use std::rc::Rc;

use altim_core::web::store::{AppState, STATE_KEY};
use yew::prelude::*;

use super::{Store, local_get, local_set, use_store};

thread_local! {
    pub static APP: Store<AppState> = Store::new(AppState::parse(local_get(STATE_KEY).as_deref()));
}

/// Current state outside components (the alert checks).
pub fn app_state() -> Rc<AppState> {
    APP.with(|s| s.get())
}

/// `setState`: applies the change, saves, re-renders the subscribers (and the money display on a currency change).
pub fn set_app_state(update: impl FnOnce(&mut AppState)) {
    let mut s = (*app_state()).clone();
    update(&mut s);
    local_set(STATE_KEY, &s.to_json().to_string());
    APP.with(|a| a.set(s));
    crate::money::refresh();
}

#[hook]
pub fn use_app_state() -> Rc<AppState> {
    use_store(&APP)
}
