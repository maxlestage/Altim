//! `/api/validation`: the signal's backtest on the fixed basket of `engine::validation` (≈ 34 long daily histories,
//! a few at a time), pooled by class, regime and overall. Heavy: computed in the background and kept 12 h fresh; a
//! report up to 24 h old is served while the next one is computed. Until the first one is ready the client gets 202
//! `{ "pending": true }` (like `/api/selection`) and comes back.
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use serde::Serialize;
use serde_json::json;

use super::data::long;
use super::error::ApiResult;
use super::json_of;
use crate::cache::{cached, peek};
use crate::engine::validation::{AssetResult, BASKET, Failure, ValidationReport, aggregate, run_asset};
use crate::http::{Error, Result};
use crate::js::now_ms;

pub const CACHE_KEY: &str = "validation:v1";
/// Fresh for 12 h; served while recomputing up to 24 h.
pub const FRESH_MS: i64 = 12 * 3_600_000;
pub const KEEP_MS: i64 = 24 * 3_600_000;
/// Histories fetched at the same time (each one queries several sources).
const CONCURRENCY: usize = 4;
/// Under the Heroku router's 30 s.
const WAIT: Duration = Duration::from_secs(20);

/// Fetches every history of the basket (bounded concurrency), runs the backtests off the async threads, pools them.
/// An asset that fails is listed with its reason; an error only when none could be tested.
pub async fn compute() -> Result<ValidationReport> {
    let fetched: Vec<_> = futures::stream::iter(0..BASKET.len())
        .map(|i| async move { (i, long(BASKET[i].symbol, BASKET[i].kind).await) })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    let (assets, failures) = tokio::task::spawn_blocking(move || {
        let mut assets: Vec<AssetResult> = Vec::new();
        let mut failures: Vec<Failure> = Vec::new();
        for (i, h) in fetched {
            let a = &BASKET[i];
            let fail = |error: String| Failure { symbol: a.symbol.into(), name: a.name.into(), kind: a.kind, class: a.class, error };
            match h {
                Ok(h) => match run_asset(a, &h.candles, &h.source) {
                    Some(r) => assets.push(r),
                    None => failures.push(fail(format!("historique journalier trop court ({} bougies)", h.candles.len()))),
                },
                Err(e) => failures.push(fail(format!("historique indisponible ({})", e.0))),
            }
        }
        (assets, failures)
    })
    .await
    .map_err(|e| Error(format!("calcul interrompu : {e}")))?;
    if assets.is_empty() {
        return Err(Error(format!("aucun actif du panier n'a pu être testé ({} échecs)", failures.len())));
    }
    Ok(aggregate(assets, failures, now_ms(), "Bougies journalières multi-sources (historique long d'Altim ; source retenue indiquée par actif)"))
}

/// The value when `fut` finishes within `wait`, else 202 `{ "pending": true }` (the work goes on in its own task).
pub async fn ready_or_pending<T: Serialize>(fut: impl Future<Output = Result<Arc<T>>>, wait: Duration) -> ApiResult<Response> {
    match tokio::time::timeout(wait, fut).await {
        Ok(r) => Ok(json_of(&*r?)),
        Err(_) => Ok((StatusCode::ACCEPTED, json_of(&json!({ "pending": true }))).into_response()),
    }
}

/// `GET /api/validation`.
pub async fn validation_route() -> ApiResult<Response> {
    // Starts (or joins) the computation when the report is older than 12 h; it keeps going if the client leaves.
    let task = tokio::spawn(cached(CACHE_KEY, FRESH_MS, compute));
    if let Some(r) = peek::<ValidationReport>(CACHE_KEY, KEEP_MS) {
        return Ok(json_of(&*r));
    }
    ready_or_pending(async move { task.await.unwrap_or_else(|e| Err(Error(format!("tâche interrompue : {e}")))) }, WAIT).await
}
