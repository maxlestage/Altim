//! "Point du jour" (BriefCard.tsx): the day in a few lines for the radar and the holdings (/api/brief), refreshed
//! every 5 minutes while the tab is visible. Used by the Radar.
use std::rc::Rc;

use altim_core::web::insights::news::{BriefReport, brief_pct};
use altim_core::web::store::asset_key;
use yew::prelude::*;

use crate::app::news::use_my_assets;
use crate::hooks::every_visible;
use crate::route::use_on_link;

#[component]
pub fn BriefCard() -> Html {
    let assets = use_my_assets();
    let on_link = use_on_link();
    let brief = use_state(|| None::<Rc<BriefReport>>);
    let key = assets.iter().map(|(s, k)| asset_key(s, *k)).collect::<Vec<_>>().join(",");
    {
        let brief = brief.clone();
        use_effect_with(key, move |_| {
            let alive = Rc::new(std::cell::Cell::new(true));
            let load = {
                let alive = alive.clone();
                move || {
                    let (brief, alive, assets) = (brief.clone(), alive.clone(), assets.clone());
                    wasm_bindgen_futures::spawn_local(async move {
                        let url = format!("/api/brief?symbols={}{}", crate::api::list(&assets), crate::money::cur_param());
                        if let Ok(b) = crate::api::get::<BriefReport>(&url).await {
                            if alive.get() {
                                brief.set(Some(Rc::new(b)));
                            }
                        }
                    });
                }
            };
            load();
            let timer = every_visible(300_000, load);
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }
    let Some(b) = &*brief else { return html! { <div class="skeleton" aria-label="Chargement du point du jour" /> } };
    let moves = b.moves();
    let themes = b.market.as_ref().map(|m| m.themes.clone()).filter(|t| !t.is_empty());
    html! {
        <div class="card brief-card">
            <h2 class="card-title">{ "Point du jour" }</h2>
            <p class="brief-headline">{ &b.headline }</p>
            if let Some(t) = themes {
                <p class="muted small">{ format!("Sujets du moment : {}.", t.join(", ").to_lowercase()) }</p>
            }
            if !moves.is_empty() {
                <div class="news-tags">
                    { for moves.iter().map(|m| html! {
                        <a key={asset_key(&m.symbol, m.kind)} class="chip" href={format!("/app/actif/{}/{}", m.kind.as_str(), m.symbol)} onclick={on_link.clone()}>
                            { format!("{} ", m.symbol) }<b class={if m.change >= 0.0 { "up" } else { "down" }}>{ brief_pct(m.change) }</b>
                        </a>
                    }) }
                </div>
            }
            if !b.news.is_empty() {
                <ul class="brief-news small">
                    { for b.news.iter().map(|n| html! {
                        <li key={n.id.clone()}>
                            if n.alert {
                                <span class="badge sell">{ "ALERTE" }</span>
                            }
                            { " " }
                            <a href={n.link.clone()} target="_blank" rel="noopener noreferrer nofollow">{ &n.title }</a>{ " " }
                            <span class="muted">{ format!("· {}{}", n.source, if n.also_in.is_empty() { String::new() } else { format!(" +{}", n.also_in.len()) }) }</span>
                        </li>
                    }) }
                </ul>
            }
            <p class="muted small">
                { "Achetable = la décision complète de l'actif dit ACHETER ou ZONE D'ACHAT (la règle des notifications) ; tant qu'elle n'est pas calculée, rien n'est annoncé. Variations depuis la dernière clôture journalière. Conseil indicatif : Altim ne passe aucun ordre." }
            </p>
        </div>
    }
}
