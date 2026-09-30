//! Small components shared by several screens (FxNote.tsx, PortfolioTabs of Simulation.tsx).
use yew::prelude::*;

use crate::route::use_on_link;

#[derive(Properties, PartialEq)]
pub struct FxNoteProps {
    #[prop_or(AttrValue::Static("muted small fx-note"))]
    pub class: AttrValue,
}

/// Discreet line under amounts: "1 $ = 0,881 € · Yahoo Finance, 14:05", or why they are still in dollars (euros
/// asked but no rate: never a made-up conversion).
#[component]
pub fn FxNote(p: &FxNoteProps) -> Html {
    let state = crate::state::app::use_app_state();
    let fx = crate::state::fx::use_fx();
    if state.currency == altim_core::web::money::Currency::Usd {
        return html! {};
    }
    match &fx.fx {
        None => html! { <p class={p.class.clone()} title={fx.error.clone()}>{ "Taux EUR/USD indisponible : montants affichés en $." }</p> },
        Some(r) => html! {
            <p class={p.class.clone()}>{ crate::money::fx_line(r, js_sys::Date::now()) }{ if r.stale { " (dernier taux connu)" } else { "" } }</p>
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PortfolioTab {
    Real,
    Paper,
    Journal,
}

#[derive(Properties, PartialEq)]
pub struct PortfolioTabsProps {
    pub active: PortfolioTab,
}

/// "Mes avoirs réels" / "Simulation" / "Journal" switch, at the top of the three screens.
#[component]
pub fn PortfolioTabs(p: &PortfolioTabsProps) -> Html {
    let on_link = use_on_link();
    let tab = |t: PortfolioTab, href: &'static str, label: Html| {
        let on = p.active == t;
        html! { <a {href} onclick={on_link.clone()} class={if on { "on" } else { "" }} aria-current={on.then_some("page")}>{ label }</a> }
    };
    html! {
        <nav class="portfolio-tabs three" aria-label="Portefeuille">
            { tab(PortfolioTab::Real, "/app/avoirs", html! { "Mes avoirs réels" }) }
            { tab(PortfolioTab::Paper, "/app/simulation", html! { <>{ "Simulation " }<small>{ "(sans argent réel)" }</small></> }) }
            { tab(PortfolioTab::Journal, "/app/journal", html! { "Journal" }) }
        </nav>
    }
}
