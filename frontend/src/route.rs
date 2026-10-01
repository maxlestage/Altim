//! URLs of the site and the web app (the same as the React front: App.tsx, WebApp.tsx, router.ts). The site's links
//! are plain page loads; inside /app, `use_on_link()` gives the `onclick` of an internal link (history navigation
//! without reloading, like `onLink` of router.ts). A new screen = a variant here + its arm in `app::screen`.
pub use crate::part::serves;
use altim_core::web::bundle::bundle_of;
use wasm_bindgen::JsCast;
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

fn switch(r: Route) -> Html {
    if !serves(bundle_of(&pathname())) {
        return other_bundle();
    }
    reloaded();
    if r.is_app() || (r == Route::NotFound && pathname().starts_with("/app/")) { crate::part::app(r) } else { crate::part::site(r) }
}

/// The address `other_bundle` last reloaded (session storage).
const RELOAD_KEY: &str = "altim.front.reload";

/// This address belongs to another .wasm (the site and each group of app screens are built apart): the whole app
/// takes the page over for an app address when the loader has it ready, else the page is loaded from the server,
/// once per address so a misrouted page can never loop.
fn other_bundle() -> Html {
    // An app address reached in place (back button, notification): the whole app takes the page over when ready.
    if bundle_of(&pathname()) != "site" && crate::part::hand_over(None) {
        return html! {};
    }
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

/// `onclick` of an internal /app link: navigation without reloading, then back to the top of the page (`onLink` of
/// router.ts). The browser keeps its own behaviour for Ctrl/⌘/Shift/Alt-click and the other buttons, a link with a
/// `target` or `download`, and an address outside the app. Another group's screen shows in place once the whole app
/// is ready (`part::hand_over`), else it is a page load; a site page is always a page load.
#[hook]
pub fn use_on_link() -> Callback<MouseEvent> {
    let nav = use_navigator();
    Callback::from(move |e: MouseEvent| {
        if e.default_prevented() || e.meta_key() || e.ctrl_key() || e.shift_key() || e.alt_key() || e.button() != 0 {
            return;
        }
        // Yew listens at the app's root: `currentTarget` is that root, the link is the clicked element's closest <a>.
        let Some(a) = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()).and_then(|t| t.closest("a").ok().flatten()) else {
            return;
        };
        let Some(href) = a.get_attribute("href") else { return };
        if a.has_attribute("download") || a.get_attribute("target").is_some_and(|t| !t.is_empty() && t != "_self") {
            return;
        }
        let shown = if let Some(route) = in_app_route(&href) {
            let Some(nav) = nav.as_ref() else { return };
            e.prevent_default();
            if pathname() != href {
                nav.push(&route);
            }
            true
        } else {
            // Another group's screen: in place with the whole app when the loader has it ready, else a page load.
            other_group_screen(&href) && crate::part::hand_over(Some(&href)) && {
                e.prevent_default();
                true
            }
        };
        if shown {
            if let Some(w) = web_sys::window() {
                w.scroll_to_with_x_and_y(0.0, 0.0);
            }
        }
    })
}

/// An app screen of another group (same-site path, no query or fragment).
fn other_group_screen(href: &str) -> bool {
    href.starts_with('/') && !href.starts_with("//") && !href.contains(['?', '#']) && bundle_of(href) != "site" && !serves(bundle_of(href))
}

/// The app screen of an `href` that this .wasm shows in place: a same-site path (no scheme, no `//`, no query or
/// fragment) of an app screen of this group.
pub fn in_app_route(href: &str) -> Option<Route> {
    if !href.starts_with('/') || href.starts_with("//") || href.contains(['?', '#']) {
        return None;
    }
    Route::recognize(href).filter(|r| r.is_app() && serves(bundle_of(href)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_shown_in_place() {
        // The default build has every screen: any app screen navigates in place.
        assert_eq!(in_app_route("/app/reglages"), Some(Route::Reglages));
        assert_eq!(in_app_route("/app"), Some(Route::Radar));
        assert_eq!(in_app_route("/app/actif/crypto/BTC"), Some(Route::Asset { kind: "crypto".into(), symbol: "BTC".into() }));
        // The site, other origins, protocol-relative links, queries and fragments: the browser's own navigation.
        for href in ["/", "/risques", "https://example.org/app", "//example.org/app", "/app/bot?x=1", "/app#top", "mailto:a@b.c", "app"] {
            assert_eq!(in_app_route(href), None, "{href}");
        }
    }
}
