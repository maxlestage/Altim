//! Server routes of Mes avoirs and its tools (the `api.radar`, `candles`, `quotes`, `search`, `universe`, `history`
//! and `sectors` of api.ts). Reply types in `altim_core::web::portfolio::{wire, sectors}`.
use altim_core::types::Kind;
use altim_core::web::market::Tick;
use altim_core::web::portfolio::sectors::SectorsReport;
use altim_core::web::portfolio::wire::{CandlesSnapshot, HistoryReply, RadarRow, UniverseItem, UniversePage};

use crate::api::{ApiError, batched, enc, get, list};

/// Signals and prices of the assets on one timeframe ("1d", "4h"), 20 per request.
pub async fn radar(items: &[(String, Kind)], interval: &str) -> Result<Vec<RadarRow>, ApiError> {
    batched(items, |l| format!("/api/radar?symbols={l}&interval={interval}")).await
}

/// Consensus candles of one asset.
pub async fn candles(symbol: &str, kind: Kind, interval: &str) -> Result<CandlesSnapshot, ApiError> {
    get(&format!("/api/candles?symbol={}&kind={}&interval={interval}", enc(symbol), kind.as_str())).await
}

/// Consensus prices (`/api/tickers`), 20 per request.
pub async fn quotes(items: &[(String, Kind)]) -> Result<Vec<Tick>, ApiError> {
    batched(items, |l| format!("/api/tickers?symbols={l}")).await
}

/// Cryptos and stocks together, by symbol or name.
pub async fn search(q: &str, limit: usize) -> Result<Vec<UniverseItem>, ApiError> {
    get(&format!("/api/search?q={}&limit={limit}", enc(q))).await
}

/// One page of the full catalogue of a kind.
pub async fn universe(kind: Kind, q: &str, offset: usize, limit: usize) -> Result<UniversePage, ApiError> {
    get(&format!("/api/universe?kind={}&q={}&offset={offset}&limit={limit}", kind.as_str(), enc(q))).await
}

/// Daily closes of up to 20 assets (and of the benchmarks) over 30, 90, 365 or 730 days.
pub async fn history(items: &[(String, Kind)], days: u32) -> Result<HistoryReply, ApiError> {
    let symbols = if items.is_empty() { String::new() } else { format!("&symbols={}", list(&items[..items.len().min(20)])) };
    get(&format!("/api/history?days={days}{symbols}")).await
}

/// Sector of each stock (Nasdaq screener, SEC SIC code, ETF flag); 50 at most.
pub async fn sectors(symbols: &[String]) -> Result<SectorsReport, ApiError> {
    get(&format!("/api/sectors?symbols={}", enc(&symbols[..symbols.len().min(50)].join(",")))).await
}
