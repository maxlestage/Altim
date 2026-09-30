//! Markets panel (Ticker.tsx): prices validated by consensus, in the display currency, with the bot's live view
//! (/api/bot/views, refreshed every 5 minutes; empty when unavailable). Fixed grid, no horizontal scrolling.
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::web::bot::{BotView, action_ui};
use altim_core::web::market::format_percent;
use yew::prelude::*;

use super::hero::TicksProps;

const BOT_EVERY: u32 = 5 * 60_000;

#[hook]
fn use_bot_views(ticks: Rc<Vec<altim_core::web::market::Tick>>) -> Rc<HashMap<String, BotView>> {
    let views = use_state(|| Rc::new(HashMap::new()));
    let key: String = ticks.iter().map(|t| format!("{}:{}", t.kind.as_str(), t.symbol)).collect::<Vec<_>>().join(",");
    {
        let views = views.clone();
        use_effect_with(key, move |key| {
            let mut timer = None;
            let alive = Rc::new(std::cell::Cell::new(true));
            if !key.is_empty() {
                let items: Vec<(String, altim_core::types::Kind)> = ticks.iter().map(|t| (t.symbol.clone(), t.kind)).collect();
                let load = {
                    let alive = alive.clone();
                    move || {
                        let (views, items, alive) = (views.clone(), items.clone(), alive.clone());
                        wasm_bindgen_futures::spawn_local(async move {
                            if let Ok(r) = crate::api::bot_views(&items).await {
                                if alive.get() {
                                    views.set(Rc::new(r.views.into_iter().map(|v| (format!("{}:{}", v.kind.as_str(), v.symbol), v)).collect()));
                                }
                            }
                        });
                    }
                };
                load();
                timer = Some(gloo::timers::callback::Interval::new(BOT_EVERY, load));
            }
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }
    (*views).clone()
}

#[component]
pub fn Ticker(p: &TicksProps) -> Html {
    let _m = crate::money::use_money();
    let bot = use_bot_views(p.ticks.clone());
    let any_bot = bot.values().any(|v| v.available && v.action.is_some());
    html! {
        <section class="markets" aria-labelledby="markets-title">
            <div class="markets-head">
                <h2 id="markets-title">{ "Marchés en direct" }</h2>
                <small class="muted">{ "Chaque cours = médiane des sources concordantes" }{ if any_bot { " · avis du Bot Altim en direct" } else { "" } }</small>
            </div>
            if p.ticks.is_empty() {
                <p class="muted markets-loading">{ "Interrogation des sources…" }</p>
            } else {
                <ul class="markets-grid">
                    { for p.ticks.iter().map(|t| {
                        let v = bot.get(&format!("{}:{}", t.kind.as_str(), t.symbol));
                        let ui = v.filter(|v| v.available).and_then(|v| v.action).map(action_ui);
                        let ok = t.agreeing >= 2;
                        html! {
                            <li key={t.symbol.clone()} class="market">
                                <div class="market-top">
                                    <b>{ &t.symbol }</b>
                                    <span class={if ok { "agree ok" } else { "agree weak" }} title={format!("{} source(s) concordante(s) sur {}", t.agreeing, t.total)}>
                                        { format!("{} {}/{}", if ok { "✔" } else { "!" }, t.agreeing, t.total) }
                                    </span>
                                </div>
                                <span class="market-price">{ crate::money::price_sep(t.price, " ") }</span>
                                if let Some(c) = t.change {
                                    <span class={if c >= 0.0 { "up" } else { "down" }}>{ format_percent(c) }</span>
                                }
                                if let (Some((label, tone)), Some(v)) = (ui, v) {
                                    <a class="market-bot" href="/app/bot" title={if v.counts { "Compte dans la décision" } else { "Pour information : ne compte pas dans la décision" }}>
                                        <span class={classes!("bot-chip", tone)}>{ format!("Bot : {label}") }</span>
                                        if !v.counts {
                                            <small class="muted">{ "pour info" }</small>
                                        }
                                    </a>
                                }
                            </li>
                        }
                    }) }
                </ul>
            }
            if any_bot {
                <small class="muted markets-note">
                    { "Avis du bot = modèle appris, jugé hors échantillon ; tant qu'il n'a pas prouvé d'avantage, il ne pèse pas dans les décisions (« pour info »). " }
                    <a href="/app/bot">{ "Voir ses résultats" }</a>
                </small>
            }
        </section>
    }
}
