//! Real-time WebSocket of the apps (`GET /api/ws`), protocol version 1.
//!
//! One authenticated socket per screen carries everything that changes while the user looks: live prices, the
//! "can I buy now?" alerts, the Radar verdicts and the EUR/USD rate. `/api/live` (SSE) stays for older clients.
//!
//! # Access
//!
//! - Same session as the rest of the API (the `access` middleware runs first): the `altim_session` cookie (browser,
//!   or the `Cookie` header the iPhone / Android apps send) or the bot token (`Authorization: Bearer`). Without it the
//!   upgrade is answered 401 and no socket is opened.
//! - `Origin`, when present, must be this server's own host (cross-site WebSocket hijacking): 403 otherwise. Native
//!   apps send none.
//! - At most [`MAX_SOCKETS`] sockets per address (IPv6 by /64), messages of [`MAX_MESSAGE`] bytes at most,
//!   [`MAX_ASSETS`] assets per subscription, [`MAX_CLIENT_MESSAGES`] client messages per minute, closed after
//!   [`IDLE_TIMEOUT`] without any frame from the client (pongs count).
//! - The server pings every [`PING_EVERY`] (the Heroku router closes a connection idle for 55 s).
//!
//! # Messages (JSON text frames)
//!
//! Client → server:
//! - `{"type":"subscribe","assets":[{"kind":"crypto","symbol":"BTC"},…],"interval":"4h","currency":"EUR"}`: replaces
//!   the previous subscription (`interval` 1h | 4h | 1d | 4d | 1w, informational; `currency` EUR | USD for the texts).
//! - `{"type":"ping"}`: answered `{"type":"pong","time":…}`.
//!
//! Server → client:
//! - `{"type":"hello","v":1,"maxAssets":20,"pingEvery":20}` once the socket is open.
//! - `{"type":"tick",…}`: the fields of an `/api/live` tick (`symbol`, `kind`, `price`, `change`, `agreeing`,
//!   `total`, `sources`, `time`, `market`): the last known ones right after `subscribe`, then every change.
//! - `{"type":"alerts","items":[…],"checkedAt":…}`: the items of `/api/alerts` for the subscribed list, once after
//!   `subscribe` (when every asset was checked, or after 20 s with those that were), then only when one changed
//!   (price and time stamps excluded: prices come with the ticks).
//! - `{"type":"checked","checkedAt":…}`: the alerts were checked again and did not change (30 s apart at most).
//! - `{"type":"verdict","symbol":…,"kind":…,"verdict":…,"label":…,"rating":…,"ratingLabel":…,"chipNote":…,
//!   "asOf":…}`: the asset's decision (Radar chip), once per asset after `subscribe`, then when it changes.
//! - `{"type":"fx","rate":…,"usdPerEur":…,"asOf":…,"source":…}`: EUR/USD (euro subscriptions only), after
//!   `subscribe` and when the rate changes.
//! - `{"type":"error","code":…,"message":…}`: `bad_message`, `too_many_assets`, `rate_limited` (then closed).
//!
//! # Computation
//!
//! Ticks come from the shared [`LiveHub`](crate::live::LiveHub) (dropping the socket unsubscribes). Alerts and
//! verdicts are computed by one worker per asset and currency, shared by every socket that watches it
//! ([`AlertHub`]): every [`CHECK_EVERY`] for cryptos and for stocks while the US market is open (the inputs are
//! cached 60 s), every [`CHECK_CLOSED`] for stocks while it is closed. The worker stops when no socket watches it.
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequest, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use futures::stream::{SelectAll, Stream};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::time::Instant;

use super::{AppState, StreamSlot, json_of, with_fx};
use crate::js::{now_ms, to_value};
use crate::live::Subscription;
use crate::quotes::make_asset;
use crate::types::{Asset, Interval, Kind};

pub const PROTOCOL: u32 = 1;
/// Sockets per address (phone, watch, browser tabs), like the SSE streams.
pub const MAX_SOCKETS: usize = 8;
/// Largest client message (a subscription of 20 assets is ≈ 1 KB).
pub const MAX_MESSAGE: usize = 16 * 1024;
/// Same cap as `/api/live` and `/api/alerts`.
pub const MAX_ASSETS: usize = super::validate::MAX_ASSETS;
pub const MAX_CLIENT_MESSAGES: usize = 60;
pub const PING_EVERY: Duration = Duration::from_secs(20);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(65);
pub const CHECK_EVERY: Duration = Duration::from_secs(60);
pub const CHECK_CLOSED: Duration = Duration::from_secs(600);
/// The first alerts message waits at most this long for every asset's first check.
pub const FIRST_ALERTS_WAIT: Duration = Duration::from_secs(20);
/// "checked" messages at most this often.
pub const CHECKED_EVERY_MS: i64 = 30_000;

// ---------- Client messages (pure) ----------

#[derive(Deserialize)]
struct AssetRef {
    kind: String,
    symbol: String,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Raw {
    Subscribe {
        #[serde(default)]
        assets: Vec<AssetRef>,
        #[serde(default)]
        interval: Option<String>,
        #[serde(default)]
        currency: Option<String>,
    },
    Ping,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientMsg {
    Subscribe { assets: Vec<Asset>, interval: Interval, usd: bool },
    Ping,
}

/// A refused client message: `code` and the French text sent back in an `error` message.
#[derive(Debug, Clone, PartialEq)]
pub struct Refused {
    pub code: &'static str,
    pub message: String,
}

fn refused(code: &'static str, message: impl Into<String>) -> Refused {
    Refused { code, message: message.into() }
}

/// Parses one client text frame (validation of `/api/live`'s parameters; duplicates kept once).
pub fn parse_client(text: &str) -> Result<ClientMsg, Refused> {
    let raw: Raw = serde_json::from_str(text).map_err(|_| refused("bad_message", "message invalide (subscribe | ping)"))?;
    match raw {
        Raw::Ping => Ok(ClientMsg::Ping),
        Raw::Subscribe { assets, interval, currency } => {
            if assets.len() > MAX_ASSETS {
                return Err(refused("too_many_assets", format!("{MAX_ASSETS} actifs au plus")));
            }
            let bad = |e: super::error::ApiError| match e {
                super::error::ApiError::BadRequest(m) | super::error::ApiError::Upstream(m) => refused("bad_message", m),
            };
            let mut list: Vec<Asset> = Vec::new();
            for a in assets {
                let kind = super::validate::parse_kind(Some(&a.kind)).map_err(bad)?;
                let symbol = super::validate::parse_symbol(Some(&a.symbol), kind).map_err(bad)?;
                if !list.iter().any(|x| x.kind == kind && x.symbol == symbol) {
                    list.push(make_asset(&symbol, kind, None));
                }
            }
            let interval = super::validate::parse_interval(Some(interval.as_deref().unwrap_or("4h"))).map_err(bad)?;
            let usd = super::validate::parse_currency(currency.as_deref()).map_err(bad)?;
            Ok(ClientMsg::Subscribe { assets: list, interval, usd })
        }
    }
}

/// `Origin` allowed: absent (native apps), or this very host over http(s).
pub fn origin_allowed(origin: Option<&str>, host: Option<&str>) -> bool {
    let Some(origin) = origin else { return true };
    let Ok(url) = url::Url::parse(origin) else { return false };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let url_host = match url.port() {
        Some(p) => format!("{}:{p}", url.host_str().unwrap_or("")),
        None => url.host_str().unwrap_or("").to_string(),
    };
    host.is_some_and(|h| !h.is_empty() && h.eq_ignore_ascii_case(&url_host))
}

/// Client messages per minute: a fixed window.
#[derive(Debug, Default)]
pub struct Budget {
    start: i64,
    count: usize,
}

impl Budget {
    /// False once more than [`MAX_CLIENT_MESSAGES`] arrived within the current minute.
    pub fn take(&mut self, now: i64) -> bool {
        if now - self.start >= 60_000 {
            self.start = now;
            self.count = 0;
        }
        self.count += 1;
        self.count <= MAX_CLIENT_MESSAGES
    }
}

// ---------- What was pushed (pure change detection) ----------

/// One asset's last check, shared by every socket watching it.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetState {
    /// The `/api/alerts` item of the asset (with its `fx`).
    pub alert: Value,
    /// The `verdict` message of the asset, when the decision could be computed.
    pub verdict: Option<Value>,
    pub checked_at: i64,
}

/// What a socket already sent: each message type goes out only when its content changed.
#[derive(Debug, Default)]
pub struct Pushed {
    alerts: Option<String>,
    verdicts: HashMap<String, String>,
    fx: Option<f64>,
    checked: i64,
}

/// An item compared without what moves on every check (the price comes with the ticks; time stamps; the rate the
/// texts used, which changes the texts anyway when it matters).
fn alert_signature(item: &Value) -> String {
    let mut v = item.clone();
    if let Some(o) = v.as_object_mut() {
        for k in ["price", "asOf", "fx"] {
            o.remove(k);
        }
    }
    v.to_string()
}

fn verdict_signature(v: &Value) -> String {
    ["verdict", "label", "rating", "chipNote"].iter().map(|k| v.get(*k).map(Value::to_string).unwrap_or_default()).collect::<Vec<_>>().join("|")
}

impl Pushed {
    /// Forgets everything (new subscription: everything is sent again once).
    pub fn reset(&mut self) {
        *self = Pushed { fx: self.fx, ..Pushed::default() };
    }

    /// The `alerts` message for `items` (subscription order) if they differ from the last one sent, else a `checked`
    /// message when the last one is older than [`CHECKED_EVERY_MS`].
    pub fn alerts(&mut self, items: &[Value], checked_at: i64) -> Option<Value> {
        let sig = items.iter().map(alert_signature).collect::<Vec<_>>().join("\n");
        if self.alerts.as_deref() != Some(sig.as_str()) {
            self.alerts = Some(sig);
            self.checked = checked_at;
            return Some(json!({ "type": "alerts", "items": items, "checkedAt": checked_at }));
        }
        if checked_at - self.checked >= CHECKED_EVERY_MS {
            self.checked = checked_at;
            return Some(json!({ "type": "checked", "checkedAt": checked_at }));
        }
        None
    }

    /// The `verdict` messages whose verdict, label, rating or note changed since sent.
    pub fn verdicts<'a>(&mut self, current: impl IntoIterator<Item = &'a Value>) -> Vec<Value> {
        let mut out = Vec::new();
        for v in current {
            let key = format!("{}:{}", v["kind"].as_str().unwrap_or(""), v["symbol"].as_str().unwrap_or(""));
            let sig = verdict_signature(v);
            if self.verdicts.get(&key) != Some(&sig) {
                self.verdicts.insert(key, sig);
                out.push(v.clone());
            }
        }
        out
    }

    /// The `fx` message when the rate changed (or was never sent).
    pub fn fx(&mut self, rate: Option<&FxNow>) -> Option<Value> {
        let r = rate?;
        if self.fx == Some(r.rate) {
            return None;
        }
        self.fx = Some(r.rate);
        Some(json!({ "type": "fx", "rate": r.rate, "usdPerEur": r.usd_per_eur, "asOf": r.time, "source": r.source }))
    }
}

/// Alerts message content of a subscription: the items in its order (assets not checked yet left out) and the
/// oldest check time; None until at least one asset was checked.
pub fn merge_alerts(order: &[String], states: &HashMap<String, Arc<AssetState>>) -> Option<(Vec<Value>, i64)> {
    let ready: Vec<&Arc<AssetState>> = order.iter().filter_map(|k| states.get(k)).collect();
    let checked = ready.iter().map(|s| s.checked_at).min()?;
    Some((ready.iter().map(|s| s.alert.clone()).collect(), checked))
}

/// The `verdict` message of a decision.
pub fn verdict_message(d: &crate::engine::decision_types::Decision) -> Value {
    let v = to_value(d);
    json!({
        "type": "verdict", "symbol": d.symbol, "kind": d.kind, "verdict": v["verdict"], "label": v["label"],
        "rating": v["rating"], "ratingLabel": v["ratingLabel"], "level": v["level"], "confidence": v["confidence"],
        "chipNote": v.get("chipNote").cloned().unwrap_or(Value::Null), "asOf": d.as_of,
    })
}

/// Pause before the next check of an asset.
pub fn check_every(kind: Kind, now: i64) -> Duration {
    match kind {
        Kind::Crypto => CHECK_EVERY,
        Kind::Stock if crate::live::us_market_open(now) => CHECK_EVERY,
        Kind::Stock => CHECK_CLOSED,
    }
}

// ---------- Shared alert workers ----------

type ComputeFn = dyn Fn(Asset, bool) -> Pin<Box<dyn Future<Output = AssetState> + Send>> + Send + Sync;

/// One worker per (asset, currency) watched by at least one socket.
#[derive(Clone)]
pub struct AlertHub {
    workers: Arc<Mutex<HashMap<String, watch::Sender<Option<Arc<AssetState>>>>>>,
    compute: Arc<ComputeFn>,
    every: Arc<dyn Fn(Kind) -> Duration + Send + Sync>,
    /// Kept by the hub (which outlives every socket): a socket's `changed()` never ends on a dropped sender.
    fx: Arc<watch::Sender<Option<FxNow>>>,
    fx_live: bool,
    fx_started: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for AlertHub {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertHub {
    /// `/api/alerts` and `/api/decision` computations (their inputs cached).
    pub fn new() -> Self {
        AlertHub { fx_live: true, ..Self::with(|a, usd| Box::pin(compute(a, usd)), |k| check_every(k, now_ms())) }
    }

    /// With another computation and pace (tests).
    pub fn with(
        compute: impl Fn(Asset, bool) -> Pin<Box<dyn Future<Output = AssetState> + Send>> + Send + Sync + 'static,
        every: impl Fn(Kind) -> Duration + Send + Sync + 'static,
    ) -> Self {
        AlertHub {
            workers: Arc::default(),
            compute: Arc::new(compute),
            every: Arc::new(every),
            fx: Arc::new(watch::channel(None).0),
            fx_live: false,
            fx_started: Arc::default(),
        }
    }

    fn key(a: &Asset, usd: bool) -> String {
        format!("{}:{}:{}", a.kind.as_str(), a.symbol, if usd { "USD" } else { "EUR" })
    }

    /// Number of running workers (monitoring, tests).
    pub fn workers(&self) -> usize {
        self.workers.lock().unwrap().len()
    }

    /// Watches one asset: the worker starts on the first watcher and stops after the last one is dropped.
    pub fn watch(&self, a: &Asset, usd: bool) -> watch::Receiver<Option<Arc<AssetState>>> {
        let key = Self::key(a, usd);
        let mut w = self.workers.lock().unwrap();
        if let Some(tx) = w.get(&key) {
            return tx.subscribe();
        }
        let (tx, rx) = watch::channel(None);
        w.insert(key.clone(), tx.clone());
        let (hub, asset) = (self.clone(), a.clone());
        tokio::spawn(async move {
            loop {
                let state = tokio::select! {
                    s = (hub.compute)(asset.clone(), usd) => s,
                    _ = tx.closed() => break,
                };
                tx.send_replace(Some(Arc::new(state)));
                let pause = (hub.every)(asset.kind);
                tokio::select! {
                    _ = tokio::time::sleep(pause) => {}
                    _ = tx.closed() => {}
                }
                // Removed under the lock: a socket arriving now gets a new worker, never a dead one.
                let mut w = hub.workers.lock().unwrap();
                if tx.receiver_count() == 0 {
                    w.remove(&key);
                    return;
                }
            }
            hub.workers.lock().unwrap().remove(&key);
        });
        rx
    }
}

/// One check of an asset: its `/api/alerts` item and its decision's verdict, in the subscription's currency.
async fn compute(a: Asset, usd: bool) -> AssetState {
    crate::fx::ensure().await;
    crate::fx::scope(usd, async move {
        let decision = tokio::time::timeout(Duration::from_secs(12), super::data::decision_for(&a.symbol, a.kind, None, &[], None));
        let (alerts, d) = tokio::join!(super::data::alerts_for(std::slice::from_ref(&a)), decision);
        let alert = alerts
            .first()
            .map(|x| with_fx(to_value(x)))
            .unwrap_or_else(|| json!({ "symbol": a.symbol, "kind": a.kind, "name": a.name, "error": "indisponible" }));
        let verdict = d.ok().and_then(|d| d.ok()).map(|d| verdict_message(&d));
        AssetState { alert, verdict, checked_at: now_ms() }
    })
    .await
}

// ---------- EUR/USD ----------

#[derive(Debug, Clone, PartialEq)]
pub struct FxNow {
    pub rate: f64,
    pub usd_per_eur: f64,
    pub time: i64,
    pub source: String,
}

impl AlertHub {
    /// EUR/USD as the server knows it, refreshed by one task for every socket (checked each minute; the rate itself
    /// is refetched every 10 minutes by `crate::fx`). A hub made with [`AlertHub::with`] never fetches it.
    pub fn fx(&self) -> watch::Receiver<Option<FxNow>> {
        if self.fx_live && !self.fx_started.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let t = self.fx.clone();
            tokio::spawn(async move {
                loop {
                    let r = crate::fx::ensure().await.map(|r| FxNow { rate: r.rate, usd_per_eur: r.usd_per_eur, time: r.time, source: r.source });
                    t.send_if_modified(|cur| {
                        if r.is_some() && cur.as_ref().map(|c: &FxNow| c.rate) != r.as_ref().map(|c| c.rate) {
                            *cur = r;
                            true
                        } else {
                            false
                        }
                    });
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            });
        }
        self.fx.subscribe()
    }
}

// ---------- Route ----------

fn refuse(status: StatusCode, msg: &str) -> Response {
    (status, json_of(&json!({ "error": msg }))).into_response()
}

/// `GET /api/ws`: the access middleware already refused a request without a session.
pub async fn ws_route(State(st): State<AppState>, req: Request) -> Response {
    let headers: &HeaderMap = req.headers();
    let header_str = |n: header::HeaderName| headers.get(n).and_then(|v| v.to_str().ok()).map(str::to_string);
    let host = header_str(header::HOST).or_else(|| req.uri().authority().map(|a| a.as_str().to_string()));
    if !origin_allowed(header_str(header::ORIGIN).as_deref(), host.as_deref()) {
        return refuse(StatusCode::FORBIDDEN, "origine refusée");
    }
    let who = crate::auth::client_key(&crate::auth::express::client_ip(&req).unwrap_or_else(|| "?".into()));
    let ws = match WebSocketUpgrade::from_request(req, &st).await {
        Ok(ws) => ws,
        Err(_) => return refuse(StatusCode::BAD_REQUEST, "connexion WebSocket attendue"),
    };
    {
        let mut m = st.sockets.lock().unwrap();
        let open = m.get(&who).copied().unwrap_or(0);
        if open >= MAX_SOCKETS {
            let mut r = refuse(StatusCode::TOO_MANY_REQUESTS, "Trop de connexions en direct ouvertes.");
            r.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("10"));
            return r;
        }
        m.insert(who.clone(), open + 1);
    }
    let slot = StreamSlot { streams: st.sockets.clone(), who };
    ws.max_message_size(MAX_MESSAGE).max_frame_size(MAX_MESSAGE).on_upgrade(move |socket| run(socket, st, slot))
}

type AlertStream = Pin<Box<dyn Stream<Item = (String, Arc<AssetState>)> + Send>>;

/// The subscription of a socket (its ticks and alert streams are kept apart: `select!` polls them together).
struct Watching {
    order: Vec<String>,
    states: HashMap<String, Arc<AssetState>>,
    usd: bool,
}

async fn next_tick(t: &mut Option<Subscription>) -> Option<crate::live::Tick> {
    match t {
        Some(t) => t.recv().await,
        None => std::future::pending().await,
    }
}

async fn next_state(s: &mut SelectAll<AlertStream>) -> Option<(String, Arc<AssetState>)> {
    if s.is_empty() { std::future::pending().await } else { s.next().await }
}

async fn deadline(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

fn tick_message(t: &crate::live::Tick) -> Value {
    let mut v = to_value(t);
    if let Some(o) = v.as_object_mut() {
        o.insert("type".into(), json!("tick"));
    }
    v
}

fn text(v: &Value) -> Message {
    Message::Text(v.to_string().into())
}

fn alert_streams(st: &AppState, assets: &[Asset], order: &[String], usd: bool) -> SelectAll<AlertStream> {
    let mut streams: SelectAll<AlertStream> = SelectAll::new();
    for (a, key) in assets.iter().zip(order) {
        let key = key.clone();
        let s = tokio_stream::wrappers::WatchStream::new(st.alerts.watch(a, usd)).filter_map(move |s| {
            let key = key.clone();
            async move { s.map(|s| (key, s)) }
        });
        streams.push(Box::pin(s));
    }
    streams
}

async fn run(socket: WebSocket, st: AppState, slot: StreamSlot) {
    let _slot = slot;
    let (mut tx, mut rx) = socket.split();
    let hello = json!({ "type": "hello", "v": PROTOCOL, "maxAssets": MAX_ASSETS, "pingEvery": PING_EVERY.as_secs() });
    if tx.send(text(&hello)).await.is_err() {
        return;
    }
    let mut watching: Option<Watching> = None;
    let mut ticks: Option<Subscription> = None;
    let mut streams: SelectAll<AlertStream> = SelectAll::new();
    // While set, the first alerts message waits for every asset's first check (at most until this instant).
    let mut first: Option<Instant> = None;
    let mut pushed = Pushed::default();
    let mut fx = st.alerts.fx();
    let mut budget = Budget::default();
    let mut last_seen = Instant::now();
    let mut ping = tokio::time::interval_at(Instant::now() + PING_EVERY, PING_EVERY);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let mut out: Vec<Value> = Vec::new();
        let mut close: Option<(u16, &'static str)> = None;
        tokio::select! {
            m = rx.next() => {
                let Some(Ok(m)) = m else { break };
                last_seen = Instant::now();
                match m {
                    Message::Text(t) => {
                        if !budget.take(now_ms()) {
                            out.push(json!({ "type": "error", "code": "rate_limited", "message": "trop de messages" }));
                            close = Some((1008, "trop de messages"));
                        } else {
                            match parse_client(t.as_str()) {
                                Ok(ClientMsg::Ping) => out.push(json!({ "type": "pong", "time": now_ms() })),
                                Ok(ClientMsg::Subscribe { assets, usd, .. }) => {
                                    // The previous subscription is dropped first (its feeds and workers may stop).
                                    drop(ticks.take());
                                    drop(std::mem::take(&mut streams));
                                    let order: Vec<String> = assets.iter().map(|a| format!("{}:{}", a.kind.as_str(), a.symbol)).collect();
                                    ticks = Some(st.live.subscribe(&order));
                                    streams = alert_streams(&st, &assets, &order, usd);
                                    first = (!order.is_empty()).then(|| Instant::now() + FIRST_ALERTS_WAIT);
                                    out.extend(st.live.snapshot(&order).iter().map(tick_message));
                                    pushed.reset();
                                    if !usd {
                                        let now = fx.borrow_and_update().clone();
                                        out.extend(pushed.fx(now.as_ref()));
                                    }
                                    watching = Some(Watching { order, states: HashMap::new(), usd });
                                }
                                Err(e) => out.push(json!({ "type": "error", "code": e.code, "message": e.message })),
                            }
                        }
                    }
                    Message::Binary(_) => out.push(json!({ "type": "error", "code": "bad_message", "message": "texte JSON attendu" })),
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
            t = next_tick(&mut ticks) => {
                let Some(t) = t else { break };
                out.push(tick_message(&t));
            }
            s = next_state(&mut streams) => {
                if let (Some((key, s)), Some(w)) = (s, watching.as_mut()) {
                    w.states.insert(key, s);
                    if first.is_some() && w.order.iter().all(|k| w.states.contains_key(k)) {
                        first = None;
                    }
                    if first.is_none() {
                        push_checks(w, &mut pushed, &mut out);
                    }
                }
            }
            _ = deadline(first) => {
                first = None;
                if let Some(w) = watching.as_ref() {
                    push_checks(w, &mut pushed, &mut out);
                }
            }
            r = fx.changed() => {
                if r.is_ok() && watching.as_ref().is_some_and(|w| !w.usd) {
                    let now = fx.borrow_and_update().clone();
                    out.extend(pushed.fx(now.as_ref()));
                }
            }
            _ = ping.tick() => {
                if last_seen.elapsed() > IDLE_TIMEOUT {
                    close = Some((1001, "inactif"));
                } else if tx.send(Message::Ping(Default::default())).await.is_err() {
                    break;
                }
            }
        }
        for v in &out {
            if tx.send(text(v)).await.is_err() {
                return;
            }
        }
        if let Some((code, reason)) = close {
            let _ = tx.send(Message::Close(Some(CloseFrame { code, reason: reason.into() }))).await;
            break;
        }
    }
}

/// Alerts (or "checked") and changed verdicts of a socket after a check.
fn push_checks(w: &Watching, pushed: &mut Pushed, out: &mut Vec<Value>) {
    if let Some((items, checked)) = merge_alerts(&w.order, &w.states) {
        out.extend(pushed.alerts(&items, checked));
    }
    let verdicts: Vec<&Value> = w.order.iter().filter_map(|k| w.states.get(k)).filter_map(|s| s.verdict.as_ref()).collect();
    out.extend(pushed.verdicts(verdicts));
}
