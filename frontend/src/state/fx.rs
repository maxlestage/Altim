//! EUR/USD rate of the app (/api/fx, fx.ts): refreshed every 10 minutes, the last valid body kept in "altim.fx.v1"
//! for 7 days so a server hiccup does not switch the amounts back to dollars. Never a made-up rate.
use std::cell::Cell;
use std::rc::Rc;

use altim_core::web::fx::{FX_KEY, FX_REFRESH_MS, fx_error, parse_fx, saved_fx};
use altim_core::web::money::FxRate;
use yew::prelude::*;

use super::{Store, local_get, local_set, now, use_store};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FxState {
    pub fx: Option<FxRate>,
    pub error: Option<String>,
    pub loading: bool,
}

thread_local! {
    pub static FX: Store<FxState> = Store::new(FxState::default());
    static STARTED: Cell<bool> = const { Cell::new(false) };
}

pub fn fx_state() -> Rc<FxState> {
    FX.with(|s| s.get())
}

fn emit(s: FxState) {
    FX.with(|f| f.set(s));
    crate::money::refresh();
}

/// The saved rate (≤ 7 days), read once at start-up before the first render.
pub fn load_saved() {
    let fx = saved_fx(local_get(FX_KEY).as_deref(), now());
    emit(FxState { fx, ..Default::default() });
}

pub async fn refresh_fx() {
    let s = (*fx_state()).clone();
    if s.loading {
        return;
    }
    emit(FxState { loading: true, ..s.clone() });
    match crate::api::get::<serde_json::Value>("/api/fx").await {
        Ok(body) => match parse_fx(&body, now()) {
            Some(fx) => {
                local_set(FX_KEY, &body.to_string());
                emit(FxState { fx: Some(fx), error: None, loading: false });
            }
            None => emit(FxState { error: Some(fx_error(&body)), loading: false, ..s }),
        },
        Err(e) => emit(FxState { error: Some(format!("taux indisponible ({e})")), loading: false, ..s }),
    }
}

/// Starts the refresh loop once (first read now, then every 10 minutes).
pub fn start_fx() {
    if STARTED.with(|s| s.replace(true)) {
        return;
    }
    wasm_bindgen_futures::spawn_local(refresh_fx());
    gloo::timers::callback::Interval::new(FX_REFRESH_MS, || wasm_bindgen_futures::spawn_local(refresh_fx())).forget();
}

#[hook]
pub fn use_fx() -> Rc<FxState> {
    use_store(&FX)
}
