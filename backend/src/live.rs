//! Live prices, nothing frozen (`web/server/live.ts`).
//!
//! - Crypto: real-time WebSocket feeds from 7 exchanges (OKX, Coinbase, Kraken, Bitfinex, Bitget, Gate, Crypto.com),
//!   one connection per exchange shared by every visitor, subscriptions added / removed on demand. The live price is
//!   the median of the exchanges that agree (an exchange more than 1 % away from the others is ignored).
//! - Stocks: no free real-time feed exists; the fast quote sources are polled every 5 s while someone watches.
//!   Cryptos listed on none of these exchanges are polled the same way from the REST sources.
//!
//! # Use from the SSE route (`GET /api/live`)
//!
//! One `LiveHub` for the whole server (created lazily, like `liveHub()` in app.ts), shared through the Axum state
//! (`LiveHub` is a cheap `Clone`). Keys are `"crypto:BTC"` / `"stock:AAPL"`.
//!
//! ```ignore
//! let keys: Vec<String> = assets.iter().map(|a| format!("{}:{}", a.kind.as_str(), a.symbol)).collect();
//! let sub = hub.subscribe(&keys);          // 1. subscribe (starts feeds / polling)
//! let first = hub.snapshot(&keys);         // 2. last known ticks, sent right away
//! // body: "retry: 3000\n\n", then `data: {json}\n\n` for each tick of `first` then of `sub` (a `Stream<Item = Tick>`),
//! // plus a ": ok\n\n" comment every 15 s. Serialize ticks with `crate::js::to_value` (integral numbers as JSON integers).
//! // Dropping `sub` (client gone, stream dropped) unsubscribes: nothing else to call.
//! ```
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::task::{Context, Poll};
use std::time::Duration;

use chrono::{Datelike, Weekday};
use chrono_tz::America::New_York;
use futures::future::join_all;
use futures::{SinkExt, Stream, StreamExt};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

use crate::js::{median, now_ms};
use crate::jsval::{get, idx, to_string};
use crate::quotes::{QUOTE_SOURCES, QuoteSource};
use crate::types::{Asset, Kind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Market {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tick {
    pub symbol: String,
    pub kind: Kind,
    pub price: f64,
    /// 24 h change in %, median of the sources that give it.
    pub change: Option<f64>,
    /// Sources agreeing / sources with a live price.
    pub agreeing: usize,
    pub total: usize,
    pub sources: Vec<String>,
    pub time: i64,
    /// Stocks: whether the US regular session is open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<Market>,
}

/// Quote of one source (`Quote` in live.ts).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveQuote {
    pub price: f64,
    pub change: Option<f64>,
    pub time: i64,
}

/// A price decoded from an exchange message.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Parsed {
    pub base: String,
    pub price: f64,
    pub change: Option<f64>,
}

/// `Omit<Tick, "symbol" | "kind" | "market">`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveConsensus {
    pub price: f64,
    pub change: Option<f64>,
    pub agreeing: usize,
    pub total: usize,
    pub sources: Vec<String>,
    pub time: i64,
}

/// `num` in live.ts: a string or a number, kept when finite.
fn num(v: Option<&Value>) -> Option<f64> {
    let x = match v {
        Some(Value::String(s)) => crate::jsval::string_to_number(s),
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        _ => f64::NAN,
    };
    x.is_finite().then_some(x)
}
fn truthy(x: Option<f64>) -> Option<f64> {
    x.filter(|p| *p != 0.0)
}
fn strip<'a>(s: &'a str, suffix: &str) -> &'a str {
    s.strip_suffix(suffix).unwrap_or(s)
}
fn arr(v: Option<&Value>) -> Option<&Vec<Value>> {
    v.and_then(|v| v.as_array())
}
fn is(v: Option<&Value>, s: &str) -> bool {
    v.and_then(|v| v.as_str()) == Some(s)
}
/// Change from the 24 h open: `open ? (price / open - 1) * 100 : null`.
fn from_open(price: f64, open: Option<f64>) -> Option<f64> {
    truthy(open).map(|o| (price / o - 1.0) * 100.0)
}

// ---------- Exchange feeds ----------

/// Bitfinex channel ids → base (a JavaScript `Map`, keys compared like `SameValueZero`).
#[derive(Debug, Clone, Default)]
pub struct ChanIds(pub Vec<(Value, String)>);

impl ChanIds {
    fn same(a: &Value, b: &Value) -> bool {
        match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => x == y,
            _ => a == b,
        }
    }
    pub fn set(&mut self, k: Value, v: String) {
        match self.0.iter_mut().find(|(x, _)| Self::same(x, &k)) {
            Some(slot) => slot.1 = v,
            None => self.0.push((k, v)),
        }
    }
    pub fn get(&self, k: &Value) -> Option<&String> {
        self.0.iter().find(|(x, _)| Self::same(x, k)).map(|(_, v)| v)
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Ping {
    pub every: Duration,
    pub message: fn() -> Value,
}

/// One exchange's WebSocket ticker feed. A `Value::String` message is sent as is, anything else as JSON.
#[derive(Debug, Clone, Copy)]
pub struct FeedSpec {
    pub name: &'static str,
    pub url: &'static str,
    pub subscribe: fn(&[String]) -> Vec<Value>,
    pub unsubscribe: fn(&[String]) -> Vec<Value>,
    pub parse: fn(&Value, &mut ChanIds) -> Vec<Parsed>,
    /// Messages to send periodically to keep the connection open.
    pub ping: Option<Ping>,
    /// Reply required by the exchange to some messages (heartbeats).
    pub reply: Option<fn(&Value) -> Option<Value>>,
}

fn bfx_symbol(b: &str) -> String {
    if b.encode_utf16().count() > 3 { format!("t{b}:USD") } else { format!("t{b}USD") }
}
fn secs() -> i64 {
    now_ms().div_euclid(1000)
}

fn okx_args(b: &[String]) -> Value {
    b.iter().map(|x| json!({ "channel": "tickers", "instId": format!("{x}-USDT") })).collect()
}
fn coinbase_ids(b: &[String]) -> Value {
    b.iter().map(|x| Value::String(format!("{x}-USD"))).collect()
}
fn kraken_symbols(b: &[String]) -> Value {
    b.iter().map(|x| Value::String(format!("{x}/USD"))).collect()
}
fn bitget_args(b: &[String]) -> Value {
    b.iter().map(|x| json!({ "instType": "SPOT", "channel": "ticker", "instId": format!("{x}USDT") })).collect()
}
fn gate_pairs(b: &[String]) -> Value {
    b.iter().map(|x| Value::String(format!("{x}_USDT"))).collect()
}
fn cdc_channels(b: &[String]) -> Value {
    b.iter().map(|x| Value::String(format!("ticker.{x}_USDT"))).collect()
}

/// The 7 exchange feeds (message formats checked on real streams, see tests/live.rs).
pub static FEEDS: LazyLock<Vec<FeedSpec>> = LazyLock::new(|| {
    vec![
        FeedSpec {
            name: "OKX",
            url: "wss://ws.okx.com/ws/v5/public",
            subscribe: |b| vec![json!({ "op": "subscribe", "args": okx_args(b) })],
            unsubscribe: |b| vec![json!({ "op": "unsubscribe", "args": okx_args(b) })],
            parse: |m, _| {
                let m = Some(m);
                if !is(get(get(m, "arg"), "channel"), "tickers") {
                    return vec![];
                }
                let Some(data) = arr(get(m, "data")) else { return vec![] };
                data.iter()
                    .filter_map(|d| {
                        let d = Some(d);
                        let price = truthy(num(get(d, "last")))?;
                        Some(Parsed { base: strip(&to_string(get(d, "instId")), "-USDT").into(), price, change: from_open(price, num(get(d, "open24h"))) })
                    })
                    .collect()
            },
            ping: Some(Ping { every: Duration::from_millis(25_000), message: || Value::String("ping".into()) }),
            reply: None,
        },
        FeedSpec {
            name: "Coinbase",
            url: "wss://ws-feed.exchange.coinbase.com",
            subscribe: |b| vec![json!({ "type": "subscribe", "product_ids": coinbase_ids(b), "channels": ["ticker"] })],
            unsubscribe: |b| vec![json!({ "type": "unsubscribe", "product_ids": coinbase_ids(b), "channels": ["ticker"] })],
            parse: |m, _| {
                let m = Some(m);
                if !is(get(m, "type"), "ticker") {
                    return vec![];
                }
                let Some(price) = truthy(num(get(m, "price"))) else { return vec![] };
                vec![Parsed { base: strip(&to_string(get(m, "product_id")), "-USD").into(), price, change: from_open(price, num(get(m, "open_24h"))) }]
            },
            ping: None,
            reply: None,
        },
        FeedSpec {
            name: "Kraken",
            url: "wss://ws.kraken.com/v2",
            subscribe: |b| vec![json!({ "method": "subscribe", "params": { "channel": "ticker", "symbol": kraken_symbols(b) } })],
            unsubscribe: |b| vec![json!({ "method": "unsubscribe", "params": { "channel": "ticker", "symbol": kraken_symbols(b) } })],
            parse: |m, _| {
                let m = Some(m);
                if !is(get(m, "channel"), "ticker") {
                    return vec![];
                }
                let Some(data) = arr(get(m, "data")) else { return vec![] };
                data.iter()
                    .filter_map(|d| {
                        let d = Some(d);
                        let price = truthy(num(get(d, "last")))?;
                        Some(Parsed { base: strip(&to_string(get(d, "symbol")), "/USD").into(), price, change: num(get(d, "change_pct")) })
                    })
                    .collect()
            },
            ping: Some(Ping { every: Duration::from_millis(30_000), message: || json!({ "method": "ping" }) }),
            reply: None,
        },
        FeedSpec {
            name: "Bitfinex",
            url: "wss://api-pub.bitfinex.com/ws/2",
            subscribe: |b| b.iter().map(|x| json!({ "event": "subscribe", "channel": "ticker", "symbol": bfx_symbol(x) })).collect(),
            // Handled by closing the channel ids (see LiveHub).
            unsubscribe: |_| vec![],
            parse: |m, feed| {
                let mv = Some(m);
                if is(get(mv, "event"), "subscribed") && is(get(mv, "channel"), "ticker") {
                    let sym = to_string(get(mv, "symbol"));
                    let mut chars = sym.chars();
                    chars.next();
                    let s = chars.as_str();
                    let base = s.strip_suffix(":USD").or_else(|| s.strip_suffix("USD")).unwrap_or(s);
                    feed.set(get(mv, "chanId").cloned().unwrap_or(Value::Null), base.into());
                    return vec![];
                }
                if let (Some(_), Some(d)) = (m.as_array(), arr(idx(mv, 1))) {
                    let Some(base) = idx(mv, 0).and_then(|id| feed.get(id)) else { return vec![] };
                    let d = Some(&Value::Array(d.clone()));
                    let Some(price) = truthy(num(idx(d, 6))) else { return vec![] };
                    return vec![Parsed { base: base.clone(), price, change: num(idx(d, 5)).map(|c| c * 100.0) }];
                }
                vec![]
            },
            ping: None,
            reply: None,
        },
        FeedSpec {
            name: "Bitget",
            url: "wss://ws.bitget.com/v2/ws/public",
            subscribe: |b| vec![json!({ "op": "subscribe", "args": bitget_args(b) })],
            unsubscribe: |b| vec![json!({ "op": "unsubscribe", "args": bitget_args(b) })],
            parse: |m, _| {
                let m = Some(m);
                if !is(get(get(m, "arg"), "channel"), "ticker") {
                    return vec![];
                }
                let Some(data) = arr(get(m, "data")) else { return vec![] };
                data.iter()
                    .filter_map(|d| {
                        let d = Some(d);
                        let price = truthy(num(get(d, "lastPr")))?;
                        Some(Parsed { base: strip(&to_string(get(d, "instId")), "USDT").into(), price, change: from_open(price, num(get(d, "open24h"))) })
                    })
                    .collect()
            },
            ping: Some(Ping { every: Duration::from_millis(25_000), message: || Value::String("ping".into()) }),
            reply: None,
        },
        FeedSpec {
            name: "Gate.io",
            url: "wss://api.gateio.ws/ws/v4/",
            subscribe: |b| vec![json!({ "time": secs(), "channel": "spot.tickers", "event": "subscribe", "payload": gate_pairs(b) })],
            unsubscribe: |b| vec![json!({ "time": secs(), "channel": "spot.tickers", "event": "unsubscribe", "payload": gate_pairs(b) })],
            parse: |m, _| {
                let m = Some(m);
                let r = get(m, "result");
                if !is(get(m, "channel"), "spot.tickers") || !is(get(m, "event"), "update") || !crate::jsval::truthy(r) {
                    return vec![];
                }
                let Some(price) = truthy(num(get(r, "last"))) else { return vec![] };
                vec![Parsed { base: strip(&to_string(get(r, "currency_pair")), "_USDT").into(), price, change: num(get(r, "change_percentage")) }]
            },
            ping: Some(Ping { every: Duration::from_millis(20_000), message: || json!({ "time": secs(), "channel": "spot.ping" }) }),
            reply: None,
        },
        FeedSpec {
            name: "Crypto.com",
            url: "wss://stream.crypto.com/exchange/v1/market",
            subscribe: |b| vec![json!({ "id": now_ms(), "method": "subscribe", "params": { "channels": cdc_channels(b) } })],
            unsubscribe: |b| vec![json!({ "id": now_ms(), "method": "unsubscribe", "params": { "channels": cdc_channels(b) } })],
            parse: |m, _| {
                let m = Some(m);
                let r = get(m, "result");
                if !is(get(m, "method"), "subscribe") || !is(get(r, "channel"), "ticker") {
                    return vec![];
                }
                let Some(data) = arr(get(r, "data")) else { return vec![] };
                let base = strip(&to_string(get(r, "instrument_name")), "_USDT").to_string();
                data.iter()
                    .filter_map(|d| {
                        let d = Some(d);
                        let price = truthy(num(get(d, "a")))?;
                        Some(Parsed { base: base.clone(), price, change: num(get(d, "c")).map(|c| c * 100.0) })
                    })
                    .collect()
            },
            ping: None,
            reply: Some(|m| {
                if !is(get(Some(m), "method"), "public/heartbeat") {
                    return None;
                }
                let mut o = serde_json::Map::new();
                if let Some(id) = get(Some(m), "id") {
                    o.insert("id".into(), id.clone());
                }
                o.insert("method".into(), "public/respond-heartbeat".into());
                Some(Value::Object(o))
            }),
        },
    ]
});

// ---------- Consensus of live quotes ----------

/// Median of the fresh quotes; a source more than `tolerance` % from the median is ignored.
pub fn live_consensus(quotes: &IndexMap<String, LiveQuote>, now: i64, max_age: i64, tolerance: f64) -> Option<LiveConsensus> {
    let fresh: Vec<(&String, &LiveQuote)> = quotes.iter().filter(|(_, q)| now - q.time <= max_age && q.price > 0.0).collect();
    if fresh.is_empty() {
        return None;
    }
    let reference = median(&fresh.iter().map(|(_, q)| q.price).collect::<Vec<_>>());
    let ok: Vec<&(&String, &LiveQuote)> = fresh.iter().filter(|(_, q)| (q.price / reference - 1.0).abs() * 100.0 <= tolerance).collect();
    let changes: Vec<f64> = ok.iter().filter_map(|(_, q)| q.change).filter(|c| c.is_finite()).collect();
    Some(LiveConsensus {
        price: median(&ok.iter().map(|(_, q)| q.price).collect::<Vec<_>>()),
        change: if changes.is_empty() { None } else { Some(median(&changes)) },
        agreeing: ok.len(),
        total: fresh.len(),
        sources: ok.iter().map(|(n, _)| (*n).clone()).collect(),
        time: ok.iter().map(|(_, q)| q.time).max().unwrap_or(i64::MIN),
    })
}

/// US regular session (9:30 – 16:00 New York, weekdays). Holidays are not known: the price simply stops moving.
pub fn us_market_open(now: i64) -> bool {
    let Some(d) = chrono::DateTime::from_timestamp_millis(now) else { return false };
    let d = d.with_timezone(&New_York);
    if matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
        return false;
    }
    let open = crate::market::ny_open(d.year(), d.month(), d.day());
    now >= open && now < open + 23_400_000
}

// ---------- Exchange connections ----------

enum Cmd {
    Add(Vec<String>),
    Remove(Vec<String>),
    Close,
}

type OnQuote = Arc<dyn Fn(&str, Parsed) + Send + Sync>;

/// One WebSocket per exchange, run by its own task (spawned on the first subscription).
struct ExchangeConnection {
    spec: FeedSpec,
    tx: Mutex<Option<mpsc::UnboundedSender<Cmd>>>,
    on_quote: OnQuote,
}

impl ExchangeConnection {
    fn cmd(&self, c: Cmd, spawn: bool) {
        let mut tx = self.tx.lock().unwrap();
        if let Some(t) = tx.as_ref() {
            if !t.is_closed() {
                let _ = t.send(c);
                return;
            }
        }
        if !spawn {
            return;
        }
        let (t, rx) = mpsc::unbounded_channel();
        let _ = t.send(c);
        tokio::spawn(run_connection(self.spec, rx, self.on_quote.clone()));
        *tx = Some(t);
    }
    fn add(&self, bases: &[String]) {
        self.cmd(Cmd::Add(bases.to_vec()), true);
    }
    fn remove(&self, bases: &[String]) {
        self.cmd(Cmd::Remove(bases.to_vec()), false);
    }
    fn close(&self) {
        self.cmd(Cmd::Close, false);
    }
}

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn send(ws: &mut Ws, msgs: Vec<Value>) {
    for m in msgs {
        let text = match m {
            Value::String(s) => s,
            other => other.to_string(),
        };
        let _ = ws.send(Message::text(text)).await;
    }
}

/// What the connection does after a command (`Continue`, back to idle after `close()`, or stop).
enum Next {
    Continue,
    Idle,
    Stop,
}

fn add_bases(bases: &mut IndexSet<String>, new: Vec<String>) -> Vec<String> {
    let fresh: Vec<String> = new.into_iter().filter(|b| !bases.contains(b)).collect::<IndexSet<_>>().into_iter().collect();
    bases.extend(fresh.iter().cloned());
    fresh
}

/// `remove(bases)` of the TypeScript; `open` = a live WebSocket to send the unsubscriptions on.
async fn remove_bases(spec: &FeedSpec, bases: &mut IndexSet<String>, chan_ids: &mut ChanIds, gone: Vec<String>, ws: Option<&mut Ws>) -> Next {
    let gone: Vec<String> = gone.into_iter().filter(|b| bases.shift_remove(b)).collect();
    if gone.is_empty() {
        return Next::Continue;
    }
    if bases.is_empty() {
        return Next::Idle;
    }
    if spec.name == "Bitfinex" {
        let ids: Vec<Value> = chan_ids.0.iter().filter(|(_, b)| gone.contains(b)).map(|(id, _)| id.clone()).collect();
        chan_ids.0.retain(|(_, b)| !gone.contains(b));
        if let Some(ws) = ws {
            send(ws, ids.into_iter().map(|id| json!({ "event": "unsubscribe", "chanId": id })).collect()).await;
        }
    } else if let Some(ws) = ws {
        send(ws, (spec.unsubscribe)(&gone)).await;
    }
    Next::Continue
}

async fn run_connection(spec: FeedSpec, mut rx: mpsc::UnboundedReceiver<Cmd>, on_quote: OnQuote) {
    let mut bases: IndexSet<String> = IndexSet::new();
    let mut chan_ids = ChanIds::default();
    let mut retry: u32 = 0;
    let mut connect_now = false;
    loop {
        // Idle: no socket, waiting for a subscription.
        if !connect_now {
            match rx.recv().await {
                None | Some(Cmd::Close) => return,
                Some(Cmd::Add(b)) => connect_now = !add_bases(&mut bases, b).is_empty(),
                Some(Cmd::Remove(b)) => {
                    remove_bases(&spec, &mut bases, &mut chan_ids, b, None).await;
                }
            }
            continue;
        }
        connect_now = false;
        // Connecting (subscriptions made meanwhile are sent on open, with the whole set).
        let connecting = tokio_tungstenite::connect_async(spec.url);
        tokio::pin!(connecting);
        let ws = loop {
            tokio::select! {
                r = &mut connecting => break Some(r.ok().map(|(ws, _)| ws)),
                c = rx.recv() => match c {
                    None | Some(Cmd::Close) => return,
                    Some(Cmd::Add(b)) => { add_bases(&mut bases, b); }
                    Some(Cmd::Remove(b)) => {
                        if let Next::Idle = remove_bases(&spec, &mut bases, &mut chan_ids, b, None).await {
                            break None;
                        }
                    }
                },
            }
        };
        let Some(ws) = ws else { continue }; // closed by us while connecting
        if let Some(mut ws) = ws {
            retry = 0;
            chan_ids.clear();
            if !bases.is_empty() {
                send(&mut ws, (spec.subscribe)(&bases.iter().cloned().collect::<Vec<_>>())).await;
            }
            let mut ping = spec.ping.map(|p| {
                let mut i = tokio::time::interval_at(tokio::time::Instant::now() + p.every, p.every);
                i.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                (i, p.message)
            });
            let next = loop {
                let tick = async {
                    match ping.as_mut() {
                        Some((i, _)) => {
                            i.tick().await;
                        }
                        None => std::future::pending::<()>().await,
                    }
                };
                tokio::select! {
                    msg = ws.next() => match msg {
                        Some(Ok(Message::Text(t))) => {
                            let text = t.as_str();
                            if text.is_empty() || text == "pong" {
                                continue;
                            }
                            let Ok(msg) = serde_json::from_str::<Value>(text) else { continue };
                            if let Some(r) = spec.reply.and_then(|f| f(&msg)) {
                                send(&mut ws, vec![r]).await;
                            }
                            for p in (spec.parse)(&msg, &mut chan_ids) {
                                if bases.contains(&p.base) {
                                    on_quote(spec.name, p);
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break Next::Continue,
                        Some(Ok(_)) => {}
                    },
                    _ = tick => {
                        if let Some((_, message)) = ping.as_ref() {
                            send(&mut ws, vec![message()]).await;
                        }
                    }
                    c = rx.recv() => match c {
                        None | Some(Cmd::Close) => {
                            let _ = ws.close(None).await;
                            break Next::Stop;
                        }
                        Some(Cmd::Add(b)) => {
                            let fresh = add_bases(&mut bases, b);
                            if !fresh.is_empty() {
                                send(&mut ws, (spec.subscribe)(&fresh)).await;
                            }
                        }
                        Some(Cmd::Remove(b)) => {
                            if let Next::Idle = remove_bases(&spec, &mut bases, &mut chan_ids, b, Some(&mut ws)).await {
                                let _ = ws.close(None).await;
                                break Next::Idle;
                            }
                        }
                    },
                }
            };
            match next {
                Next::Stop => return,
                Next::Idle => continue,
                Next::Continue => {}
            }
        }
        // Closed by the exchange (or failed to connect): reconnect with backoff while someone watches.
        if bases.is_empty() {
            continue;
        }
        let delay = Duration::from_millis(30_000.min(1000u64.saturating_mul(2u64.saturating_pow(retry))));
        retry = retry.saturating_add(1);
        let sleep = tokio::time::sleep(delay);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                _ = &mut sleep => {
                    connect_now = !bases.is_empty();
                    break;
                }
                c = rx.recv() => match c {
                    None | Some(Cmd::Close) => return,
                    Some(Cmd::Add(b)) => {
                        // `add` with no socket connects right away.
                        if !add_bases(&mut bases, b).is_empty() {
                            connect_now = true;
                            break;
                        }
                    }
                    Some(Cmd::Remove(b)) => {
                        if let Next::Idle = remove_bases(&spec, &mut bases, &mut chan_ids, b, None).await {
                            break;
                        }
                    }
                },
            }
        }
    }
}

// ---------- Hub ----------

/// Stock quote sources fast enough to poll every few seconds.
const FAST_STOCK: [&str; 5] = ["Robinhood", "TradingView", "Zacks", "Webull", "Fidelity"];
/// A WebSocket quote younger than this makes REST polling useless for that crypto.
pub const WS_FRESH: i64 = 15_000;

#[derive(Default)]
struct State {
    /// key → listener id → channel.
    listeners: IndexMap<String, IndexMap<u64, mpsc::Sender<Tick>>>,
    /// key → source → quote.
    quotes: HashMap<String, IndexMap<String, LiveQuote>>,
    last: HashMap<String, Tick>,
    last_emit: HashMap<String, i64>,
    pending: HashMap<String, JoinHandle<()>>,
    timer: Option<JoinHandle<()>>,
    polling: bool,
    /// Stocks: US session state at the last poll.
    market: HashMap<String, Market>,
    next_id: u64,
}

struct Inner {
    state: Mutex<State>,
    connections: Vec<ExchangeConnection>,
    rest_sources: Vec<QuoteSource>,
    poll_every: Duration,
}

/// Live price hub shared by every visitor. Cheap to clone (one shared state).
#[derive(Clone)]
pub struct LiveHub {
    inner: Arc<Inner>,
}

/// Ticks of the keys given to `LiveHub::subscribe`, as a `Stream`. Dropping it unsubscribes.
pub struct Subscription {
    hub: Weak<Inner>,
    id: u64,
    keys: Vec<String>,
    rx: mpsc::Receiver<Tick>,
}

impl Subscription {
    pub async fn recv(&mut self) -> Option<Tick> {
        self.rx.recv().await
    }
}

impl Stream for Subscription {
    type Item = Tick;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Tick>> {
        self.rx.poll_recv(cx)
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(inner) = self.hub.upgrade() {
            inner.unsubscribe(self.id, &self.keys);
        }
    }
}

impl Default for LiveHub {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveHub {
    /// The 7 exchange feeds, the crypto REST sources and the fast stock ones, polled every 5 s.
    pub fn new() -> Self {
        let rest = QUOTE_SOURCES.iter().filter(|s| s.kind == Kind::Crypto || FAST_STOCK.contains(&s.name.as_str())).cloned().collect();
        Self::with(FEEDS.clone(), rest, Duration::from_millis(5_000))
    }

    pub fn with(feeds: Vec<FeedSpec>, rest_sources: Vec<QuoteSource>, poll_every: Duration) -> Self {
        let inner = Arc::new_cyclic(|weak: &Weak<Inner>| {
            let connections = feeds
                .into_iter()
                .map(|spec| {
                    let hub = weak.clone();
                    let on_quote: OnQuote = Arc::new(move |exchange: &str, p: Parsed| {
                        if let Some(inner) = hub.upgrade() {
                            Inner::on_quote(&inner, &format!("crypto:{}", p.base), exchange, p.price, p.change, now_ms());
                        }
                    });
                    ExchangeConnection { spec, tx: Mutex::new(None), on_quote }
                })
                .collect();
            Inner { state: Mutex::new(State::default()), connections, rest_sources, poll_every }
        });
        LiveHub { inner }
    }

    /// Last known tick of each key (sent right away to a new subscriber).
    pub fn snapshot(&self, keys: &[String]) -> Vec<Tick> {
        let st = self.inner.state.lock().unwrap();
        keys.iter().filter_map(|k| st.last.get(k).cloned()).collect()
    }

    /// Starts watching `keys` ("crypto:BTC", "stock:AAPL"): WebSocket subscriptions, an immediate REST poll for the
    /// new keys, polling every `poll_every` while someone watches. Must be called inside a Tokio runtime.
    pub fn subscribe(&self, keys: &[String]) -> Subscription {
        let (tx, rx) = mpsc::channel(256);
        let (id, added) = {
            let mut st = self.inner.state.lock().unwrap();
            st.next_id += 1;
            let id = st.next_id;
            let mut added = Vec::new();
            for k in keys {
                if !st.listeners.contains_key(k) {
                    st.listeners.insert(k.clone(), IndexMap::new());
                    added.push(k.clone());
                }
                st.listeners.get_mut(k).unwrap().insert(id, tx.clone());
            }
            (id, added)
        };
        let crypto: Vec<String> = added.iter().filter_map(|k| k.strip_prefix("crypto:").map(String::from)).collect();
        if !crypto.is_empty() {
            self.inner.connections.iter().for_each(|c| c.add(&crypto));
        }
        if !added.is_empty() {
            let inner = self.inner.clone();
            tokio::spawn(async move { Inner::poll(&inner).await });
        }
        let mut st = self.inner.state.lock().unwrap();
        if st.timer.is_none() {
            let weak = Arc::downgrade(&self.inner);
            let every = self.inner.poll_every;
            st.timer = Some(tokio::spawn(async move {
                let mut i = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
                i.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    i.tick().await;
                    let Some(inner) = weak.upgrade() else { return };
                    Inner::poll(&inner).await;
                }
            }));
        }
        Subscription { hub: Arc::downgrade(&self.inner), id, keys: keys.to_vec(), rx }
    }

    /// Number of watched symbols (for monitoring).
    pub fn watched(&self) -> usize {
        self.inner.state.lock().unwrap().listeners.len()
    }

    /// A new price from `source` for `key` (at most 4 ticks per second per key reach the subscribers).
    pub fn on_quote(&self, key: &str, source: &str, price: f64, change: Option<f64>) {
        Inner::on_quote(&self.inner, key, source, price, change, now_ms());
    }

    /// REST polling: every stock, plus the cryptos no WebSocket exchange lists or whose feeds went silent.
    pub async fn poll(&self) {
        Inner::poll(&self.inner).await;
    }

    pub fn close(&self) {
        self.inner.close();
    }
}

impl Inner {
    fn on_quote(this: &Arc<Inner>, key: &str, source: &str, price: f64, change: Option<f64>, now: i64) {
        let mut st = this.state.lock().unwrap();
        st.quotes.entry(key.to_string()).or_default().insert(source.to_string(), LiveQuote { price, change, time: now });
        Self::schedule(this, &mut st, key);
    }

    /// At most 4 ticks per second per symbol: the latest state is always the one sent.
    fn schedule(this: &Arc<Inner>, st: &mut State, key: &str) {
        if st.pending.contains_key(key) {
            return;
        }
        let wait = (250 - (now_ms() - st.last_emit.get(key).copied().unwrap_or(0))).max(0);
        let weak = Arc::downgrade(this);
        let k = key.to_string();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(wait as u64)).await;
            if let Some(inner) = weak.upgrade() {
                let mut st = inner.state.lock().unwrap();
                st.pending.remove(&k);
                Self::emit(&mut st, &k);
            }
        });
        st.pending.insert(key.to_string(), handle);
    }

    fn emit(st: &mut State, key: &str) {
        let stock = key.starts_with("stock:");
        let now = now_ms();
        let Some(c) = st.quotes.get(key).and_then(|q| live_consensus(q, now, if stock { 3_600_000 } else { 120_000 }, if stock { 1.5 } else { 1.0 })) else {
            return;
        };
        let mut parts = key.split(':');
        let (Some(kind), Some(symbol)) = (parts.next().and_then(Kind::parse), parts.next()) else { return };
        let market = st.market.get(key).copied();
        let tick = Tick { symbol: symbol.into(), kind, price: c.price, change: c.change, agreeing: c.agreeing, total: c.total, sources: c.sources, time: c.time, market };
        let prev = st.last.insert(key.to_string(), tick.clone());
        st.last_emit.insert(key.to_string(), now);
        if let Some(p) = prev {
            if p.price == tick.price && p.agreeing == tick.agreeing && p.total == tick.total && p.market == tick.market {
                return;
            }
        }
        if let Some(ls) = st.listeners.get(key) {
            for tx in ls.values() {
                // A client too slow to read 256 ticks loses the oldest ones rather than growing memory.
                let _ = tx.try_send(tick.clone());
            }
        }
    }

    fn unsubscribe(&self, id: u64, keys: &[String]) {
        let mut st = self.state.lock().unwrap();
        let mut removed = Vec::new();
        for k in keys {
            if let Some(set) = st.listeners.get_mut(k) {
                set.shift_remove(&id);
                if set.is_empty() {
                    st.listeners.shift_remove(k);
                    removed.push(k.clone());
                }
            }
        }
        let empty = st.listeners.is_empty();
        if empty {
            if let Some(t) = st.timer.take() {
                t.abort();
            }
        }
        drop(st);
        let gone: Vec<String> = removed.iter().filter_map(|k| k.strip_prefix("crypto:").map(String::from)).collect();
        if !gone.is_empty() {
            self.connections.iter().for_each(|c| c.remove(&gone));
        }
    }

    async fn poll(this: &Arc<Inner>) {
        let assets: Vec<Asset> = {
            let mut st = this.state.lock().unwrap();
            if st.polling {
                return;
            }
            let now = now_ms();
            let ws_names: Vec<&str> = FEEDS.iter().map(|f| f.name).collect();
            let assets: Vec<Asset> = st
                .listeners
                .keys()
                .filter_map(|k| {
                    let mut parts = k.split(':');
                    let kind = parts.next().and_then(Kind::parse)?;
                    let symbol = parts.next().unwrap_or("").to_string();
                    if kind == Kind::Crypto {
                        let live_ws = st.quotes.get(k).is_some_and(|q| q.iter().any(|(n, x)| ws_names.contains(&n.as_str()) && now - x.time < WS_FRESH));
                        if live_ws {
                            return None;
                        }
                    }
                    Some(Asset { name: symbol.clone(), symbol, kind, gecko: None })
                })
                .collect();
            if assets.is_empty() {
                return;
            }
            st.polling = true;
            assets
        };
        struct Reset<'a>(&'a Inner);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.state.lock().unwrap().polling = false;
            }
        }
        let _reset = Reset(this);
        let results = join_all(this.rest_sources.iter().filter(|s| assets.iter().any(|a| a.kind == s.kind)).map(|s| {
            let fut = (s.fetch)(assets.iter().filter(|a| a.kind == s.kind).cloned().collect());
            let name = s.name.clone();
            async move { (name, fut.await.ok()) }
        }))
        .await;
        let t = now_ms();
        let market = if us_market_open(t) { Market::Open } else { Market::Closed };
        let mut st = this.state.lock().unwrap();
        for (name, quotes) in &results {
            let source_kind = this.rest_sources.iter().find(|s| &s.name == name).map(|s| s.kind);
            for (sym, q) in quotes.iter().flatten() {
                let kind = assets.iter().find(|a| &a.symbol == sym && source_kind == Some(a.kind)).map(|a| a.kind);
                // Same name as a WebSocket feed: one exchange, one vote (the most recent quote wins).
                if let Some(kind) = kind {
                    st.quotes.entry(format!("{}:{sym}", kind.as_str())).or_default().insert(name.clone(), LiveQuote { price: q.price, change: q.change, time: t });
                }
            }
        }
        for a in &assets {
            let key = format!("{}:{}", a.kind.as_str(), a.symbol);
            if a.kind == Kind::Stock {
                st.market.insert(key.clone(), market);
            }
            Self::schedule(this, &mut st, &key);
        }
    }

    fn close(&self) {
        self.connections.iter().for_each(|c| c.close());
        let mut st = self.state.lock().unwrap();
        if let Some(t) = st.timer.take() {
            t.abort();
        }
        for (_, t) in st.pending.drain() {
            t.abort();
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.connections.iter().for_each(|c| c.close());
        if let Ok(mut st) = self.state.lock() {
            if let Some(t) = st.timer.take() {
                t.abort();
            }
        }
    }
}
