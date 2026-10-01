//! Alerts kept in this browser ("altim.alerts.v1", alerts-store.ts): format and rules in
//! `altim_core::web::insights::alerts`; this is the store the screens and the checks share.
use std::rc::Rc;

use altim_core::types::Kind;
use altim_core::web::insights::alerts::{ALERTS_KEY, AlertsState, BuyAlert};
use altim_core::web::insights::news::{NewsReport, Quote};
use yew::prelude::*;

use crate::api::ApiError;
use crate::state::{Store, local_get, local_set, use_store};

thread_local! {
    pub static ALERTS: Store<AlertsState> = Store::new(AlertsState::parse(local_get(ALERTS_KEY).as_deref()));
}

/// Current state outside components (the checks).
pub fn alerts_state() -> Rc<AlertsState> {
    ALERTS.with(|s| s.get())
}

/// `setAlerts`: applies the change, saves, re-renders the subscribers.
pub fn set_alerts(update: impl FnOnce(&mut AlertsState)) {
    let mut s = (*alerts_state()).clone();
    update(&mut s);
    local_set(ALERTS_KEY, &s.to_json().to_string());
    ALERTS.with(|a| a.set(s));
}

#[hook]
pub fn use_alerts() -> Rc<AlertsState> {
    use_store(&ALERTS)
}

/// The radar's assets and the holdings, once each (`uniq([...watchlist, ...getHoldingAssets()])`).
pub fn my_assets() -> Vec<(String, Kind)> {
    let app = crate::state::app::app_state();
    let held = crate::state::holdings::holdings_state().assets();
    let all: Vec<(String, Kind)> = app.watchlist.iter().chain(held.iter()).map(|w| (w.symbol.clone(), w.kind)).collect();
    altim_core::web::insights::alerts::uniq(&all, |(s, k)| altim_core::web::store::asset_key(s, *k))
}

/// Consensus quotes (`/api/tickers?symbols=`), 20 assets per request.
pub async fn quotes(items: &[(String, Kind)]) -> Result<Vec<Quote>, ApiError> {
    crate::api::batched(items, |list| format!("/api/tickers?symbols={list}")).await
}

/// "Can I buy now?" (`/api/alerts`), 20 assets per request.
pub async fn buy_alerts(items: &[(String, Kind)]) -> Result<Vec<BuyAlert>, ApiError> {
    let cur = crate::money::cur_param();
    crate::api::batched(items, |list| format!("/api/alerts?symbols={list}{cur}")).await
}

/// The news of these assets (`/api/news`, 20 assets at most).
pub async fn news(items: &[(String, Kind)]) -> Result<NewsReport, ApiError> {
    let items = &items[..items.len().min(20)];
    let q = if items.is_empty() { String::new() } else { format!("?symbols={}", crate::api::list(items)) };
    crate::api::get(&format!("/api/news{q}")).await
}
