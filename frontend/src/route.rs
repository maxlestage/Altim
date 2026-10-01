//! URLs of the site and the web app (the same as the React front: App.tsx, WebApp.tsx, router.ts). The site's links
//! are plain page loads; inside /app, `use_on_link()` gives the `onclick` of an internal link (history navigation
//! without reloading, like `onLink` of router.ts). A new screen = a variant here + its arm in `app::screen`.
use altim_core::web::bundle::bundle_of;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Debug, Clone, PartialEq, Routable)]
pub enum Route {
    #[at("/")]
    Home,
    #[at("/mentions-legales")]
    MentionsLegales,
    #[at("/confidentialite")]
    Confidentialite,
    #[at("/risques")]
    Risques,
    #[at("/app")]
    Radar,
    #[at("/app/actif/:kind/:symbol")]
    Asset { kind: String, symbol: String },
    #[at("/app/avoirs")]
    Avoirs,
    #[at("/app/selection")]
    Selection,
    #[at("/app/opportunites")]
    Opportunites,
    #[at("/app/reglages")]
    Reglages,
    #[at("/app/actu")]
    Actu,
    #[at("/app/lexique")]
    Lexique,
    #[at("/app/simulation")]
    Simulation,
    #[at("/app/journal")]
    Journal,
    #[at("/app/validation")]
    Validation,
    #[at("/app/bot")]
    Bot,
    #[at("/app/alertes")]
    Alertes,
    #[not_found]
    #[at("/404")]
    NotFound,
}

impl Route {
    pub fn is_app(&self) -> bool {
        !matches!(self, Route::Home | Route::MentionsLegales | Route::Confidentialite | Route::Risques | Route::NotFound)
    }
}

/// "/risques/" → "/risques" before routing (App.tsx strips the trailing slash).
pub fn normalize_path() {
    let Some(w) = web_sys::window() else { return };
    let l = w.location();
    let path = l.pathname().unwrap_or_default();
    if path.len() > 1 && path.ends_with('/') {
        let to = format!("{}{}{}", path.trim_end_matches('/'), l.search().unwrap_or_default(), l.hash().unwrap_or_default());
        if let Ok(h) = w.history() {
            let _ = h.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&to));
        }
    }
}

pub fn pathname() -> String {
    web_sys::window().and_then(|w| w.location().pathname().ok()).unwrap_or_default()
}

#[component]
pub fn Root() -> Html {
    html! {
        <BrowserRouter>
            <Switch<Route> render={switch} />
        </BrowserRouter>
    }
}

/// Whether this .wasm has the screens of a part of the front (altim_core::web::bundle): the site, or a group of the app.
pub fn serves(bundle: &str) -> bool {
    let parts = [
        ("site", cfg!(feature = "site")),
        ("radar", cfg!(feature = "app-radar")),
        ("actif", cfg!(feature = "app-actif")),
        ("avoirs", cfg!(feature = "app-avoirs")),
        ("selection", cfg!(feature = "app-selection")),
        ("actu", cfg!(feature = "app-actu")),
        ("reglages", cfg!(feature = "app-reglages")),
        ("bot", cfg!(feature = "app-bot")),
    ];
    parts.iter().any(|&(b, built)| built && b == bundle)
}

fn switch(r: Route) -> Html {
    if !serves(bundle_of(&pathname())) {
        return other_bundle();
    }
    reloaded();
    if r.is_app() || (r == Route::NotFound && pathname().starts_with("/app/")) { app_screen(r) } else { site_screen(r) }
}

#[cfg(feature = "app-shell")]
fn app_screen(r: Route) -> Html {
    // Any other /app/… address shows the Radar (WebApp.tsx).
    let route = if r == Route::NotFound { Route::Radar } else { r };
    html! { <crate::app::WebApp {route} /> }
}

#[cfg(feature = "site")]
fn site_screen(r: Route) -> Html {
    match r {
        Route::MentionsLegales | Route::Confidentialite | Route::Risques => html! { <crate::site::legal::LegalScreen route={r} /> },
        // Anything else: the home page (App.tsx).
        _ => html! { <crate::site::Home /> },
    }
}

#[cfg(not(feature = "app-shell"))]
fn app_screen(_: Route) -> Html {
    other_bundle()
}

#[cfg(not(feature = "site"))]
fn site_screen(_: Route) -> Html {
    other_bundle()
}

/// The address `other_bundle` last reloaded (session storage).
const RELOAD_KEY: &str = "altim.front.reload";

/// This address belongs to another .wasm (the site and each group of app screens are built apart; links between
/// them are full page loads): load it from the server, once per address so a misrouted page can never loop.
fn other_bundle() -> Html {
    if let Some(w) = web_sys::window() {
        let path = pathname();
        let store = w.session_storage().ok().flatten();
        if store.as_ref().and_then(|s| s.get_item(RELOAD_KEY).ok().flatten()).as_deref() != Some(path.as_str()) {
            if let Some(s) = &store {
                let _ = s.set_item(RELOAD_KEY, &path);
            }
            let _ = w.location().reload();
        } else if let Some(s) = &store {
            let _ = s.remove_item(RELOAD_KEY);
        }
    }
    html! {}
}

/// A screen of this .wasm is shown: a later move to another group's address may reload again.
fn reloaded() {
    let store = web_sys::window().and_then(|w| w.session_storage().ok().flatten());
    if let Some(s) = store.filter(|s| s.get_item(RELOAD_KEY).ok().flatten().is_some()) {
        let _ = s.remove_item(RELOAD_KEY);
    }
}

/// `onclick` of an internal /app link: navigation without reloading (Ctrl/⌘/Shift-click and middle click keep the
/// browser's behaviour), then back to the top of the page.
#[hook]
pub fn use_on_link() -> Callback<MouseEvent> {
    let nav = use_navigator();
    Callback::from(move |e: MouseEvent| {
        if e.meta_key() || e.ctrl_key() || e.shift_key() || e.button() != 0 {
            return;
        }
        let Some(a) = e.current_target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::Element>(t).ok()) else { return };
        let Some(href) = a.get_attribute("href") else { return };
        // Only the screens of this .wasm navigate in place; the site's pages and the other groups of app screens are
        // full page loads (another .wasm).
        let (Some(nav), Some(route)) = (nav.as_ref(), Route::recognize(&href).filter(|r| r.is_app() && serves(bundle_of(&href)))) else {
            return;
        };
        e.prevent_default();
        if pathname() != href {
            nav.push(&route);
        }
        if let Some(w) = web_sys::window() {
            w.scroll_to_with_x_and_y(0.0, 0.0);
        }
    })
}
