//! Pure logic of the web front (`frontend/`, Yew), kept here so it is tested with a plain `cargo test` and reusable
//! by any client: money display (money.ts), the saved state formats (store.ts, fx.ts) and the JSON contracts and
//! helpers the screens read. Nothing here touches the browser: the frontend passes the clock, the rate and storage.
pub mod bot;
pub mod danger;
pub mod fx;
pub mod market;
pub mod money;
pub mod store;
// One folder per group of screens: Radar and asset screen, bot / validation / alerts / news, holdings, trading.
pub mod decision;
pub mod insights;
pub mod portfolio;
pub mod trading;
