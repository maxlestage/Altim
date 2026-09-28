//! Altim web server (Axum): showcase site, /app web application and multi-source APIs (port of web/server/app.ts).
pub mod data;
pub mod error;
pub mod extras;
pub mod validate;
pub mod web;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use bytes::Bytes;
use futures::StreamExt;
use futures::future::join_all;
use serde::Serialize;
use serde_json::{Value, json};
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, Predicate, SizeAbove};

use crate::auth::{Auth, RateLimit};
use crate::engine::brief::{BriefBuy, MARKET_LABEL, MarketLevel, PriceNow, headline, movers};
use crate::engine::history::BENCHMARKS;
use crate::engine::macro_ctx::MacroLevel;
use crate::engine::screener::{HORIZON_LIST, to_horizon};
use crate::js::{now_ms, to_value};
use crate::live::LiveHub;
use crate::quotes::{ASSETS, make_asset};
use crate::types::{Asset, DAY_MS, Kind};
use crate::universe::{search_universe, universe};
use data::*;
use error::{ApiError, ApiResult, bad};
use validate::{int_or, js_number, parse_cost, parse_days, parse_interval, parse_kind, parse_score_weights, parse_symbol, parse_symbol_list, parse_weights};

type Q = Query<HashMap<String, String>>;

/// A few streams per address (phone, watch, browser tabs): a leaked token cannot open thousands.
const MAX_LIVE_STREAMS: usize = 8;

#[derive(Clone)]
pub struct AppState {
    pub live: LiveHub,
    streams: Arc<Mutex<HashMap<String, usize>>>,
}

impl AppState {
    pub fn new(live: LiveHub) -> Self {
        AppState { live, streams: Arc::default() }
    }
}

/// JSON exactly as `res.json` writes it (integral numbers without ".0").
fn json_of<T: Serialize + ?Sized>(t: &T) -> Response {
    let body = serde_json::to_vec(&to_value(t)).unwrap_or_default();
    ([(header::CONTENT_TYPE, "application/json; charset=utf-8")], body).into_response()
}

fn assets(q: &HashMap<String, String>, key: &str) -> ApiResult<Vec<Asset>> {
    validate::parse_assets(q.get(key).map(String::as_str), &ASSETS, |s, k| make_asset(s, k, None))
}

fn q<'a>(q: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    q.get(key).map(String::as_str)
}

async fn tickers(Query(p): Q) -> ApiResult<Response> {
    let data = quotes(&assets(&p, "symbols")?).await?;
    if data.is_empty() {
        return Err(ApiError::Upstream("aucune source disponible".into()));
    }
    Ok(json_of(&*data))
}

async fn candles(Query(p): Q) -> ApiResult<Response> {
    let kind = parse_kind(q(&p, "kind"))?;
    let symbol = parse_symbol(q(&p, "symbol").or(q(&p, "base")), kind)?;
    let interval = parse_interval(q(&p, "interval"))?;
    Ok(json_of(&*snap(&symbol, kind, interval).await?))
}

async fn radar(Query(p): Q) -> ApiResult<Response> {
    let list = assets(&p, "symbols")?;
    let interval = parse_interval(Some(q(&p, "interval").unwrap_or("4h")))?;
    let (items, qs) = tokio::join!(join_all(list.iter().map(|a| radar_item(a, interval))), quotes(&list));
    let qs = qs.ok();
    let out: Vec<Value> = items
        .into_iter()
        .map(|it| {
            let (symbol, kind, last) = match &it {
                RadarItem::Ok(o) => (o.symbol.clone(), o.kind, o.last_close),
                RadarItem::Err(e) => (e.symbol.clone(), e.kind, None),
            };
            let quote = qs.as_ref().and_then(|qs| qs.iter().find(|x| x.symbol == symbol && x.kind == kind).cloned());
            let mut v = to_value(&it);
            let o = v.as_object_mut().unwrap();
            o.insert("name".into(), json!(make_asset(&symbol, kind, None).name));
            o.insert("price".into(), to_value(&quote.as_ref().map(|q| q.price).or(last)));
            o.insert("change".into(), to_value(&quote.as_ref().and_then(|q| q.change)));
            o.insert("priceSources".into(), to_value(&quote.as_ref().map(|q| format!("{}/{}", q.agreeing, q.total))));
            v
        })
        .collect();
    Ok(json_of(&out))
}

/// `String(req.query.q ?? "").slice(0, 30)` (30 UTF-16 units).
fn query_text(p: &HashMap<String, String>) -> String {
    crate::jsval::slice_utf16(q(p, "q").unwrap_or(""), 30).to_string()
}

async fn search(Query(p): Q) -> ApiResult<Response> {
    let text = query_text(&p);
    let limit = int_or(q(&p, "limit"), 20.0).clamp(1.0, 50.0) as usize;
    if crate::engine::news::js_trim(&text).is_empty() {
        return Ok(json_of(&Vec::<Value>::new()));
    }
    let key = format!("search:{limit}:{}", text.to_uppercase());
    let items = crate::cache::cached(&key, 600_000, || async move { Ok::<_, crate::http::Error>(search_assets(&text, limit).await) }).await?;
    Ok(json_of(&*items))
}

/// Full catalogue, paginated: /api/universe?kind=crypto|stock&q=&offset=0&limit=50
async fn universe_route(Query(p): Q) -> ApiResult<Response> {
    let kind = parse_kind(q(&p, "kind"))?;
    let text = query_text(&p);
    let offset = int_or(q(&p, "offset"), 0.0).max(0.0) as usize;
    let limit = int_or(q(&p, "limit"), 50.0).clamp(1.0, 200.0) as usize;
    let list = universe(kind).await?;
    let matches: Vec<&crate::universe::UniverseEntry> =
        if crate::engine::news::js_trim(&text).is_empty() { list.iter().collect() } else { search_universe(&list, &text, list.len()) };
    let items: Vec<Value> = matches.iter().skip(offset).take(limit).map(|e| to_item(e, kind)).collect();
    Ok(json_of(&json!({ "total": matches.len(), "offset": offset, "items": items })))
}

/// Market guard (regime, shock risk, reversal risk, policy for bots).
async fn guard(Query(p): Q) -> ApiResult<Response> {
    let kind = parse_kind(q(&p, "kind"))?;
    let symbol = parse_symbol(q(&p, "symbol"), kind)?;
    Ok(json_of(&*guard_for(&symbol, kind).await?))
}

/// One open live stream of an address; released when the stream ends (client gone, server stopping).
struct StreamSlot {
    streams: Arc<Mutex<HashMap<String, usize>>>,
    who: String,
}

impl Drop for StreamSlot {
    fn drop(&mut self) {
        let mut m = self.streams.lock().unwrap();
        let n = m.get(&self.who).copied().unwrap_or(1).saturating_sub(1);
        if n == 0 {
            m.remove(&self.who);
        } else {
            m.insert(self.who.clone(), n);
        }
    }
}

/// Live prices (Server-Sent Events): last known price right away, then every change, several times per second.
async fn live(State(st): State<AppState>, Query(p): Q, req: Request) -> ApiResult<Response> {
    let list = assets(&p, "symbols")?;
    let who = crate::auth::express::client_ip(&req).unwrap_or_else(|| "?".into());
    {
        let mut m = st.streams.lock().unwrap();
        let open = m.get(&who).copied().unwrap_or(0);
        if open >= MAX_LIVE_STREAMS {
            let mut r = (StatusCode::TOO_MANY_REQUESTS, json_of(&json!({ "error": "Trop de flux en direct ouverts." }))).into_response();
            r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("10"));
            return Ok(r);
        }
        m.insert(who.clone(), open + 1);
    }
    let slot = StreamSlot { streams: st.streams.clone(), who };
    let keys: Vec<String> = list.iter().map(|a| format!("{}:{}", a.kind.as_str(), a.symbol)).collect();
    let sub = st.live.subscribe(&keys);
    let first = st.live.snapshot(&keys);
    let event = |t: &crate::live::Tick| Bytes::from(format!("data: {}\n\n", to_value(t)));
    let head = futures::stream::iter(std::iter::once(Bytes::from_static(b"retry: 3000\n\n")).chain(first.iter().map(event)).collect::<Vec<_>>());
    // Comment every 15 s: keeps the connection open through the Heroku router (55 s idle limit) and proxies.
    let beat = tokio_stream::wrappers::IntervalStream::new({
        let mut i = tokio::time::interval_at(tokio::time::Instant::now() + Duration::from_secs(15), Duration::from_secs(15));
        i.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        i
    })
    .map(|_| Bytes::from_static(b": ok\n\n"));
    let ticks = sub.map(move |t| event(&t));
    let body = head.chain(futures::stream::select(ticks, beat)).map(move |b| {
        let _keep = &slot;
        Ok::<_, Infallible>(b)
    });
    let mut r = Response::new(Body::from_stream(body));
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/event-stream; charset=utf-8"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache, no-transform"));
    h.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    h.insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(r)
}

/// Buy zones by horizon (Fibonacci, short / medium / long term) with the macro context.
async fn zones_route(Query(p): Q) -> ApiResult<Response> {
    let kind = parse_kind(q(&p, "kind"))?;
    Ok(json_of(&zones(&parse_symbol(q(&p, "symbol"), kind)?, kind).await?))
}

/// Which stocks or cryptos to buy: ranked, finalists checked.
async fn selection_route(Query(p): Q) -> ApiResult<Response> {
    let market = parse_kind(Some(q(&p, "kind").unwrap_or("stock")))?;
    let Some(h) = to_horizon(q(&p, "horizon").unwrap_or("1m"), market) else {
        let list: Vec<&str> = HORIZON_LIST.iter().map(|h| h.as_str()).collect();
        return bad(format!("horizon invalide ({})", list.join(" | ")));
    };
    // The first computation scans 150 stocks (≈ 30 s): the Heroku router cuts at 30 s, so after 20 s the client is
    // told to come back; the computation keeps going and lands in the cache.
    match tokio::time::timeout(Duration::from_secs(20), selection(h, market)).await {
        Ok(r) => Ok(json_of(&*r?)),
        Err(_) => Ok((StatusCode::ACCEPTED, json_of(&json!({ "pending": true }))).into_response()),
    }
}

/// "Can I buy now?" for the notifications of the apps (iPhone, Apple Watch, Android): same rule everywhere.
async fn alerts(Query(p): Q) -> ApiResult<Response> {
    Ok(json_of(&alerts_for(&assets(&p, "symbols")?).await))
}

/// Daily closes of the held assets over 30, 90, 365 or 730 days, plus Bitcoin and the S&P 500 to compare.
async fn history(Query(p): Q) -> ApiResult<Response> {
    let days = q(&p, "days").map(js_number).unwrap_or(90.0);
    if ![30.0, 90.0, 365.0, 730.0].contains(&days) {
        return bad("days invalide (30 | 90 | 365 | 730)");
    }
    let held = if q(&p, "symbols").is_some_and(|s| !s.is_empty()) { assets(&p, "symbols")? } else { vec![] };
    let mut unique: indexmap::IndexMap<String, Asset> = indexmap::IndexMap::new();
    let bench = BENCHMARKS.iter().map(|b| {
        let (kind, symbol) = b.id.split_once(':').unwrap();
        make_asset(symbol, Kind::parse(kind).unwrap(), None)
    });
    for a in held.into_iter().chain(bench) {
        unique.insert(format!("{}:{}", a.kind.as_str(), a.symbol), a);
    }
    let from = now_ms() - (days as i64 + 7) * DAY_MS;
    let series = join_all(unique.values().map(|a| async move {
        match long(&a.symbol, a.kind).await {
            Ok(h) => json!({ "symbol": a.symbol, "kind": a.kind,
                "closes": h.candles.iter().filter(|c| c.time >= from).map(|c| json!([c.time, c.close])).collect::<Vec<_>>() }),
            Err(_) => json!({ "symbol": a.symbol, "kind": a.kind, "closes": [], "error": "historique indisponible" }),
        }
    }))
    .await;
    Ok(json_of(&json!({ "asOf": now_ms(), "days": days, "series": series })))
}

/// News section: world, markets, crypto and the user's own assets, duplicates merged, most recent first.
async fn news(Query(p): Q) -> ApiResult<Response> {
    let list = if q(&p, "symbols").is_some_and(|s| !s.is_empty()) { assets(&p, "symbols")? } else { vec![] };
    Ok(json_of(&*news_report(&list).await?))
}

fn market_level(l: MacroLevel) -> MarketLevel {
    match l {
        MacroLevel::Calm => MarketLevel::Calm,
        MacroLevel::Tense => MarketLevel::Tense,
        MacroLevel::High => MarketLevel::High,
    }
}

/// "Point du jour": market climate, what can be bought now, moves since the last daily close, top stories. Each
/// part that fails is left empty: the rest still comes.
async fn brief(Query(p): Q) -> ApiResult<Response> {
    let list = assets(&p, "symbols")?;
    let key = format!("brief:{}", quotes_key(&list));
    let out = crate::cache::cached(&key, 120_000, || async move {
        let (m, alerts, prices, closes, news) = tokio::join!(
            macro_now(),
            alerts_for(&list),
            quotes(&list),
            join_all(list.iter().map(|a| async move {
                let k = format!("{}:{}", a.kind.as_str(), a.symbol);
                (k, snap(&a.symbol, a.kind, crate::types::Interval::D1).await.ok().and_then(|d| d.candles.last().map(|c| c.close)))
            })),
            news_report(&list),
        );
        let m = m.ok();
        let mut buyable: Vec<BriefBuy> = alerts
            .iter()
            .filter_map(|x| match x {
                AlertItem::Ok(a) if a.alert.buy => {
                    Some(BriefBuy { symbol: a.symbol.clone(), kind: a.kind, strong: a.alert.strong, title: a.alert.title.clone() })
                }
                _ => None,
            })
            .collect();
        buyable.sort_by_key(|b| std::cmp::Reverse(b.strong));
        let prices: Vec<PriceNow> =
            prices.map(|p| p.iter().map(|q| PriceNow { symbol: q.symbol.clone(), kind: q.kind, price: Some(q.price) }).collect()).unwrap_or_default();
        let moves = movers(&prices, &closes.into_iter().collect());
        let top: Vec<Value> =
            news.ok().map(|n| n.top.iter().filter_map(|id| n.items.iter().find(|i| &i.id == id)).take(3).map(to_value).collect()).unwrap_or_default();
        let level = m.as_ref().map(|m| market_level(m.level));
        Ok::<_, crate::http::Error>(json!({
            "asOf": now_ms(),
            "headline": headline(level, &buyable, &moves),
            "market": m.as_ref().map(|m| json!({
                "level": market_level(m.level),
                "label": MARKET_LABEL.iter().find(|(l, _)| *l == market_level(m.level)).map(|(_, s)| *s),
                "score": m.score,
                "themes": m.themes.iter().filter(|t| t.count > 0).map(|t| t.label.clone()).take(3).collect::<Vec<_>>(),
            })),
            "buyable": to_value(&buyable),
            "movers": to_value(&moves[..moves.len().min(6)]),
            "news": top,
        }))
    })
    .await?;
    Ok(json_of(&*out))
}

/// Macro / geopolitical context (market-wide), readable by bots, with the market regime (risk-on / risk-off / neutre).
async fn macro_route() -> ApiResult<Response> {
    let report = macro_now().await?;
    let mut out = to_value(&*report);
    out["regime"] = to_value(&data::macro_regime(&report).await);
    Ok(json_of(&out))
}

/// Decision for one asset: verdict, reasons, vetoes, setup, plan, scenarios, what could make it wrong. Personal
/// only with `cost=` (average cost) or `weights=` (portfolio weights): used for this answer, never stored or logged.
async fn decision_route(Query(p): Q) -> ApiResult<Response> {
    let kind = parse_kind(q(&p, "kind"))?;
    let symbol = parse_symbol(q(&p, "symbol"), kind)?;
    let cost = parse_cost(q(&p, "cost"))?;
    let weights = parse_weights(q(&p, "weights"))?;
    let score_weights = parse_score_weights(q(&p, "w"))?;
    Ok(json_of(&decision_for(&symbol, kind, cost, &weights, score_weights).await?))
}

/// Upcoming events (economy, central banks, earnings, dividends, splits, IPOs) over `days` days (14 by default, 30
/// at most); with `symbols=AAPL,NVDA`, earnings, dividends and splits of these stocks only.
async fn calendar_route(Query(p): Q) -> ApiResult<Response> {
    let days = parse_days(q(&p, "days"), crate::calendar::DEFAULT_DAYS, crate::calendar::MAX_DAYS)?;
    let symbols = parse_symbol_list(q(&p, "symbols"))?;
    Ok(json_of(&crate::calendar::calendar(days, symbols.as_deref()).await))
}

async fn sentiment_route(Query(p): Q) -> ApiResult<Response> {
    let kind = parse_kind(q(&p, "kind"))?;
    Ok(json_of(&sentiment(&parse_symbol(q(&p, "symbol"), kind)?, kind).await))
}

async fn unknown() -> Response {
    (StatusCode::NOT_FOUND, json_of(&json!({ "error": "route inconnue" }))).into_response()
}

/// Private data: never written to the browser's disk cache.
async fn no_store(req: Request, next: Next) -> Response {
    let mut r = next.run(req).await;
    // Set before the route in Express: a route's own value (the live stream's) wins.
    r.headers_mut().entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static("private, no-store"));
    r
}

/// A query string Express would refuse is a 400 like a bad parameter (never a panic or an Axum text error).
async fn query_errors(req: Request, next: Next) -> Response {
    let r = next.run(req).await;
    if r.status() == StatusCode::BAD_REQUEST && r.headers().get(header::CONTENT_TYPE).is_some_and(|v| v.as_bytes().starts_with(b"text/plain")) {
        return (StatusCode::BAD_REQUEST, json_of(&json!({ "error": "requête invalide" }))).into_response();
    }
    r
}

pub fn api(state: AppState) -> Router {
    Router::new()
        .route("/tickers", get(tickers))
        .route("/candles", get(candles))
        .route("/radar", get(radar))
        .route("/search", get(search))
        .route("/universe", get(universe_route))
        .route("/guard", get(guard))
        .route("/live", get(live))
        .route("/zones", get(zones_route))
        .route("/selection", get(selection_route))
        .route("/alerts", get(alerts))
        .route("/history", get(history))
        .route("/news", get(news))
        .route("/brief", get(brief))
        .route("/macro", get(macro_route))
        .route("/sentiment", get(sentiment_route))
        .route("/decision", get(decision_route))
        .route("/calendar", get(calendar_route))
        .fallback(unknown)
        .method_not_allowed_fallback(unknown)
        .layer(middleware::from_fn(query_errors))
        .layer(middleware::from_fn(no_store))
        .layer(RateLimit::new(240, 60_000))
        .with_state(state)
}

/// With ALTIM_LOG=1: one line per request (method, path without the query, status, duration), never the query
/// (it lists the user's assets) nor any header.
async fn log_requests(req: Request, next: Next) -> Response {
    let (method, path) = (req.method().clone(), req.uri().path().to_string());
    let start = std::time::Instant::now();
    let r = next.run(req).await;
    println!("{method} {path} {} {} ms", r.status().as_u16(), start.elapsed().as_millis());
    r
}

/// The whole application: security headers, HTTPS, compression (never on event streams), private access, API, site.
pub fn router(state: AppState, auth: Arc<Auth>) -> Router {
    let app = Router::new()
        .nest("/api", api(state))
        .fallback(web::site)
        .layer(middleware::from_fn_with_state(auth, crate::auth::access))
        .layer(CompressionLayer::new().compress_when(DefaultPredicate::new().and(SizeAbove::new(1024))))
        .layer(middleware::from_fn(web::security_headers))
        .layer(middleware::from_fn(web::https_redirect));
    if std::env::var("ALTIM_LOG").is_ok_and(|v| v == "1") { app.layer(middleware::from_fn(log_requests)) } else { app }
}
