//! The web app (/app/*, WebApp.tsx): header, tabs, disclaimer, and one module per screen (its sub-components in the
//! same folder, the pure logic in `altim_core::web`); this file only dispatches the routes.
pub mod common;
// Bot, validation, alerts, news and agenda
pub mod alerts;
pub mod bot;
pub mod news;
pub mod validation;
// Radar and asset screen
pub mod asset;
pub mod radar;
// Holdings and their tools
pub mod holdings;
// Selection, opportunities, settings, glossary, simulation, journal
pub mod glossary;
pub mod journal;
pub mod opportunities;
pub mod selection;
pub mod settings;
pub mod simulation;

use yew::prelude::*;

use crate::route::{Route, use_on_link};
use crate::state::app::{set_app_state, use_app_state};

const TABS: [(&str, &str, &str); 5] = [
    ("/app", "Radar", "M3 12a9 9 0 1 0 18 0 9 9 0 1 0-18 0M12 12l6-6M7.5 12a4.5 4.5 0 0 0 9 0"),
    ("/app/selection", "Sélection", "M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z"),
    ("/app/avoirs", "Mes avoirs", "M12 2 3 7l9 5 9-5-9-5zM3 12l9 5 9-5M3 17l9 5 9-5"),
    ("/app/actu", "Actu", "M4 5h13v14H6a2 2 0 0 1-2-2zM17 9h3v8a2 2 0 0 1-2 2M8 9h6M8 13h6M8 17h4"),
    ("/app/reglages", "Réglages", "M4 6h10M18 6h2M4 12h4M12 12h8M4 18h12M20 18h0M14 4v4M8 10v4M16 16v4"),
];

pub use altim_core::web::bundle::asset_of;

/// The tab lit for a screen (the simulation and the journal sit next to the holdings, the opportunities next to the
/// selection, the validation and the bot next to the settings, an asset and the alerts under the Radar).
fn active_tab(r: &Route) -> &'static str {
    match r {
        Route::Selection | Route::Opportunites => "/app/selection",
        Route::Avoirs | Route::Simulation | Route::Journal => "/app/avoirs",
        Route::Actu => "/app/actu",
        Route::Reglages | Route::Validation | Route::Bot => "/app/reglages",
        _ => "/app",
    }
}

/// The screen of a route. Each arm is compiled only into its group's .wasm (`app-<group>` features, see
/// altim_core::web::bundle); `route::switch` sends any other group's address to the server first.
fn screen(r: &Route) -> Html {
    match r {
        #[cfg(feature = "app-actif")]
        Route::Asset { kind, symbol } => match asset_of(kind, symbol) {
            Some((kind, symbol)) => {
                let key = format!("{}/{symbol}", kind.as_str());
                html! { <asset::AssetScreen {key} {kind} {symbol} /> }
            }
            None => radar(),
        },
        #[cfg(feature = "app-avoirs")]
        Route::Avoirs => html! { <holdings::MyHoldings /> },
        #[cfg(feature = "app-selection")]
        Route::Selection => html! { <selection::Selection /> },
        #[cfg(feature = "app-selection")]
        Route::Opportunites => html! { <opportunities::Opportunities /> },
        #[cfg(feature = "app-reglages")]
        Route::Reglages => html! { <settings::Settings /> },
        #[cfg(feature = "app-actu")]
        Route::Actu => html! { <news::News /> },
        #[cfg(feature = "app-reglages")]
        Route::Lexique => html! { <glossary::Glossary /> },
        #[cfg(feature = "app-avoirs")]
        Route::Simulation => html! { <simulation::Simulation /> },
        #[cfg(feature = "app-avoirs")]
        Route::Journal => html! { <journal::Journal /> },
        #[cfg(feature = "app-bot")]
        Route::Validation => html! { <validation::Validation /> },
        #[cfg(feature = "app-bot")]
        Route::Bot => html! { <bot::Bot /> },
        #[cfg(feature = "app-radar")]
        Route::Alertes => html! { <alerts::Alerts /> },
        _ => radar(),
    }
}

/// Any other /app/… address shows the Radar (WebApp.tsx); only the Radar's group gets such an address.
#[cfg(feature = "app-radar")]
fn radar() -> Html {
    html! { <radar::Radar /> }
}

#[cfg(not(feature = "app-radar"))]
fn radar() -> Html {
    html! {}
}

fn icon(d: &'static str) -> Html {
    html! {
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d={d} />
        </svg>
    }
}

/// Delay before `prefetch_groups`: the first screen and its data come first.
const PREFETCH_AFTER_MS: u32 = 5_000;

/// The page names other groups' files (`<meta name="altim-prefetch">`, scripts/build-web.sh: the Radar's and the asset
/// screen's, the most common moves): the browser fetches them in the background at its lowest priority, so going
/// there later loads from its cache. Not when the browser asks to save data.
fn prefetch_groups() {
    let save_data = js_sys::Reflect::get(&js_sys::global(), &"navigator".into())
        .and_then(|n| js_sys::Reflect::get(&n, &"connection".into()))
        .and_then(|c| js_sys::Reflect::get(&c, &"saveData".into()))
        .is_ok_and(|s| s.as_bool() == Some(true));
    let doc = gloo::utils::document();
    let (false, Some(meta), Some(head)) =
        (save_data, doc.query_selector("meta[name=altim-prefetch]").ok().flatten(), doc.query_selector("head").ok().flatten())
    else {
        return;
    };
    for url in meta.get_attribute("content").unwrap_or_default().split_whitespace() {
        if let Ok(link) = doc.create_element("link") {
            let _ = link.set_attribute("rel", "prefetch");
            let _ = link.set_attribute("href", url);
            let _ = head.append_child(&link);
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct WebAppProps {
    pub route: Route,
}

#[component]
pub fn WebApp(p: &WebAppProps) -> Html {
    let state = use_app_state();
    let _fx = crate::state::fx::use_fx();
    let on_link = use_on_link();
    use_effect_with((), |_| {
        crate::hooks::set_title("Altim — Application web");
        crate::state::fx::start_fx();
        // The alert checks of this browser (`startChecks` of notify.ts).
        alerts::start_checks();
        gloo::timers::callback::Timeout::new(PREFETCH_AFTER_MS, prefetch_groups).forget();
    });
    if !state.accepted_disclaimer {
        return html! { <Disclaimer /> };
    }
    let active = active_tab(&p.route);
    let tabs = |with_icon: bool| {
        TABS.iter()
            .map(|(href, label, d)| {
                let on = *href == active;
                html! {
                    <a key={*href} href={*href} onclick={on_link.clone()} class={if on { "on" } else { "" }} aria-current={on.then_some("page")}>
                        if with_icon {
                            { icon(d) }
                            <span>{ *label }</span>
                        } else {
                            { *label }
                        }
                    </a>
                }
            })
            .collect::<Html>()
    };
    html! {
        <div class="webapp">
            <header class="app-bar">
                <a href="/app" onclick={on_link.clone()} class="brand" aria-label="Altim, radar">
                    <img src="/logo.svg" alt="" width="30" height="30" />
                    <span>{ "ALTIM" }</span>
                </a>
                <span class="app-env">{ "CONSEIL" }</span>
                <nav class="app-tabs-top" aria-label="Sections">{ tabs(false) }</nav>
                <a href="/" class="app-site">{ "Site" }</a>
            </header>
            <main class="app-main">{ screen(&p.route) }</main>
            <nav class="app-tabs" aria-label="Sections">{ tabs(true) }</nav>
        </div>
    }
}

#[component]
fn Disclaimer() -> Html {
    let ok = use_state(|| false);
    let toggle = {
        let ok = ok.clone();
        Callback::from(move |e: Event| ok.set(e.target_unchecked_into::<web_sys::HtmlInputElement>().checked()))
    };
    let enter = Callback::from(|_| set_app_state(|s| s.accepted_disclaimer = true));
    html! {
        <main class="app-disclaimer">
            <img src="/logo.svg" alt="" width="72" height="72" />
            <h1>{ "Bienvenue sur " }<span class="gradient">{ "Altim" }</span></h1>
            <ul>
                <li>{ "Les signaux sont des probabilités calculées sur l'historique, jamais des certitudes. Aucun outil ne garantit un gain." }</li>
                <li>{ "Chaque cours est recoupé sur plusieurs sources indépendantes ; si les données ne sont pas fiables, le signal est suspendu." }</li>
                <li>{ "Altim vous " }<b>{ "conseille" }</b>{ " : il ne passe aucun ordre et n'a jamais accès à vos comptes. Vous restez libre de suivre ou non ses conseils." }</li>
                <li>{ "Ce que vous renseignez dans « Mes avoirs » reste dans ce navigateur." }</li>
                <li>{ "Le trading comporte un risque de perte totale du capital investi." }</li>
            </ul>
            <label class="check-row">
                <input type="checkbox" checked={*ok} onchange={toggle} />
                <span>{ "J'ai compris que je reste seul responsable de mes décisions d'investissement." }</span>
            </label>
            <button class="btn" disabled={!*ok} onclick={enter}>{ "Entrer dans l'app" }</button>
            <a href="/" class="muted back-site">{ "← Retour au site" }</a>
        </main>
    }
}
