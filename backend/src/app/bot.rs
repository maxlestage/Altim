//! `/api/bot`: the « Bot Altim » of `engine::bot` trained and tested (nested walk-forward) on the validation's basket
//! plus the extra training universe, with the long daily histories of `bot_history` (a few at a time). Heavy:
//! computed in the background, kept 12 h fresh and served up to 24 h old while the next one is computed; 202 `{ "pending": true }` until the first one is ready (like
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
use crate::bot_history::long_history;
use crate::cache::{cached, peek};
use crate::engine::bot::{BotGroup, BotReport, EXTRA, History, MAX_THREADS, Timing, bot_view, run};
use crate::engine::validation::{BASKET, BasketAsset, Failure};
use crate::http::{Error, Result};
use crate::js::now_ms;
use crate::quotes::{ASSETS, make_asset};
use crate::types::{Candle, Kind};

pub const CACHE_KEY: &str = "bot:v2";
/// Fresh for 12 h; served while recomputing up to 24 h.
pub const FRESH_MS: i64 = 12 * 3_600_000;
pub const KEEP_MS: i64 = 24 * 3_600_000;
/// Histories fetched at the same time (Yahoo and Bitstamp; a crypto asks both).
const CONCURRENCY: usize = 4;
/// Under the Heroku router's 30 s.
const WAIT: Duration = Duration::from_secs(20);

/// Asset `i` of the training universe: the validation's basket, then the extra list (`true`).
fn entry(i: usize) -> (&'static BasketAsset, bool) {
    if i < BASKET.len() { (&BASKET[i], false) } else { (&EXTRA[i - BASKET.len()], true) }
}

/// One asset's long history: `bot_history` (up to 20 years / since listing); for the basket, Altim's usual long
/// history when that fails (said in the source). An extra asset that fails is left out with its reason.
async fn fetch(a: &'static BasketAsset, extra: bool, now: i64) -> std::result::Result<(Vec<Candle>, String), String> {
    match long_history(a.symbol, a.kind, now).await {
        Ok(h) => Ok(h),
        Err(e) if extra => Err(format!("historique long indisponible ({})", e.0)),
        Err(e) => match long(a.symbol, a.kind).await {
            Ok(h) => Ok((h.candles.clone(), format!("{} (repli : historique long indisponible, {})", h.source, e.0))),
            Err(e2) => Err(format!("historique indisponible ({} ; {})", e.0, e2.0)),
        },
    }
}

/// Fetches every history (bounded concurrency), trains and tests off the async threads. An asset that fails is
/// listed with its reason; an error only when no basket asset could be fetched.
pub async fn compute() -> Result<BotReport> {
    let started = now_ms();
    let fetched: Vec<_> = futures::stream::iter(0..BASKET.len() + EXTRA.len())
        .map(|i| async move {
            let (a, extra) = entry(i);
            (i, fetch(a, extra, started).await)
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    let mut histories = Vec::new();
    let mut failures = Vec::new();
    for (i, h) in fetched {
        let (a, extra) = entry(i);
        match h {
            Ok((candles, source)) => histories.push(History { asset: a, candles, source, extra }),
            Err(error) => failures.push(Failure { symbol: a.symbol.into(), name: a.name.into(), kind: a.kind, class: a.class, error }),
        }
    }
    if !histories.iter().any(|h| !h.extra) {
        return Err(Error(format!("aucun actif du panier n'a pu être chargé ({} échecs)", failures.len())));
    }
    let fetched_at = now_ms();
    // `ALTIM_BOT_THREADS` caps the threads (1: one core; same result either way).
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let threads = std::env::var("ALTIM_BOT_THREADS").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(cores).clamp(1, MAX_THREADS);
    let mut report = tokio::task::spawn_blocking(move || {
        run(
            histories,
            failures,
            fetched_at,
            "Bougies journalières longues : Yahoo Finance (actions, jusqu'à 20 ans ; cryptos) et Bitstamp (cryptos, depuis la cotation) ; source retenue indiquée par actif",
            threads,
        )
    })
    .await
    .map_err(|e| Error(format!("calcul interrompu : {e}")))?;
    report.timing = Some(Timing { fetch_ms: fetched_at - started, compute_ms: now_ms() - fetched_at, threads });
    Ok(report)
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
    let market = |k: Kind| async move { long(BotGroup::of(k).market(), k).await.map(|h| h.candles.clone()).unwrap_or_default() };
    let (stock_market, crypto_market) = tokio::join!(market(Kind::Stock), market(Kind::Crypto));
    let (stock_market, crypto_market) = (&stock_market, &crypto_market);
    let views = join_all(list.iter().take(20).map(|a| {
        let report = report.clone();
        async move {
            let candles = long(&a.symbol, a.kind).await.map(|h| h.candles.clone()).unwrap_or_default();
            let m = if a.kind == Kind::Crypto { crypto_market } else { stock_market };
            let mut v = crate::js::to_value(&bot_view(report.as_deref(), &a.symbol, a.kind, &candles, m, now));
            v["symbol"] = json!(a.symbol);
            v["kind"] = json!(a.kind);
            v
        }
    }))
    .await;
    Ok(json_of(&json!({ "asOf": report.as_ref().map(|r| r.as_of), "views": views })))
}
