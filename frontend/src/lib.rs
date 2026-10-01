//! Altim web front in Rust + Yew (WebAssembly): the presentation site (/, legal pages) and the web app (/app/*).
//! Same URLs, same localStorage keys and formats, same CSS classes as the former React front.
//!
//! Layout (see README.md): `site/` the presentation site, `app/` the web app (one module per screen), `state/` the
//! stores saved in the browser, `api` the server client, `money` the display currency, `ui` the shared primitives,
//! `live` the live price stream, `hooks` small browser hooks, `route` the URLs. Pure logic lives in altim-core.
//! Each part of the front (the site, each group of app screens) is its own .wasm: `part`, and the entry points of
//! `frontend/bundles/`.
pub mod api;
pub mod app;
pub mod hooks;
pub mod live;
pub mod money;
pub mod part;
pub mod route;
pub mod site;
pub mod state;
pub mod ui;
