//! Routes read by the Radar and the asset screen (api.ts, WhyCard.tsx): same URLs, same parameters (`&cur=USD` where
//! the server writes amounts in its texts). Response types in `altim_core::web::decision::reports` and the server's
//! own `engine` types.
use altim_core::engine::strategies::StrategiesReport;
use altim_core::types::{Interval, Kind};
use altim_core::web::decision::doc::{DecisionDoc, PersonalInput, decision_url, parse_decision};
use altim_core::web::decision::reports::*;
use altim_core::web::store::ScoreWeights;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::api::{ApiError, batched, enc, get, get_value};
use crate::money::cur_param;

pub async fn radar(items: &[(String, Kind)], interval: Interval) -> Result<Vec<RadarRow>, ApiError> {
    batched(items, |list| format!("/api/radar?symbols={list}&interval={}", interval.as_str())).await
}

pub async fn candles(symbol: &str, kind: Kind, interval: Interval) -> Result<Snapshot, ApiError> {
    get(&format!("/api/candles?symbol={}&kind={}&interval={}", enc(symbol), kind.as_str(), interval.as_str())).await
}

pub async fn quotes(items: &[(String, Kind)]) -> Result<Vec<Quote>, ApiError> {
    batched(items, |list| format!("/api/tickers?symbols={list}")).await
}

pub async fn guard(symbol: &str, kind: Kind) -> Result<GuardReport, ApiError> {
    get(&format!("/api/guard?symbol={}&kind={}", enc(symbol), kind.as_str())).await
}

pub async fn zones(symbol: &str, kind: Kind) -> Result<ZonesReport, ApiError> {
    get(&format!("/api/zones?symbol={}&kind={}{}", enc(symbol), kind.as_str(), cur_param())).await
}

pub async fn macro_info() -> Result<MacroInfo, ApiError> {
    get("/api/macro").await
}

pub async fn alerts(items: &[(String, Kind)]) -> Result<Vec<BuyAlertRow>, ApiError> {
    let cur = cur_param();
    batched(items, |list| format!("/api/alerts?symbols={list}{cur}")).await
}

pub async fn sentiment(symbol: &str, kind: Kind) -> Result<Sentiment, ApiError> {
    get(&format!("/api/sentiment?symbol={}&kind={}", enc(symbol), kind.as_str())).await
}

pub async fn strategies(symbol: &str, kind: Kind) -> Result<StrategiesReport, ApiError> {
    get(&altim_core::web::decision::strategies::strategies_url(symbol, kind)).await
}

pub async fn anomalies(symbol: &str, kind: Kind) -> Result<AnomalyReport, ApiError> {
    get(&format!("/api/anomalies?symbol={}&kind={}{}", enc(symbol), kind.as_str(), cur_param())).await
}

pub async fn why(symbol: &str, kind: Kind) -> Result<WhyReport, ApiError> {
    get(&why_url(symbol, kind)).await
}

/// Decision for one asset; `personal` (average cost, portfolio weights) only for a held asset, never stored by the
/// server. A wrong answer is an error naming the field.
pub async fn decision(symbol: &str, kind: Kind, personal: Option<&PersonalInput>, weights: Option<&ScoreWeights>) -> Result<DecisionDoc, ApiError> {
    let url = format!("{}{}", decision_url(symbol, kind, personal, weights), cur_param());
    let (_, body) = get_value(&url).await?;
    parse_decision(body).map_err(ApiError)
}

/// `POST /api/ask`: a question on the observations of « Pourquoi ça bouge ? ».
pub async fn ask(symbol: &str, kind: Kind, question: &str) -> Result<AskAnswer, ApiError> {
    let w = web_sys::window().ok_or_else(|| ApiError("fenêtre indisponible".into()))?;
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(&JsValue::from_str(&ask_body(symbol, kind, question)));
    let headers = web_sys::Headers::new().map_err(|_| ApiError("Requête impossible".into()))?;
    let _ = headers.set("Content-Type", "application/json");
    init.set_headers(&headers);
    let req = web_sys::Request::new_with_str_and_init("/api/ask", &init).map_err(|_| ApiError("Requête impossible".into()))?;
    let res = JsFuture::from(w.fetch_with_request(&req)).await.map_err(|_| ApiError("Serveur injoignable".into()))?;
    let res: web_sys::Response = res.dyn_into().map_err(|_| ApiError("Réponse illisible".into()))?;
    let status = res.status();
    if status == 401 {
        let l = w.location();
        let next = format!("{}{}", l.pathname().unwrap_or_default(), l.search().unwrap_or_default());
        let _ = l.assign(&format!("/login?next={}", enc(&next)));
        return Err(ApiError("Session expirée".into()));
    }
    let text = match res.text() {
        Ok(p) => JsFuture::from(p).await.ok().and_then(|t| t.as_string()).unwrap_or_default(),
        Err(_) => String::new(),
    };
    let body: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({}));
    if !(200..300).contains(&status) {
        return Err(ApiError(body.get("error").and_then(|e| e.as_str()).map(String::from).unwrap_or_else(|| format!("Erreur {status}"))));
    }
    serde_json::from_value(body).map_err(|e| ApiError(format!("Réponse inattendue du serveur ({e})")))
}

/// `document.visibilityState === "visible"`: the periodic refreshes skip a hidden tab.
pub fn visible() -> bool {
    gloo::utils::document().visibility_state() == web_sys::VisibilityState::Visible
}

/// `navigator.onLine === false`.
pub fn offline() -> bool {
    let nav = js_sys::Reflect::get(&js_sys::global(), &"navigator".into()).ok();
    nav.and_then(|n| js_sys::Reflect::get(&n, &"onLine".into()).ok()).and_then(|v| v.as_bool()) == Some(false)
}
