//! « Pourquoi ça bouge ? » routes: `GET /api/why?symbol=&kind=` gathers, from the server's cached sources only, what
//! was observed with today's move (engine::why), and the optional `POST /api/ask` sends a question with ONLY that data
//! (and the last decision computed for the asset, if any) to the Anthropic Messages API — available only when the
//! ANTHROPIC_API_KEY environment variable is set (never logged, never sent to the browser).
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde_json::{Value, json};

use super::data::{guard_for, macro_now, macro_regime, quotes, snap};
use super::error::{ApiError, ApiResult, bad, scrub};
use super::validate::{parse_kind, parse_symbol};
use crate::cache::cached;
use crate::engine::ask;
use crate::engine::decision_types::Decision;
use crate::engine::why::{BenchMove, DecisionInfo, EventInput, NewsInput, SourceState, WhyInput, WhyReport, change_since_close, why_report};
use crate::js::{now_ms, to_value};
use crate::quotes::make_asset;
use crate::types::{Interval, Kind};

/// A decision computed less than this long ago is "the last decision" of the asset.
const DECISION_MAX_AGE: i64 = 30 * 60_000;
static DECISIONS: LazyLock<Mutex<HashMap<String, (i64, Arc<Decision>)>>> = LazyLock::new(Mutex::default);

/// Keeps the last informational decision of an asset (never a personal one: it carries the user's cost / weights).
pub fn remember_decision(d: &Decision) {
    if d.mode != "informational" {
        return;
    }
    let mut m = DECISIONS.lock().unwrap();
    let now = now_ms();
    m.retain(|_, (at, _)| now - *at < DECISION_MAX_AGE);
    if m.len() < 2_000 {
        m.insert(format!("{}:{}", d.kind.as_str(), d.symbol), (now, Arc::new(d.clone())));
    }
}

fn last_decision(symbol: &str, kind: Kind) -> Option<Arc<Decision>> {
    let m = DECISIONS.lock().unwrap();
    m.get(&format!("{}:{symbol}", kind.as_str())).filter(|(at, _)| now_ms() - *at < DECISION_MAX_AGE).map(|(_, d)| d.clone())
}

/// What the model gets from the decision: the verdict and the families, no plan nor personal part.
fn decision_summary(d: &Decision) -> Value {
    let v = to_value(d);
    let pick = |k: &str| v.get(k).cloned().unwrap_or(Value::Null);
    json!({
        "asOf": pick("asOf"), "verdict": pick("verdict"), "label": pick("label"), "ratingLabel": pick("ratingLabel"),
        "headline": pick("headline"), "confidence": pick("confidence"), "degraded": pick("degraded"), "marketRegime": pick("marketRegime"),
        "families": v.get("families").and_then(Value::as_array).map(|f| f.iter().map(|x| json!({
            "key": x.get("key"), "label": x.get("label"), "score": x.get("score"), "status": x.get("status"), "summary": x.get("summary"),
        })).collect::<Vec<_>>()),
    })
}

/// The ANTHROPIC_API_KEY of the environment, when set and not empty.
pub fn ask_key(env: impl Fn(&str) -> Option<String>) -> Option<String> {
    env("ANTHROPIC_API_KEY").map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

pub fn ask_enabled() -> bool {
    ask_key(|k| std::env::var(k).ok()).is_some()
}

async fn within<T>(secs: u64, f: impl std::future::Future<Output = T>) -> Option<T> {
    tokio::time::timeout(Duration::from_secs(secs), f).await.ok()
}

fn paris_today(now: i64) -> String {
    chrono::DateTime::from_timestamp_millis(now)
        .map(|t| t.with_timezone(&chrono_tz::Europe::Paris).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// Gathers the inputs (each source optional, with its state) and builds the report. Cached 60 s per asset.
pub async fn why_for(symbol: &str, kind: Kind) -> crate::http::Result<Arc<WhyReport>> {
    let s = symbol.to_string();
    cached(&format!("why:{}:{symbol}", kind.as_str()), 60_000, || async move {
        let now = now_ms();
        let name = crate::universe::universe(kind)
            .await
            .ok()
            .and_then(|l| l.iter().find(|e| e.0 == s).map(|e| e.1.clone()))
            .unwrap_or_else(|| make_asset(&s, kind, None).name);
        let (bsym, bname) = match kind {
            Kind::Crypto => ("BTC", "Bitcoin"),
            Kind::Stock => ("SPY", "S&P 500 (SPY)"),
        };
        let is_bench = s == bsym;
        let asset = make_asset(&s, kind, None);
        let bench = make_asset(bsym, kind, None);
        let list = if is_bench { vec![asset] } else { vec![asset, bench] };
        let (daily, bdaily, q, g, m, events) = tokio::join!(
            within(10, snap(&s, kind, Interval::D1)),
            async { if is_bench { None } else { within(10, snap(bsym, kind, Interval::D1)).await } },
            within(8, quotes(&list)),
            within(10, guard_for(&s, kind)),
            within(8, macro_now()),
            within(4, crate::calendar::upcoming_for(&s, kind, 0)),
        );
        let daily = daily.and_then(|r| r.ok());
        let q = q.and_then(|r| r.ok());
        let g = g.and_then(|r| r.ok());
        let m = m.and_then(|r| r.ok());
        let events = events.flatten();
        let regime = match &m {
            Some(r) => macro_regime(r).await,
            None => None,
        };
        let quote = |sym: &str| q.as_ref().and_then(|q| q.iter().find(|x| x.symbol == sym).cloned());
        let aq = quote(&s);
        let benchmark = if is_bench {
            None
        } else {
            let bq = quote(bsym);
            let bd = bdaily.and_then(|r| r.ok());
            match (bq, bd) {
                (Some(bq), Some(bd)) => change_since_close(&bd.candles, Some(bq.price)).map(|c| BenchMove { name: bname.into(), pct: c.pct }),
                _ => None,
            }
        };
        let today = paris_today(now);
        let decision = last_decision(&s, kind);
        let inputs = g.as_ref().map(|g| &g.inputs);
        let input = WhyInput {
            symbol: s.clone(),
            name,
            kind: Some(kind),
            now,
            price: aq.as_ref().map(|x| x.price).or_else(|| g.as_ref().and_then(|g| g.price)),
            price_sources: aq.as_ref().map(|x| (x.agreeing, x.total)),
            daily: daily.as_ref().map(|d| d.candles.clone()).unwrap_or_default(),
            benchmark,
            benchmark_name: (!is_bench).then(|| bname.to_string()),
            guard_loaded: g.is_some(),
            funding_rate: inputs.and_then(|i| i.funding_rate),
            long_short: inputs.and_then(|i| i.long_short_ratio),
            fear_greed: inputs.and_then(|i| i.fear_greed),
            news: inputs.map(|i| NewsInput {
                count24h: i.news24h,
                negative: i.news_tone.negative,
                positive: i.news_tone.positive,
                headlines: i.headlines.iter().map(|h| h.title.clone()).collect(),
            }),
            macro_score: m.as_ref().map(|r| r.score),
            macro_level: m.as_ref().map(|r| r.level),
            regime,
            events_today: events.as_ref().map(|ev| {
                ev.iter()
                    .filter(|e| e.day == today)
                    .map(|e| EventInput {
                        title: e.title.clone(),
                        time: e.time.clone(),
                        high: matches!(e.importance, crate::calendar::Importance::High),
                        released: e.actual.is_some(),
                    })
                    .collect()
            }),
            decision: decision.as_ref().map(|d| DecisionInfo { as_of: d.as_of, label: d.label.clone(), families: d.families.clone() }),
        };
        let mut report = why_report(&input);
        let state = |name: &str, ok: bool, detail: &str| SourceState { name: name.into(), ok, detail: detail.into() };
        report.sources = vec![
            state("Bougies journalières", daily.is_some(), if daily.is_some() { "consensus multi-sources, bougies closes" } else { "indisponibles" }),
            state("Cours en direct", aq.is_some(), if aq.is_some() { "consensus multi-sources" } else { "indisponible" }),
            state("Garde de marché", g.is_some(), if g.is_some() { "financement, sentiment, actualités" } else { "indisponible" }),
            state("Contexte macro", m.is_some(), if m.is_some() { "stress et régime" } else { "indisponible" }),
            state("Calendrier", events.is_some(), if events.is_some() { "annonces du jour" } else { "non chargé ou source en échec" }),
            state("Décision", decision.is_some(), if decision.is_some() { "dernière calculée (30 min au plus)" } else { "aucune récente" }),
        ];
        Ok::<_, crate::http::Error>(report)
    })
    .await
}

fn json_response(status: StatusCode, v: &Value) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json; charset=utf-8")], serde_json::to_vec(v).unwrap_or_default()).into_response()
}

/// GET /api/why?symbol=&kind=
pub async fn why_route(axum::extract::Query(p): axum::extract::Query<HashMap<String, String>>) -> ApiResult<Response> {
    let kind = parse_kind(p.get("kind").map(String::as_str))?;
    let symbol = parse_symbol(p.get("symbol").map(String::as_str), kind)?;
    let report = why_for(&symbol, kind).await?;
    let mut v = to_value(&*report);
    v["askEnabled"] = Value::Bool(ask_enabled());
    Ok(json_response(StatusCode::OK, &v))
}

/// POST /api/ask {symbol, kind, question}: 503 without a configured key (checked first, nothing else is done).
pub async fn ask_route(headers: HeaderMap, body: Bytes) -> ApiResult<Response> {
    let Some(key) = ask_key(|k| std::env::var(k).ok()) else {
        return Ok(json_response(StatusCode::SERVICE_UNAVAILABLE, &json!({ "error": ask::NO_KEY })));
    };
    // JSON only: a cross-site form cannot send this content type without a preflight.
    if !headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("application/json")) {
        return bad("corps JSON attendu");
    }
    let Ok(req) = serde_json::from_slice::<Value>(&body) else { return bad("corps JSON invalide") };
    let kind = parse_kind(req.get("kind").and_then(Value::as_str))?;
    let symbol = parse_symbol(req.get("symbol").and_then(Value::as_str), kind)?;
    let question = match ask::clean_question(req.get("question").and_then(Value::as_str).unwrap_or("")) {
        Ok(q) => q,
        Err(e) => return bad(e),
    };
    let report = why_for(&symbol, kind).await?;
    let decision = last_decision(&symbol, kind).map(|d| decision_summary(&d));
    let data = ask::data_block(&to_value(&*report), decision.as_ref());
    let res = crate::http::CLIENT
        .post(ask::API_URL)
        .timeout(Duration::from_secs(25))
        .header("x-api-key", key)
        .header("anthropic-version", ask::API_VERSION)
        .header("anthropic-beta", ask::FALLBACK_BETA)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&ask::build_request(&question, &data))
        .send()
        .await
        .map_err(|e| ApiError::Upstream(format!("Service de questions injoignable ({})", crate::http::Error::from(e).0)))?;
    let status = res.status();
    let answer: Value =
        res.json().await.map_err(|_| ApiError::Upstream(format!("Service de questions : réponse illisible (HTTP {})", status.as_u16())))?;
    match ask::parse_answer(&answer) {
        Ok(text) => Ok(json_response(
            StatusCode::OK,
            &json!({ "answer": text, "model": answer.get("model").cloned().unwrap_or(json!(ask::MODEL)), "question": question, "data": data, "disclaimer": crate::engine::why::DISCLAIMER }),
        )),
        Err(e) => Err(ApiError::Upstream(scrub(&e))),
    }
}
