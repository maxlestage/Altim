//! Altim's pure logic, shared by the server (`backend/`) and the browser (`frontend/`, wasm32): the engines ported
//! from `web/src/engine/*.ts`, JavaScript number/date semantics, the JSON contracts both sides read. No I/O, no
//! clock of its own except `js::now_ms` (host clock); time is passed in wherever a result depends on it.
pub mod bot_history;
pub mod calendar;
pub mod candles;
pub mod engine;
pub mod error;
pub mod fx;
pub mod js;
pub mod jstime;
pub mod types;
pub mod web;
