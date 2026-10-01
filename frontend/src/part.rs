//! A part of the front: what one .wasm shows. scripts/build-web.sh builds one .wasm per part, from the small entry
//! points of `frontend/bundles` (package altim-bundles): the presentation site, each group of app screens of
//! altim_core::web::bundle (the first load of an app address downloads only its group), and the whole app. They all
//! link this one library, compiled once; each entry point names only its own screens, so the link-time optimisation
//! leaves the others out of its .wasm.
//!
//! Once a group's screen is shown, the page's loader fetches and compiles the whole app in the background, and a
//! group's own .wasm when a link to it is about to be followed or when it is a neighbour (`altimFull`); a move to
//! another group's address then hands the page over to it (`hand_over`, `stop`) without reloading, and once the whole
//! app has it, every app screen navigates in place.
use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use altim_core::web::bundle::APP_BUNDLES;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use yew::prelude::*;

use crate::route::{Root, Route};

pub struct Part {
    /// "site" and/or groups of altim_core::web::bundle.
    pub bundles: &'static [&'static str],
    /// The page of an app address (shell and screen); `none` in the site's .wasm.
    pub app: fn(Route) -> Html,
    /// The page of a site address; `none` in the app's .wasm.
    pub site: fn(Route) -> Html,
}

static PART: OnceLock<&'static Part> = OnceLock::new();

thread_local! {
    static APP: RefCell<Option<yew::AppHandle<Root>>> = const { RefCell::new(None) };
    static STOPPED: Cell<bool> = const { Cell::new(false) };
}

/// Starts the front with this part (the `#[wasm_bindgen(start)]` of an entry point). What `#root` holds (the
/// previous part's last picture, see `stop`) is replaced by this part's render.
pub fn run(part: &'static Part) {
    let _ = PART.set(part);
    crate::route::normalize_path();
    crate::money::install_engine_rate();
    crate::state::fx::load_saved();
    let root = gloo::utils::document().get_element_by_id("root").expect("#root");
    root.set_inner_html("");
    let app = yew::Renderer::<Root>::with_root(root).render();
    APP.with(|a| *a.borrow_mut() = Some(app));
}

/// Stops this .wasm before the whole app takes the page over (called by the loader): its screens unmount (their
/// effects clean up: streams, timers) and its own background loops stop (`stopped`).
#[wasm_bindgen(js_name = altimStop)]
pub fn stop() {
    STOPPED.with(|s| s.set(true));
    if let Some(app) = APP.with(|a| a.borrow_mut().take()) {
        app.destroy();
    }
}

/// This .wasm handed the page over: its background loops (alert checks, exchange rate) stop.
pub fn stopped() -> bool {
    STOPPED.with(Cell::get)
}

/// Shows an app address of another group in place, with the whole app or that group's .wasm, when the loader has one
/// of them compiled: `href` is pushed to the history first (None: the address is already the current one). False
/// when neither is ready: the caller loads the page.
pub fn hand_over(href: Option<&str>) -> bool {
    let Some(w) = web_sys::window() else { return false };
    let Some(full) = js_sys::Reflect::get(&w, &"altimFull".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()) else {
        return false;
    };
    // The loader takes the page over with the whole app, or the target group's .wasm when it is compiled (hover,
    // neighbour); false when neither is ready.
    let group = altim_core::web::bundle::bundle_of(href.map(String::from).unwrap_or_else(crate::route::pathname).as_str());
    let ready = js_sys::Reflect::get(&full, &"ready".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call1(&JsValue::NULL, &group.into()).ok())
        .is_some_and(|r| r.as_bool() == Some(true));
    if !ready {
        return false;
    }
    if let Some(href) = href {
        let Ok(h) = w.history() else { return false };
        if h.push_state_with_url(&JsValue::NULL, "", Some(href)).is_err() {
            return false;
        }
    }
    full.call1(&JsValue::NULL, &group.into()).is_ok()
}

/// The part ("site" or an app group) of a same-site address, for the loader's prefetch on hover; "" otherwise.
#[wasm_bindgen(js_name = altimGroupOf)]
pub fn group_of(href: &str) -> String {
    if !href.starts_with('/') || href.starts_with("//") {
        return String::new();
    }
    altim_core::web::bundle::bundle_of(href).to_string()
}

/// Whether this .wasm has the screens of a part of the front ("site" or an app group). Before `run` (tests): all.
pub fn serves(bundle: &str) -> bool {
    match PART.get() {
        Some(p) => p.bundles.contains(&bundle),
        None => bundle == "site" || APP_BUNDLES.contains(&bundle),
    }
}

pub fn app(r: Route) -> Html {
    PART.get().map_or_else(Html::default, |p| (p.app)(r))
}

pub fn site(r: Route) -> Html {
    PART.get().map_or_else(Html::default, |p| (p.site)(r))
}

/// What a part does not show (never reached: `route::switch` hands another part's address over first).
pub fn none(_: Route) -> Html {
    Html::default()
}
