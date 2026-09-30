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

use altim_core::types::Kind;
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

/// `^(crypto|stock)/[A-Za-z0-9.-]{1,10}$` of WebApp.tsx: the asset of /app/actif/…, symbol in upper case.
pub fn asset_of(kind: &str, symbol: &str) -> Option<(Kind, String)> {
    let kind = Kind::parse(kind)?;
    let ok = (1..=10).contains(&symbol.len()) && symbol.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    ok.then(|| (kind, symbol.to_ascii_uppercase()))
}

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

fn screen(r: &Route) -> Html {
    match r {
        Route::Asset { kind, symbol } => match asset_of(kind, symbol) {
            Some((kind, symbol)) => {
                let key = format!("{}/{symbol}", kind.as_str());
                html! { <asset::AssetScreen {key} {kind} {symbol} /> }
            }
            None => html! { <radar::Radar /> },
        },
        Route::Avoirs => html! { <holdings::MyHoldings /> },
        Route::Selection => html! { <selection::Selection /> },
        Route::Opportunites => html! { <opportunities::Opportunities /> },
        Route::Reglages => html! { <settings::Settings /> },
        Route::Actu => html! { <news::News /> },
        Route::Lexique => html! { <glossary::Glossary /> },
        Route::Simulation => html! { <simulation::Simulation /> },
        Route::Journal => html! { <journal::Journal /> },
        Route::Validation => html! { <validation::Validation /> },
        Route::Bot => html! { <bot::Bot /> },
        Route::Alertes => html! { <alerts::Alerts /> },
        _ => html! { <radar::Radar /> },
    }
}

fn icon(d: &'static str) -> Html {
    html! {
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d={d} />
        </svg>
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_paths() {
        assert_eq!(asset_of("crypto", "btc"), Some((Kind::Crypto, "BTC".into())));
        assert_eq!(asset_of("stock", "BRK.B"), Some((Kind::Stock, "BRK.B".into())));
        assert_eq!(asset_of("fx", "EUR"), None);
        assert_eq!(asset_of("stock", "TOOLONGSYMBOL"), None);
        assert_eq!(asset_of("stock", "A B"), None);
    }
}
