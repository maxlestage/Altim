//! URLs of the site and the web app (the same as the React front: App.tsx, WebApp.tsx, router.ts). The site's links
//! are plain page loads; inside /app, `use_on_link()` gives the `onclick` of an internal link (history navigation
//! without reloading, like `onLink` of router.ts). A new screen = a variant here + its arm in `app::screen`.
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
    match r {
        Route::Home => html! { <crate::site::Home /> },
        Route::MentionsLegales | Route::Confidentialite | Route::Risques => html! { <crate::site::legal::LegalScreen route={r} /> },
        // Any other /app/… address shows the Radar (WebApp.tsx); anything else the home page (App.tsx).
        Route::NotFound if pathname().starts_with("/app/") => html! { <crate::app::WebApp route={Route::Radar} /> },
        Route::NotFound => html! { <crate::site::Home /> },
        r => html! { <crate::app::WebApp route={r} /> },
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
        let (Some(nav), Some(route)) = (nav.as_ref(), Route::recognize(&href)) else { return };
        e.prevent_default();
        if pathname() != href {
            nav.push(&route);
        }
        if let Some(w) = web_sys::window() {
            w.scroll_to_with_x_and_y(0.0, 0.0);
        }
    })
}
