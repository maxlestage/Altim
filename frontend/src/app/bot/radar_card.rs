//! « Bot Altim » on the Radar (BotRadarCard.tsx): the report's headline (read from /api/bot, never written here), the
//! forward test's counter and a link to the Bot screen. Loading, pending (first training) and error states; one
//! compact card. Used by the Radar.
use std::rc::Rc;

use altim_core::web::bot::{BotReport, forward_signals};
use yew::prelude::*;

use crate::api::Pending;
use crate::route::use_on_link;

#[derive(Clone, PartialEq)]
pub enum RadarState {
    Loading,
    Pending,
    Error,
    Ready(Rc<BotReport>),
}

#[component]
pub fn BotRadarCard() -> Html {
    let state = use_state(|| RadarState::Loading);
    {
        let state = state.clone();
        use_effect_with((), move |_| {
            let alive = Rc::new(std::cell::Cell::new(true));
            let a = alive.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // Pending (first computation on the server): asked again every 15 s.
                loop {
                    let r = crate::api::bot().await;
                    if !a.get() {
                        return;
                    }
                    match r {
                        Ok(Pending::Ready(r)) => return state.set(RadarState::Ready(Rc::new(r))),
                        Ok(Pending::Pending) => state.set(RadarState::Pending),
                        Err(_) => return state.set(RadarState::Error),
                    }
                    gloo_timers::future::TimeoutFuture::new(15_000).await;
                    if !a.get() {
                        return;
                    }
                }
            });
            move || alive.set(false)
        });
    }
    html! { <BotRadarView state={(*state).clone()} /> }
}

#[derive(Properties, PartialEq)]
pub struct BotRadarViewProps {
    pub state: RadarState,
}

/// The card itself.
#[component]
pub fn BotRadarView(p: &BotRadarViewProps) -> Html {
    let on_link = use_on_link();
    html! {
        <div class="card bot-radar">
            <h2 class="card-title">{ "Bot Altim" }</h2>
            { match &p.state {
                RadarState::Loading => html! { <p class="muted small">{ "Chargement…" }</p> },
                RadarState::Pending => html! { <p class="muted small">{ "Entraînement et test en cours sur le serveur (plusieurs minutes la première fois)…" }</p> },
                RadarState::Error => html! { <p class="muted small">{ "Résultats indisponibles pour le moment." }</p> },
                RadarState::Ready(r) => {
                    let n = r.v3.as_ref().map(forward_signals);
                    let headline = r.v3.as_ref().map(|v| v.headline.as_str()).unwrap_or(&r.headline);
                    html! {
                        <>
                            <p class="small bot-radar-headline">{ altim_core::web::bot::screen::first_sentence(headline).to_string() }</p>
                            if let Some(n) = n {
                                <p class="small"><b>{ format!("Test sur l'avenir : {n} {} sur 30", if n > 1 { "signaux" } else { "signal" }) }</b></p>
                            }
                        </>
                    }
                }
            } }
            <a href="/app/bot" onclick={on_link} class="link small">{ "Voir le bot →" }</a>
        </div>
    }
}
