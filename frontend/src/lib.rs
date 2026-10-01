//! Altim web front in Rust + Yew (WebAssembly): the presentation site (/, legal pages) and the web app (/app/*).
//! Same URLs, same localStorage keys and formats, same CSS classes as the former React front.
//!
//! Layout (see README.md): `site/` the presentation site, `app/` the web app (one module per screen), `state/` the
//! stores saved in the browser, `api` the server client, `money` the display currency, `ui` the shared primitives,
//! `live` the live price stream, `hooks` small browser hooks, `route` the URLs. Pure logic lives in altim-core.
//! Features `site` and `app` (both by default): the release build makes one .wasm with each.
#![cfg_attr(not(all(feature = "site", feature = "app")), allow(dead_code, unused_imports))]
pub mod api;
#[cfg(feature = "app")]
pub mod app;
pub mod hooks;
pub mod live;
pub mod money;
pub mod route;
#[cfg(feature = "site")]
pub mod site;
pub mod state;
pub mod ui;

use wasm_bindgen::prelude::*;

/// Entry point (called by the wasm-bindgen loader in index.html).
#[wasm_bindgen(start)]
pub fn start() {
    route::normalize_path();
    money::install_engine_rate();
    state::fx::load_saved();
    let root = gloo::utils::document().get_element_by_id("root").expect("#root");
    yew::Renderer::<route::Root>::with_root(root).render();
}
