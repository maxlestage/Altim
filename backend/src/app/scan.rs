//! Routes of the opportunities scan (`/api/opportunities`) and of the anomalies of one asset (`/api/anomalies`).
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

use super::data::snap;
use super::error::ApiResult;
use super::json_of;
use super::validate::{parse_kind, parse_symbol};
use crate::cache::cached;
use crate::derivatives::{Derivatives, derivatives};
use crate::engine::anomalies::{Anomaly, candle_anomalies, sort};
use crate::http::{Error, Result};
use crate::js::now_ms;
use crate::types::{Interval, Kind};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnomalyReport {
    pub symbol: String,
    pub kind: Kind,
    pub as_of: i64,
    /// Close and time of the last closed daily session the measures read.
    pub price: Option<f64>,
    pub session: Option<i64>,
    /// Triggered measures, most severe first.
    pub anomalies: Vec<Anomaly>,
    /// Measures below their threshold (shown as "normal").
    pub normal: Vec<Anomaly>,
    /// Cryptos only: OKX derivatives and big moves.
    pub derivatives: Option<Derivatives>,
    pub errors: Vec<String>,
    pub source: String,
}

/// Anomalies of one asset (cached 2 minutes).
pub async fn anomalies_for(symbol: &str, kind: Kind) -> Result<Arc<AnomalyReport>> {
    let s = symbol.to_string();
    cached(&format!("anomalies:{}:{symbol}", kind.as_str()), 120_000, move || async move {
        let (d, der) = tokio::join!(snap(&s, kind, Interval::D1), async { if kind == Kind::Crypto { Some(derivatives(&s).await) } else { None } });
        let mut errors = Vec::new();
        let (mut all, price, session, source) = match &d {
            Ok(d) => {
                let src = format!("Bougies journalières {} (séances closes)", d.source);
                (candle_anomalies(&d.candles, &src), d.candles.last().map(|c| c.close), d.candles.last().map(|c| c.time), src)
            }
            Err(e) => {
                errors.push(format!("Bougies journalières indisponibles : {e}"));
                (vec![], None, None, String::new())
            }
        };
        let derivatives = der.map(|(block, found)| {
            all.extend(found);
            block
        });
        if d.is_err() && derivatives.is_none() {
            return Err::<AnomalyReport, Error>(Error(errors.join(" ; ")));
        }
        sort(&mut all);
        let (anomalies, normal): (Vec<Anomaly>, Vec<Anomaly>) = all.into_iter().partition(|a| a.triggered);
        Ok(AnomalyReport { symbol: s.clone(), kind, as_of: now_ms(), price, session, anomalies, normal, derivatives, errors, source })
    })
    .await
}

type Q = Query<HashMap<String, String>>;

/// `GET /api/anomalies?symbol=&kind=`.
pub async fn anomalies_route(Query(p): Q) -> ApiResult<Response> {
    let kind = parse_kind(p.get("kind").map(String::as_str))?;
    let symbol = parse_symbol(p.get("symbol").map(String::as_str), kind)?;
    Ok(json_of(&*anomalies_for(&symbol, kind).await?))
}

/// `GET /api/opportunities?kind=stock|crypto`. The first scan reads the whole universe (≈ 30 s): after 20 s the
/// client is told to come back (202, like `/api/selection`), the scan keeps going into the cache.
pub async fn opportunities_route(Query(p): Q) -> ApiResult<Response> {
    let kind = parse_kind(Some(p.get("kind").map(String::as_str).unwrap_or("stock")))?;
    match tokio::time::timeout(Duration::from_secs(20), crate::opportunities::opportunities(kind)).await {
        Ok(r) => Ok(json_of(&*r?)),
        Err(_) => Ok((StatusCode::ACCEPTED, json_of(&json!({ "pending": true }))).into_response()),
    }
}
