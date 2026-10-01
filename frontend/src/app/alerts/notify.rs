//! Checks of the Alertes screen while an Altim tab is open (notify.ts): every 5 minutes, and 5 s after start, the
//! price alerts (consensus quotes), the buy alerts of the radar and holdings (/api/alerts, same rule as the phones)
//! and the important news. New ones are shown with the browser's Notification API when allowed, and written to the
//! alerts journal. The browser cannot run this when every Altim tab is closed (no push server). The Notification API
//! is reached through `js_sys::Reflect` (no extra web-sys feature).
use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use altim_core::web::insights::alerts::{
    CHECK_MS, Notice, add_to_journal, buy_entries, buy_notices, evaluate_targets, merge_targets, new_buy_alerts, new_news, news_notices,
    target_fired, uniq,
};
use altim_core::web::store::asset_key;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use yew_router::history::{BrowserHistory, History};

use super::store::{alerts_state, buy_alerts, my_assets, news_items, quotes, set_alerts};

fn notification_ctor() -> Option<js_sys::Function> {
    js_sys::Reflect::get(&js_sys::global(), &"Notification".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok())
}

pub fn notifications_supported() -> bool {
    notification_ctor().is_some()
}

/// "granted", "denied", "default" or "unsupported".
pub fn permission() -> String {
    match notification_ctor() {
        None => "unsupported".into(),
        Some(n) => js_sys::Reflect::get(&n, &"permission".into()).ok().and_then(|p| p.as_string()).unwrap_or_else(|| "default".into()),
    }
}

/// Asks the browser's permission (only on a click: browsers refuse it otherwise).
pub async fn ask_permission() -> String {
    let Some(n) = notification_ctor() else { return "unsupported".into() };
    let ask = js_sys::Reflect::get(&n, &"requestPermission".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    if let Some(p) = ask.and_then(|f| f.call0(&n).ok()).and_then(|p| p.dyn_into::<js_sys::Promise>().ok()) {
        if let Some(s) = JsFuture::from(p).await.ok().and_then(|v| v.as_string()) {
            return s;
        }
    }
    permission()
}

/// Shows a notification when allowed; a click focuses the tab and opens its page.
fn show(n: &Notice) {
    if permission() != "granted" {
        return;
    }
    let Some(ctor) = notification_ctor() else { return };
    let opts = js_sys::Object::new();
    for (k, v) in [("body", n.body.as_str()), ("tag", n.tag.as_str()), ("icon", "/logo.svg")] {
        let _ = js_sys::Reflect::set(&opts, &k.into(), &v.into());
    }
    let args = js_sys::Array::of2(&n.title.as_str().into(), &opts);
    // Some mobile browsers only allow notifications from a service worker: the journal still keeps them.
    let Ok(note) = js_sys::Reflect::construct(&ctor, &args) else { return };
    let open = n.open.clone();
    let target = note.clone();
    let click = Closure::once_into_js(move || {
        if let Some(w) = web_sys::window() {
            let _ = w.focus();
            if let Some(to) = open {
                if crate::route::pathname() != to {
                    BrowserHistory::new().push(to);
                }
                w.scroll_to_with_x_and_y(0.0, 0.0);
            }
        }
        if let Some(close) = js_sys::Reflect::get(&target, &"close".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()) {
            let _ = close.call0(&target);
        }
    });
    let _ = js_sys::Reflect::set(&note, &"onclick".into(), &click);
}

thread_local! {
    static RUNNING: Cell<bool> = const { Cell::new(false) };
    static STARTED: Cell<bool> = const { Cell::new(false) };
}

/// One check; returns an error text when the server could not be reached (the next check retries).
pub async fn run_check(now: f64) -> Option<String> {
    if RUNNING.with(|r| r.replace(true)) {
        return None;
    }
    let r = check(now).await;
    RUNNING.with(|r| r.set(false));
    r.err()
}

async fn check(now: f64) -> Result<(), String> {
    let s = alerts_state();
    let mine = my_assets();
    let mut entries = Vec::new();
    let mut tracker = s.tracker.clone();
    let mut targets = s.targets.clone();
    let mut news_seen = s.news_seen.clone();
    let armed: Vec<(String, altim_core::types::Kind)> =
        uniq(&s.targets.iter().filter(|t| t.triggered.is_none()).map(|t| (t.symbol.clone(), t.kind)).collect::<Vec<_>>(), |(s, k)| asset_key(s, *k));
    if !armed.is_empty() {
        let q = quotes(&armed).await.map_err(|e| e.0)?;
        let prices: HashMap<String, f64> = q.iter().map(|x| (asset_key(&x.symbol, x.kind), x.price)).collect();
        let (t, fired) = evaluate_targets(&s.targets, &prices, now, &crate::money::display());
        targets = t;
        for (t, price) in fired {
            let (e, n) = target_fired(&t, price, now, &crate::money::price(price));
            entries.push(e);
            show(&n);
        }
    }
    if s.notify.buy && !mine.is_empty() {
        let items = buy_alerts(&mine).await.map_err(|e| e.0)?;
        let (t, fresh) = new_buy_alerts(&s.tracker, &items, s.notify.strong_only, now);
        tracker = t;
        entries.extend(buy_entries(&fresh, now));
        buy_notices(&fresh).iter().for_each(show);
    }
    if s.notify.news {
        // A feed that fails does not stop the buy and price alerts.
        if let Ok(report) = news_items(&mine).await {
            let owned: HashSet<String> = mine.iter().map(|(s, k)| asset_key(s, *k)).collect();
            let (seen, fresh) = new_news(&s.news_seen, &report.items, &owned, now);
            news_seen = seen;
            news_notices(&fresh).iter().for_each(show);
        }
    }
    set_alerts(move |cur| {
        cur.tracker = tracker;
        cur.targets = merge_targets(&cur.targets, &targets);
        cur.news_seen = news_seen;
        if !entries.is_empty() {
            cur.journal = add_to_journal(&entries, &cur.journal);
        }
        cur.last_check = Some(now);
    });
    Ok(())
}

/// Starts the checks once per tab (they only query the server when something is to check).
pub fn start_checks() {
    if STARTED.with(|s| s.replace(true)) {
        return;
    }
    let tick = || {
        // This .wasm handed the page over to the whole app, which runs its own checks.
        if crate::part::stopped() {
            return;
        }
        let s = alerts_state();
        if s.notify.buy || s.notify.news || s.targets.iter().any(|t| t.triggered.is_none()) {
            wasm_bindgen_futures::spawn_local(async {
                run_check(js_sys::Date::now()).await;
            });
        }
    };
    gloo::timers::callback::Timeout::new(5_000, tick).forget();
    gloo::timers::callback::Interval::new(CHECK_MS, tick).forget();
}
