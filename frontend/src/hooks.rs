//! Small browser hooks shared by the site and the app (hooks.ts): reveal on scroll, intervals, the site's prices.
use std::rc::Rc;

use altim_core::web::market::Tick;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use yew::prelude::*;

/// Adds the class `visible` when the element enters the screen (15 % visible), then stops watching.
#[hook]
pub fn use_reveal() -> NodeRef {
    let node = use_node_ref();
    {
        let node = node.clone();
        use_effect_with((), move |_| {
            let mut keep: Option<(web_sys::IntersectionObserver, Closure<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>)> = None;
            if let Some(el) = node.cast::<web_sys::Element>() {
                let target = el.clone();
                let cb = Closure::<dyn FnMut(js_sys::Array, web_sys::IntersectionObserver)>::new(
                    move |entries: js_sys::Array, obs: web_sys::IntersectionObserver| {
                        let hit = entries.get(0).dyn_into::<web_sys::IntersectionObserverEntry>().map(|e| e.is_intersecting()).unwrap_or(false);
                        if hit {
                            let _ = target.class_list().add_1("visible");
                            obs.disconnect();
                        }
                    },
                );
                let opts = web_sys::IntersectionObserverInit::new();
                opts.set_threshold(&JsValue::from_f64(0.15));
                match web_sys::IntersectionObserver::new_with_options(cb.as_ref().unchecked_ref(), &opts) {
                    Ok(obs) => {
                        obs.observe(&el);
                        keep = Some((obs, cb));
                    }
                    // No IntersectionObserver: shown at once.
                    Err(_) => {
                        let _ = el.class_list().add_1("visible");
                    }
                }
            }
            move || {
                if let Some((obs, _cb)) = keep {
                    obs.disconnect();
                }
            }
        });
    }
    node
}

/// Calls `f` every `ms` milliseconds while the component is shown (not at once); the latest `f` is used.
#[hook]
pub fn use_interval(ms: u32, f: impl Fn() + 'static) {
    let latest = use_mut_ref(|| None::<Box<dyn Fn()>>);
    *latest.borrow_mut() = Some(Box::new(f));
    use_effect_with(ms, move |ms| {
        let timer = gloo::timers::callback::Interval::new(*ms, move || {
            if let Some(f) = latest.borrow().as_ref() {
                f();
            }
        });
        move || drop(timer)
    });
}

/// Site prices (`/api/tickers`, else the public fallbacks), refreshed every 15 s.
#[hook]
pub fn use_ticks() -> Rc<Vec<Tick>> {
    let ticks = use_state(|| Rc::new(Vec::new()));
    {
        let ticks = ticks.clone();
        use_effect_with((), move |_| {
            let alive = Rc::new(std::cell::Cell::new(true));
            let load = {
                let (ticks, alive) = (ticks.clone(), alive.clone());
                move || {
                    let (ticks, alive) = (ticks.clone(), alive.clone());
                    wasm_bindgen_futures::spawn_local(async move {
                        let t = crate::api::tickers().await;
                        if alive.get() {
                            ticks.set(Rc::new(t));
                        }
                    });
                }
            };
            load();
            let timer = gloo::timers::callback::Interval::new(15_000, load);
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }
    (*ticks).clone()
}

/// `document.visibilityState === "visible"`: the periodic refreshes skip a hidden tab.
pub fn visible() -> bool {
    gloo::utils::document().visibility_state() == web_sys::VisibilityState::Visible
}

/// Reloads every `ms` while the tab is visible (`setInterval(() => document.visibilityState === "visible" && load())`);
/// the first load is the caller's. Dropping the returned timer stops it.
pub fn every_visible(ms: u32, load: impl Fn() + 'static) -> gloo::timers::callback::Interval {
    gloo::timers::callback::Interval::new(ms, move || {
        if visible() {
            load();
        }
    })
}

/// Sets the page title while the component is shown.
pub fn set_title(t: &str) {
    gloo::utils::document().set_title(t);
}
