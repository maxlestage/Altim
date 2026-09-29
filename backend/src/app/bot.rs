//! `/api/bot`: the « Bot Altim » of `engine::bot` trained and tested walk-forward on the validation's basket (same
//! long daily histories, a few at a time). Heavy: computed in the background, kept 12 h fresh and served up to 24 h
//! old while the next one is computed; 202 `{ "pending": true }` until the first one is ready (like
//! `/api/validation`). `/api/bot/views?symbols=` gives today's view of the user's assets from the cached report only.
use std::collections::HashMap;
use std::time::Duration;

use axum::extract::Query;
use axum::response::Response;
use futures::StreamExt;
use futures::future::join_all;
use serde_json::json;

use super::data::long;
use super::error::ApiResult;
use super::json_of;
use super::validation::ready_or_pending;
use crate::cache::{cached, peek};
use crate::engine::bot::{BotReport, History, bot_view, run};
use crate::engine::validation::{BASKET, Failure};
use crate::http::{Error, Result};
use crate::js::now_ms;
use crate::quotes::{ASSETS, make_asset};

pub const CACHE_KEY: &str = "bot:v1";
/// Fresh for 12 h; served while recomputing up to 24 h.
pub const FRESH_MS: i64 = 12 * 3_600_000;
pub const KEEP_MS: i64 = 24 * 3_600_000;
/// Histories fetched at the same time (each one queries several sources; shared with the validation's cache).
const CONCURRENCY: usize = 4;
/// Under the Heroku router's 30 s.
const WAIT: Duration = Duration::from_secs(20);

/// Fetches every history of the basket (bounded concurrency), trains and tests off the async threads. An asset that
/// fails is listed with its reason; an error only when none could be fetched.
pub async fn compute() -> Result<BotReport> {
    let fetched: Vec<_> = futures::stream::iter(0..BASKET.len())
        .map(|i| async move { (i, long(BASKET[i].symbol, BASKET[i].kind).await) })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    let mut histories = Vec::new();
    let mut failures = Vec::new();
    for (i, h) in fetched {
        let a = &BASKET[i];
        match h {
            Ok(h) => histories.push(History { asset: a, candles: h.candles.clone(), source: h.source.clone() }),
            Err(e) => failures.push(Failure {
                symbol: a.symbol.into(),
                name: a.name.into(),
                kind: a.kind,
                class: a.class,
                error: format!("historique indisponible ({})", e.0),
            }),
        }
    }
    if histories.is_empty() {
        return Err(Error(format!("aucun actif du panier n'a pu être chargé ({} échecs)", failures.len())));
    }
    tokio::task::spawn_blocking(move || {
        run(
            histories,
            failures,
            now_ms(),
            "Bougies journalières multi-sources (historique long d'Altim, le même que la validation ; source retenue indiquée par actif)",
        )
    })
    .await
    .map_err(|e| Error(format!("calcul interrompu : {e}")))
}

/// The cached report, if any (a decision never starts the computation).
pub fn cached_report() -> Option<std::sync::Arc<BotReport>> {
    peek::<BotReport>(CACHE_KEY, KEEP_MS)
}

/// `GET /api/bot`.
pub async fn bot_route() -> ApiResult<Response> {
    // Starts (or joins) the computation when the report is older than 12 h; it keeps going if the client leaves.
    let task = tokio::spawn(cached(CACHE_KEY, FRESH_MS, compute));
    if let Some(r) = cached_report() {
        return Ok(json_of(&*r));
    }
    ready_or_pending(async move { task.await.unwrap_or_else(|e| Err(Error(format!("tâche interrompue : {e}")))) }, WAIT).await
}

/// `GET /api/bot/views?symbols=BTC:crypto,AAPL:stock`: today's view of each asset (20 at most) from the cached
/// report; `available: false` in each view while none is cached (the computation is not started here).
pub async fn views_route(Query(p): Query<HashMap<String, String>>) -> ApiResult<Response> {
    let list = super::validate::parse_assets(p.get("symbols").map(String::as_str), &ASSETS, |s, k| make_asset(s, k, None))?;
    let report = cached_report();
    let now = now_ms();
    let views = join_all(list.iter().take(20).map(|a| {
        let report = report.clone();
        async move {
            let candles = long(&a.symbol, a.kind).await.map(|h| h.candles.clone()).unwrap_or_default();
            let mut v = crate::js::to_value(&bot_view(report.as_deref(), &a.symbol, a.kind, &candles, now));
            v["symbol"] = json!(a.symbol);
            v["kind"] = json!(a.kind);
            v
        }
    }))
    .await;
    Ok(json_of(&json!({ "asOf": report.as_ref().map(|r| r.as_of), "views": views })))
}
