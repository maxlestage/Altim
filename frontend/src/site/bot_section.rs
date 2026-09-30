//! « Bot Altim » on the presentation site (BotSection.tsx): what it is and how it is judged (fixed text), and its
//! current results read live from /api/bot (never written here: they change with each retraining).
use std::rc::Rc;

use altim_core::web::bot::{BotReport, family_label, forward_signals, fr_iso, headline_configs, plain, verdict_short};
use yew::prelude::*;

use super::Section;
use crate::api::Pending;

#[derive(Clone, PartialEq)]
enum State {
    Loading,
    Pending,
    Error,
    Ready(Rc<BotReport>),
}

#[component]
pub fn BotSection() -> Html {
    let state = use_state(|| State::Loading);
    {
        let state = state.clone();
        use_effect_with((), move |_| {
            let alive = Rc::new(std::cell::Cell::new(true));
            let a = alive.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // Pending (first computation on the server): asked again every 10 s, 60 times at most.
                for _ in 0..60 {
                    let r = crate::api::bot().await;
                    if !a.get() {
                        return;
                    }
                    match r {
                        Ok(Pending::Ready(r)) => return state.set(State::Ready(Rc::new(r))),
                        Ok(Pending::Pending) => state.set(State::Pending),
                        Err(_) => return state.set(State::Error),
                    }
                    gloo_timers::future::TimeoutFuture::new(10_000).await;
                    if !a.get() {
                        return;
                    }
                }
            });
            move || alive.set(false)
        });
    }
    html! {
        <Section
            id="bot"
            eyebrow="Bot Altim"
            title={html! { <>{ "Un bot qui doit " }<span class="gradient">{ "prouver" }</span>{ " avant de compter" }</> }}
            intro={html! { "Des modèles appris sur de longs historiques disent ACHETER, ATTENDRE ou VENDRE. Ils ne pèsent dans une décision que s'ils ont battu le hasard sur des périodes qu'ils n'avaient jamais vues, avec une marge qui tient compte de tous les essais faits." }}
        >
            <div class="bot-site">
                <div class="card bot-site-how">
                    <h3>{ "Comment il est construit" }</h3>
                    <ul>
                        <li><b>{ "ACHETER, ATTENDRE ou VENDRE" }</b>{ " à 20 et 60 jours de bourse, frais compris ; VENDRE veut dire sortir ou alléger, jamais vendre à découvert." }</li>
                        <li><b>{ "Plus de 20 ans de données" }</b>{ " : actions et ETF américains depuis 1990, cryptos depuis leur cotation ; 125 actifs, dont 34 fixés d'avance pour le test." }</li>
                        <li><b>{ "Plusieurs modèles candidats" }</b>{ " : régressions, arbres de décision entraînés longuement avec arrêt anticipé, règles publiées ; et un classement entre pairs (faire mieux que la médiane de son groupe)." }</li>
                        <li><b>{ "Walk-forward" }</b>{ " : réentraîné chaque année sur le seul passé, jugé sur l'année suivante qu'il n'a jamais vue." }</li>
                        <li><b>{ "Pré-enregistré le 30/09/2026" }</b>{ " : le protocole est figé avant les calculs ; tout ce qui arrive ensuite forme un " }<b>{ "test sur l'avenir" }</b>{ ", le seul vraiment neuf." }</li>
                        <li><b>{ "Seuil corrigé" }</b>{ " : chaque essai compte ; plus on essaie de modèles, plus la preuve exigée est forte." }</li>
                    </ul>
                </div>

                <div class="card bot-site-live" aria-live="polite">
                    <h3>{ "Ses résultats aujourd'hui" }</h3>
                    { match &*state {
                        State::Loading => html! { <p class="muted">{ "Chargement des résultats…" }</p> },
                        State::Pending => html! { <p class="muted">{ "Entraînement et test en cours sur le serveur (plusieurs minutes la première fois)…" }</p> },
                        State::Error => html! { <p class="muted">{ "Résultats indisponibles pour le moment." }</p> },
                        State::Ready(report) => ready(report),
                    } }
                    <a href="/app/bot" class="btn btn-small">{ "Voir le détail" }</a>
                </div>
            </div>
        </Section>
    }
}

fn ready(report: &BotReport) -> Html {
    let v3 = report.v3.as_ref();
    html! {
        <>
            <p class="bot-site-headline">{ v3.map(|v| v.headline.clone()).unwrap_or_else(|| report.headline.clone()) }</p>
            if let Some(v3) = v3 {
                <dl class="bot-site-tiles">
                    <div><dt>{ format!("t ≥ {}", plain(Some(v3.t_required), 2)) }</dt><dd>{ "seuil corrigé" }</dd></div>
                    <div><dt>{ v3.k.total }</dt><dd>{ "essais comptés depuis la v1" }</dd></div>
                    <div><dt>{ forward_signals(v3) }</dt><dd>{ format!("signaux jugés depuis le {}", fr_iso(&v3.prereg_date)) }</dd></div>
                </dl>
                <p class="muted small">{ &v3.forward_headline }</p>
                <ul class="bot-site-rows">
                    { for v3.groups.iter().flat_map(|g| headline_configs(g).into_iter().map(move |(horizon, c)| html! {
                        <li key={format!("{}-{horizon}-{}", g.id, c.id)}>
                            <span class="muted">{ format!("{} · {} · {horizon} j", if g.id == "stock" { "Actions" } else { "Cryptos" }, family_label(c.family).to_lowercase()) }</span>
                            <span>{ format!("achats : {}", verdict_short(c, true)) }</span>
                            <span>{ format!("ventes : {}", verdict_short(c, false)) }</span>
                        </li>
                    })) }
                </ul>
            }
            <p class="muted small">{ format!("Calculé le {}, réentraîné toutes les 12 h.", crate::ui::fr_date_time_short(report.as_of)) }</p>
        </>
    }
}
