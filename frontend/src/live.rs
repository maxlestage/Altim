//! Live data of a screen: one WebSocket (`/api/ws`, see `altim_core::web::live_ws`) carrying the prices, the « can I
//! buy now? » alerts, the Radar verdicts and the EUR/USD rate, reconnected with a growing pause, closed while the tab
//! is hidden and reopened when it comes back. After two failed openings in a row (a proxy without WebSocket), the
//! prices come from the Server-Sent Events of `/api/live` instead (one stream per 20 assets), as before.
//!
//! The badge says « en direct » only while messages arrive: the client pings every 20 s and the server answers, so
//! 45 s of silence means a dead socket, closed and reopened.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use altim_core::types::Kind;
use altim_core::web::live_ws::{self, AlertsPush, VerdictPush, WsMsg};
use altim_core::web::sorting::Sorting;
use serde::Deserialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use yew::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LiveTick {
    pub symbol: String,
    pub kind: Kind,
    pub price: f64,
    #[serde(default)]
    pub change: Option<f64>,
    #[serde(default)]
    pub agreeing: u32,
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub time: f64,
    /// "open" / "closed" (stocks).
    #[serde(default)]
    pub market: Option<String>,
    /// Direction of the last move, for the flash.
    #[serde(skip)]
    pub dir: Option<Dir>,
    /// Reception counter (the flash animation restarts on it).
    #[serde(skip)]
    pub seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveStatus {
    Connecting,
    Live,
    /// The connection dropped or went silent: a new one is being opened.
    Reconnecting,
    Offline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Live {
    /// By "crypto:BTC".
    pub ticks: Rc<HashMap<String, LiveTick>>,
    pub status: LiveStatus,
    pub last: Option<f64>,
    /// Alerts of the watched assets pushed by the server (None until the first one, or over the SSE fallback).
    pub alerts: Option<Rc<AlertsPush>>,
    /// Radar verdicts pushed by the server, by "crypto:BTC".
    pub verdicts: Rc<HashMap<String, VerdictPush>>,
}

impl Live {
    pub fn get(&self, symbol: &str, kind: Kind) -> Option<&LiveTick> {
        self.ticks.get(&altim_core::web::store::asset_key(symbol, kind))
    }

    /// The pushed alerts as typed rows (`/api/alerts` items), the ones in error left out.
    pub fn alert_rows<T: serde::de::DeserializeOwned>(&self) -> Option<Vec<T>> {
        self.alerts
            .as_ref()
            .map(|a| a.items.iter().filter(|i| i.get("error").is_none()).filter_map(|i| serde_json::from_value(i.clone()).ok()).collect())
    }
}

thread_local! {
    static SEQ: Cell<u64> = const { Cell::new(0) };
}

type Handler = Closure<dyn FnMut(web_sys::Event)>;

/// Frees `t` after the current callback (a closure must not be freed while it runs).
fn later<T: 'static>(t: T) {
    wasm_bindgen_futures::spawn_local(async move { drop(t) });
}

/// What the connection updates.
#[derive(Clone)]
struct Out {
    ticks: UseStateHandle<Rc<HashMap<String, LiveTick>>>,
    current: Rc<RefCell<Rc<HashMap<String, LiveTick>>>>,
    status: UseStateHandle<LiveStatus>,
    last: UseStateHandle<Option<f64>>,
    alerts: UseStateHandle<Option<Rc<AlertsPush>>>,
    verdicts: UseStateHandle<Rc<HashMap<String, VerdictPush>>>,
    current_verdicts: Rc<RefCell<Rc<HashMap<String, VerdictPush>>>>,
}

impl Out {
    fn tick(&self, mut t: LiveTick) {
        if !t.price.is_finite() {
            return;
        }
        self.last.set(Some(js_sys::Date::now()));
        let mut map = (**self.current.borrow()).clone();
        let k = altim_core::web::store::asset_key(&t.symbol, t.kind);
        let prev = map.get(&k);
        let before = prev.map(|p| p.price);
        t.dir = match before {
            None => prev.and_then(|p| p.dir),
            Some(b) if b == t.price => prev.and_then(|p| p.dir),
            Some(b) => Some(if t.price > b { Dir::Up } else { Dir::Down }),
        };
        t.seq = match (before, prev) {
            (Some(b), Some(p)) if b == t.price => p.seq,
            _ => SEQ.with(|s| {
                s.set(s.get() + 1);
                s.get()
            }),
        };
        map.insert(k, t);
        let map = Rc::new(map);
        *self.current.borrow_mut() = map.clone();
        self.ticks.set(map);
    }

    fn verdict(&self, v: VerdictPush) {
        let mut map = (**self.current_verdicts.borrow()).clone();
        map.insert(altim_core::web::store::asset_key(&v.symbol, v.kind), v);
        let map = Rc::new(map);
        *self.current_verdicts.borrow_mut() = map.clone();
        self.verdicts.set(map);
    }
}

#[derive(Default)]
struct Conn {
    ws: Option<(web_sys::WebSocket, Vec<Handler>)>,
    sse: Vec<(web_sys::EventSource, Vec<Handler>)>,
    /// The current socket reached `open`.
    opened: bool,
    /// Openings that failed in a row (fallback to SSE at `FALLBACK_AFTER`).
    failures: u32,
    /// Reconnections since the last good connection (pace of the retries).
    attempt: u32,
    sse_mode: bool,
    /// The tab is visible and the screen still shown.
    running: bool,
    was_live: bool,
    last_message: Option<f64>,
    retry: Option<gloo::timers::callback::Timeout>,
    timer: Option<gloo::timers::callback::Interval>,
}

struct Driver {
    keys: Vec<String>,
    interval: &'static str,
    usd: bool,
    out: Out,
    conn: RefCell<Conn>,
}

impl Driver {
    fn start(self: &Rc<Self>) {
        self.conn.borrow_mut().running = true;
        if self.conn.borrow().sse_mode { self.open_sse() } else { self.open_ws() }
    }

    fn stop(&self) {
        let mut c = self.conn.borrow_mut();
        c.running = false;
        c.retry = None;
        later(c.timer.take());
        Self::close_ws(&mut c);
        for (es, _) in c.sse.drain(..) {
            es.close();
        }
    }

    fn close_ws(c: &mut Conn) {
        if let Some((ws, handlers)) = c.ws.take() {
            ws.set_onopen(None);
            ws.set_onmessage(None);
            ws.set_onclose(None);
            ws.set_onerror(None);
            let _ = ws.close();
            // This may run inside one of these handlers: they are freed once it returned.
            later(handlers);
        }
        c.opened = false;
    }

    fn open_ws(self: &Rc<Self>) {
        let supported = js_sys::Reflect::has(&js_sys::global(), &"WebSocket".into()).unwrap_or(false);
        let loc = gloo::utils::window().location();
        let url = live_ws::ws_url(&loc.protocol().unwrap_or_default(), &loc.host().unwrap_or_default());
        let ws = match supported.then(|| web_sys::WebSocket::new(&url)) {
            Some(Ok(ws)) => ws,
            _ => {
                self.conn.borrow_mut().sse_mode = true;
                return self.open_sse();
            }
        };
        let was_live = self.conn.borrow().was_live;
        self.out.status.set(if was_live { LiveStatus::Reconnecting } else { LiveStatus::Connecting });
        let weak = Rc::downgrade(self);
        let on = |f: fn(&Rc<Driver>, web_sys::Event)| {
            let weak: Weak<Driver> = weak.clone();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                if let Some(d) = weak.upgrade() {
                    f(&d, e)
                }
            })
        };
        let handlers = vec![
            on(|d, _| d.on_open()),
            on(|d, e| {
                if let Some(text) = e.dyn_ref::<web_sys::MessageEvent>().and_then(|m| m.data().as_string()) {
                    d.on_message(&text);
                }
            }),
            on(|d, _| d.on_close()),
        ];
        ws.set_onopen(Some(handlers[0].as_ref().unchecked_ref()));
        ws.set_onmessage(Some(handlers[1].as_ref().unchecked_ref()));
        ws.set_onclose(Some(handlers[2].as_ref().unchecked_ref()));
        let mut c = self.conn.borrow_mut();
        Self::close_ws(&mut c);
        c.ws = Some((ws, handlers));
        c.last_message = None;
        // Every 5 s: an application ping every 20 s, and the silence check.
        let weak = Rc::downgrade(self);
        let beats = Cell::new(0u32);
        c.timer = Some(gloo::timers::callback::Interval::new(5_000, move || {
            let Some(d) = weak.upgrade() else { return };
            beats.set(beats.get() + 1);
            let (open, last) = {
                let c = d.conn.borrow();
                (c.opened, c.last_message)
            };
            if !open {
                return;
            }
            if !live_ws::is_live(true, last, js_sys::Date::now()) {
                d.on_close();
                return;
            }
            if beats.get().is_multiple_of(live_ws::PING_MS / 5_000) {
                if let Some((ws, _)) = d.conn.borrow().ws.as_ref() {
                    let _ = ws.send_with_str(r#"{"type":"ping"}"#);
                }
            }
        }));
    }

    fn on_open(&self) {
        let c = self.conn.borrow();
        let Some((ws, _)) = c.ws.as_ref() else { return };
        let _ = ws.send_with_str(&live_ws::subscribe_message(&self.keys, self.interval, self.usd));
        drop(c);
        let mut c = self.conn.borrow_mut();
        c.opened = true;
        c.failures = 0;
    }

    fn on_message(&self, text: &str) {
        let Some(m) = live_ws::parse_ws(text) else { return };
        {
            let mut c = self.conn.borrow_mut();
            c.last_message = Some(js_sys::Date::now());
            c.attempt = 0;
            c.was_live = true;
        }
        self.out.status.set(LiveStatus::Live);
        match m {
            WsMsg::Tick(v) => {
                if let Ok(t) = serde_json::from_value::<LiveTick>(v) {
                    self.out.tick(t);
                }
            }
            WsMsg::Alerts(a) => self.out.alerts.set(Some(Rc::new(a))),
            WsMsg::Verdict(v) => self.out.verdict(v),
            WsMsg::Fx { rate, .. } => {
                // A new rate: the app's own copy is re-read (stored, validated, every amount converted again).
                if crate::state::fx::fx_state().fx.as_ref().is_none_or(|f| f.rate != rate) {
                    wasm_bindgen_futures::spawn_local(crate::state::fx::refresh_fx());
                }
            }
            WsMsg::Hello { .. } | WsMsg::Checked { .. } | WsMsg::Pong | WsMsg::Error { .. } | WsMsg::Other => {}
        }
    }

    /// The socket closed (or went silent): retry after a growing pause, or fall back to SSE.
    fn on_close(self: &Rc<Self>) {
        let mut c = self.conn.borrow_mut();
        if !c.opened {
            c.failures += 1;
        }
        Self::close_ws(&mut c);
        later(c.timer.take());
        if !c.running {
            return;
        }
        if c.failures >= live_ws::FALLBACK_AFTER {
            c.sse_mode = true;
            drop(c);
            return self.open_sse();
        }
        let wait = live_ws::backoff_ms(c.attempt);
        c.attempt += 1;
        self.out.status.set(if c.attempt >= 3 {
            LiveStatus::Offline
        } else if c.was_live {
            LiveStatus::Reconnecting
        } else {
            LiveStatus::Connecting
        });
        let weak = Rc::downgrade(self);
        c.retry = Some(gloo::timers::callback::Timeout::new(wait, move || {
            if let Some(d) = weak.upgrade() {
                if d.conn.borrow().running {
                    d.open_ws();
                }
            }
        }));
    }

    /// Prices only, from `/api/live` (the alerts keep their own refresh on the screens).
    fn open_sse(self: &Rc<Self>) {
        let supported = js_sys::Reflect::has(&js_sys::global(), &"EventSource".into()).unwrap_or(false);
        if !supported {
            self.out.status.set(LiveStatus::Offline);
            return;
        }
        for (es, _) in self.conn.borrow_mut().sse.drain(..) {
            es.close();
        }
        self.out.status.set(LiveStatus::Connecting);
        for c in self.keys.chunks(live_ws::MAX_ASSETS) {
            let symbols: Vec<String> = c
                .iter()
                .map(|k| {
                    let (kind, sym) = k.split_once(':').unwrap_or(("", k));
                    format!("{sym}:{kind}")
                })
                .collect();
            let url = format!("/api/live?symbols={}", crate::api::enc(&symbols.join(",")));
            let Ok(es) = web_sys::EventSource::new(&url) else { continue };
            let on_open = {
                let status = self.out.status.clone();
                Closure::<dyn FnMut(web_sys::Event)>::new(move |_| status.set(LiveStatus::Live))
            };
            let on_error = {
                let (status, es2) = (self.out.status.clone(), es.clone());
                Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                    status.set(if es2.ready_state() == web_sys::EventSource::CLOSED { LiveStatus::Offline } else { LiveStatus::Reconnecting })
                })
            };
            let on_message = {
                let out = self.out.clone();
                Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                    let Some(data) = e.dyn_ref::<web_sys::MessageEvent>().and_then(|m| m.data().as_string()) else { return };
                    if let Ok(t) = serde_json::from_str::<LiveTick>(&data) {
                        out.status.set(LiveStatus::Live);
                        out.tick(t);
                    }
                })
            };
            es.set_onopen(Some(on_open.as_ref().unchecked_ref()));
            es.set_onerror(Some(on_error.as_ref().unchecked_ref()));
            es.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            self.conn.borrow_mut().sse.push((es, vec![on_open, on_error, on_message]));
        }
    }
}

/// Live data of these assets (a stable set: the same assets keep the same connection whatever the order).
#[hook]
pub fn use_live(items: Vec<(String, Kind)>) -> Live {
    let ticks = use_state(|| Rc::new(HashMap::<String, LiveTick>::new()));
    let status = use_state(|| LiveStatus::Connecting);
    let last = use_state(|| None::<f64>);
    let alerts = use_state(|| None::<Rc<AlertsPush>>);
    let verdicts = use_state(|| Rc::new(HashMap::<String, VerdictPush>::new()));
    let interval = crate::state::app::use_app_state().interval.as_str();
    let usd = crate::money::currency_is_usd();
    let mut keys: Vec<String> = items.iter().map(|(s, k)| altim_core::web::store::asset_key(s, *k)).collect();
    keys.sort_dyn();
    keys.dedup();
    let list = keys.join(",");
    // The latest values, read by the event handlers (a state handle holds the value of its render).
    let current = use_mut_ref(|| Rc::new(HashMap::<String, LiveTick>::new()));
    *current.borrow_mut() = (*ticks).clone();
    let current_verdicts = use_mut_ref(|| Rc::new(HashMap::<String, VerdictPush>::new()));
    *current_verdicts.borrow_mut() = (*verdicts).clone();
    {
        let out = Out {
            ticks: ticks.clone(),
            current,
            status: status.clone(),
            last: last.clone(),
            alerts: alerts.clone(),
            verdicts: verdicts.clone(),
            current_verdicts,
        };
        use_effect_with((list, interval, usd), move |(list, interval, usd)| {
            // A new list: what was pushed for the previous one no longer applies.
            out.alerts.set(None);
            let mut visibility: Option<gloo::events::EventListener> = None;
            let driver = (!list.is_empty()).then(|| {
                Rc::new(Driver { keys: list.split(',').map(String::from).collect(), interval, usd: *usd, out, conn: RefCell::new(Conn::default()) })
            });
            if let Some(d) = &driver {
                if crate::hooks::visible() {
                    d.start();
                }
                let weak = Rc::downgrade(d);
                visibility = Some(gloo::events::EventListener::new(&gloo::utils::document(), "visibilitychange", move |_| {
                    let Some(d) = weak.upgrade() else { return };
                    if crate::hooks::visible() {
                        d.conn.borrow_mut().attempt = 0;
                        d.start();
                    } else {
                        d.stop();
                    }
                }));
            }
            move || {
                drop(visibility);
                if let Some(d) = driver {
                    d.stop();
                }
            }
        });
    }
    Live { ticks: (*ticks).clone(), status: *status, last: *last, alerts: (*alerts).clone(), verdicts: (*verdicts).clone() }
}

#[derive(Properties, PartialEq)]
pub struct LiveBadgeProps {
    pub status: LiveStatus,
    pub last: Option<f64>,
}

/// "EN DIRECT" pill: pulsing dot while ticks arrive, time of the last one.
#[component]
pub fn LiveBadge(p: &LiveBadgeProps) -> Html {
    let tick = use_force_update();
    crate::hooks::use_interval(1000, move || tick.force_update());
    let (label, cls) = match p.status {
        LiveStatus::Live => ("EN DIRECT", "live"),
        LiveStatus::Connecting => ("CONNEXION…", "connecting"),
        LiveStatus::Reconnecting => ("RECONNEXION…", "connecting"),
        LiveStatus::Offline => ("HORS LIGNE", "offline"),
    };
    html! {
        <span class={classes!("live-badge", cls)} role="status" aria-live="off">
            <i aria-hidden="true" />{ " " }{ label }
            if let (Some(t), LiveStatus::Live) = (p.last, p.status) {
                <small>{ format!(" · {}", crate::ui::fr_time_seconds(t)) }</small>
            }
        </span>
    }
}

#[derive(Properties, PartialEq)]
pub struct LivePriceProps {
    pub tick: Option<LiveTick>,
    pub fallback: Option<f64>,
    /// Formats the value (e.g. `crate::money::price`).
    pub format: Callback<f64, String>,
}

/// Price that flashes cyan / red on each move, its changed digits rolling in from below (up) or above (down); the key
/// restarts the animations. Tabular figures: nothing moves sideways.
#[component]
pub fn LivePrice(p: &LivePriceProps) -> Html {
    let _m = crate::money::use_money();
    // The text shown before this tick (per tick sequence), for the digits that roll.
    let shown = use_mut_ref(|| (0u64, None::<String>, None::<String>));
    let v = p.tick.as_ref().map(|t| t.price).or(p.fallback).filter(|v| v.is_finite());
    let Some(v) = v else { return html! { { "—" } } };
    let dir = p.tick.as_ref().and_then(|t| t.dir).map(|d| if d == Dir::Up { "flash-up" } else { "flash-down" });
    let seq = p.tick.as_ref().map(|t| t.seq).unwrap_or(0);
    let text = p.format.emit(v);
    let prev = {
        let mut s = shown.borrow_mut();
        if s.0 != seq {
            s.1 = s.2.take();
            s.0 = seq;
        }
        s.2 = Some(text.clone());
        if dir.is_some() { s.1.clone() } else { None }
    };
    let roll = if dir == Some("flash-up") { "roll-up" } else { "roll-down" };
    html! {
        <span key={seq.to_string()} class={classes!("live-price", dir)}>
            { for altim_core::web::motion::digit_runs(prev.as_deref(), &text).into_iter().map(|(run, changed)| if changed {
                html! { <>{ for run.chars().map(|c| html! { <span class={roll}>{ c }</span> }) }</> }
            } else {
                html! { { run } }
            }) }
        </span>
    }
}
