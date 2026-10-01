//! Live prices (Server-Sent Events from /api/live, live.tsx): the page never freezes on a stale price. One stream
//! per 20 assets, closed while the tab is hidden, reopened (with the last prices) when it comes back.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::types::Kind;
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
    Offline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Live {
    /// By "crypto:BTC".
    pub ticks: Rc<HashMap<String, LiveTick>>,
    pub status: LiveStatus,
    pub last: Option<f64>,
}

impl Live {
    pub fn get(&self, symbol: &str, kind: Kind) -> Option<&LiveTick> {
        self.ticks.get(&altim_core::web::store::asset_key(symbol, kind))
    }
}

thread_local! {
    static SEQ: Cell<u64> = const { Cell::new(0) };
}

/// Streams of these assets (a stable set: the same assets keep the same streams whatever the order).
#[hook]
pub fn use_live(items: Vec<(String, Kind)>) -> Live {
    let ticks = use_state(|| Rc::new(HashMap::<String, LiveTick>::new()));
    let status = use_state(|| LiveStatus::Connecting);
    let last = use_state(|| None::<f64>);
    let mut keys: Vec<String> = items.iter().map(|(s, k)| altim_core::web::store::asset_key(s, *k)).collect();
    keys.sort_dyn();
    keys.dedup();
    let list = keys.join(",");
    // The latest ticks, read by the event handlers (the state handle holds the value of its render).
    let current = use_mut_ref(|| Rc::new(HashMap::<String, LiveTick>::new()));
    *current.borrow_mut() = (*ticks).clone();
    {
        let (ticks, status, last) = (ticks.clone(), status.clone(), last.clone());
        use_effect_with(list, move |list| {
            let sources: Rc<RefCell<Vec<(web_sys::EventSource, Vec<Closure<dyn FnMut(web_sys::Event)>>)>>> = Rc::new(RefCell::new(Vec::new()));
            let mut visibility: Option<gloo::events::EventListener> = None;
            let supported = js_sys::Reflect::has(&js_sys::global(), &"EventSource".into()).unwrap_or(false);
            if !list.is_empty() && supported {
                let keys: Vec<String> = list.split(',').map(String::from).collect();
                let chunks: Vec<Vec<String>> = keys.chunks(20).map(|c| c.to_vec()).collect();
                let close = {
                    let sources = sources.clone();
                    Rc::new(move || {
                        for (es, _) in sources.borrow_mut().drain(..) {
                            es.close();
                        }
                    })
                };
                let open = {
                    let (sources, close) = (sources.clone(), close.clone());
                    Rc::new(move || {
                        close();
                        status.set(LiveStatus::Connecting);
                        for c in &chunks {
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
                                let status = status.clone();
                                Closure::<dyn FnMut(web_sys::Event)>::new(move |_| status.set(LiveStatus::Live))
                            };
                            let on_error = {
                                let (status, es2) = (status.clone(), es.clone());
                                Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                                    status.set(if es2.ready_state() == web_sys::EventSource::CLOSED {
                                        LiveStatus::Offline
                                    } else {
                                        LiveStatus::Connecting
                                    })
                                })
                            };
                            let on_message = {
                                let (status, last, ticks, current) = (status.clone(), last.clone(), ticks.clone(), current.clone());
                                Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
                                    let Some(data) = e.dyn_ref::<web_sys::MessageEvent>().and_then(|m| m.data().as_string()) else { return };
                                    let Ok(mut t) = serde_json::from_str::<LiveTick>(&data) else { return };
                                    if !t.price.is_finite() {
                                        return;
                                    }
                                    status.set(LiveStatus::Live);
                                    last.set(Some(js_sys::Date::now()));
                                    let mut map = (**current.borrow()).clone();
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
                                    *current.borrow_mut() = map.clone();
                                    ticks.set(map);
                                })
                            };
                            es.set_onopen(Some(on_open.as_ref().unchecked_ref()));
                            es.set_onerror(Some(on_error.as_ref().unchecked_ref()));
                            es.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
                            sources.borrow_mut().push((es, vec![on_open, on_error, on_message]));
                        }
                    })
                };
                open();
                let (open2, close2) = (open.clone(), close.clone());
                visibility = Some(gloo::events::EventListener::new(&gloo::utils::document(), "visibilitychange", move |_| {
                    if gloo::utils::document().visibility_state() == web_sys::VisibilityState::Visible { open2() } else { close2() }
                }));
            }
            move || {
                drop(visibility);
                for (es, _) in sources.borrow_mut().drain(..) {
                    es.close();
                }
            }
        });
    }
    Live { ticks: (*ticks).clone(), status: *status, last: *last }
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

/// Price that flashes green / red on each move (the key restarts the animation).
#[component]
pub fn LivePrice(p: &LivePriceProps) -> Html {
    let _m = crate::money::use_money();
    let v = p.tick.as_ref().map(|t| t.price).or(p.fallback).filter(|v| v.is_finite());
    let Some(v) = v else { return html! { { "—" } } };
    let dir = p.tick.as_ref().and_then(|t| t.dir).map(|d| if d == Dir::Up { "flash-up" } else { "flash-down" });
    html! { <span key={p.tick.as_ref().map(|t| t.seq).unwrap_or(0).to_string()} class={classes!("live-price", dir)}>{ p.format.emit(v) }</span> }
}
