//! « Bots sélectifs » (v4) on the Bot screen and in the decision's bot line: each bot's avis today (or « pas
//! d'avis »), its measured precision with the Wilson interval, its signals per year and its status (prouvé, en
//! attente, contredit, non prouvé). Stacked cards and wrapping rows, mobile first.
use std::rc::Rc;

use altim_core::engine::bot::BotReport;
use altim_core::engine::bot_v4::{PrecisionStats, SelectiveBot, Status, V4View, precision_text};
use altim_core::web::bot::fr_iso;
use altim_core::web::bot::screen::points;
use altim_core::web::insights::validation::{month_year, plain};
use yew::prelude::*;

fn tone(s: Status) -> &'static str {
    match s {
        Status::Proven => "good",
        Status::Contradicted => "warning",
        _ => "info",
    }
}

fn status_chip(s: Status) -> Html {
    html! { <span class={classes!("chip", "val-verdict", tone(s))}>{ s.label() }</span> }
}

fn pct(v: Option<f64>) -> String {
    v.map(|x| format!("{}\u{a0}%", plain(Some(x), 0))).unwrap_or_else(|| "—".into())
}

fn list(items: &[String]) -> Html {
    html! { <ul class="reasons">{ for items.iter().map(|x| html! { <li key={x.clone()}>{ x }</li> }) }</ul> }
}

/// « 62 % (57-67 %) » or « aucun signal ».
fn precision_short(s: &PrecisionStats) -> String {
    if s.signals == 0 { "aucun signal".into() } else { format!("{} ({}–{})", pct(s.precision), pct(s.wilson_low), pct(s.wilson_high)) }
}

/// The avis of today on the basket (« pas d'avis » when silent).
fn today_text(b: &SelectiveBot) -> String {
    if b.today_level.is_none() {
        return "pas d'avis (seuil fermé sur la dernière année)".into();
    }
    if b.today.is_empty() { "pas d'avis".into() } else { format!("{} : {}", b.side.avis(), b.today.join(", ")) }
}

#[derive(Properties, PartialEq)]
pub struct BotV4SectionProps {
    pub report: Rc<BotReport>,
}

#[component]
pub fn BotV4Section(p: &BotV4SectionProps) -> Html {
    let Some(v4) = p.report.v4.as_ref() else { return html! {} };
    let req = v4.t_required;
    let prereg = fr_iso(&v4.prereg_date);
    let precise = v4.bots.iter().filter(|b| b.status != Status::NotProven).count();
    let fwd: usize = v4.bots.iter().map(|b| b.forward.signals).sum();
    // Stocks 20, stocks 60, cryptos 20, cryptos 60: four cards of four bots.
    let mut cards: Vec<(String, Vec<SelectiveBot>)> = Vec::new();
    for b in &v4.bots {
        let title = format!("{} · {} jours", b.group.label(), b.horizon);
        match cards.last_mut() {
            Some((t, v)) if *t == title => v.push(b.clone()),
            _ => cards.push((title, vec![b.clone()])),
        }
    }
    html! {
        <>
            <h2 class="section-label">{ "Bots sélectifs" }</h2>
            <div class="card">
                <h2 class="card-title">{ format!("Bots sélectifs · pré-enregistrés le {prereg}") }</h2>
                <p class="small">{ &v4.headline }</p>
                <div class="val-tiles">
                    <div class="val-tile"><span class="muted small">{ "Seuil corrigé" }</span><b>{ format!("t ≥ {}", plain(Some(req), 2)) }</b></div>
                    <div class="val-tile"><span class="muted small">{ "Tests comptés (v1 à v4)" }</span><b>{ v4.k.total }</b></div>
                    <div class="val-tile"><span class="muted small">{ "Bots précis sur le passé" }</span><b>{ format!("{precise} / {}", v4.bots.len()) }</b></div>
                </div>
                <p class="muted small">
                    { "Chaque bot parle rarement : seulement quand deux modèles sont d'accord sur un score extrême, sinon « pas d'avis ». Précision = part des signaux qui ont réussi, avec son intervalle à 95 % ; « précis » seulement si le bas de l'intervalle dépasse le hasard et que le t dépasse le seuil corrigé." }
                </p>
                if !v4.after_prereg.is_empty() {
                    <div class="notice warn small">{ "Modifié après le pré-enregistrement :" }{ list(&v4.after_prereg) }</div>
                }
            </div>
            <div class="val-grid">
                { for cards.into_iter().map(|(title, bots)| html! {
                    <div key={title.clone()} class="card val-card">
                        <h3 class="val-title">{ title }</h3>
                        <ul class="bot-cands">{ for bots.iter().map(|b| html! { <BotV4Row key={b.id.clone()} b={b.clone()} {req} /> }) }</ul>
                    </div>
                }) }
            </div>
            <div class="card">
                <h2 class="card-title">{ "Depuis le 02/10/2026 (test sur l'avenir)" }</h2>
                <p class="small">{ &v4.forward_headline }</p>
                <p class="muted small">
                    { format!("{fwd} signal{} jugé{} à ce jour. Un bot précis sur le passé ne compte dans les décisions qu'après {} signaux sur l'avenir qui le confirment.", if fwd > 1 { "aux" } else { "" }, if fwd > 1 { "s" } else { "" }, v4.parameters.min_signals) }
                </p>
            </div>
            <div class="card">
                <h2 class="card-title">{ "Comment les bots sélectifs sont jugés" }</h2>
                { list(&v4.method) }
                <details class="small">
                    <summary>{ "Limites" }</summary>
                    { list(&v4.limits) }
                </details>
            </div>
        </>
    }
}

#[derive(Properties, PartialEq)]
pub struct BotV4RowProps {
    pub b: SelectiveBot,
    pub req: f64,
}

#[component]
pub fn BotV4Row(p: &BotV4RowProps) -> Html {
    let (b, s) = (&p.b, &p.b.main);
    let rhythm = s.per_year.map(|y| format!("{} par an", plain(Some(y), 1))).unwrap_or_else(|| "rythme —".into());
    let coverage = s.coverage.map(|c| format!(" · parle sur {} des jours", pct_fine(c))).unwrap_or_default();
    html! {
        <li class="bot-side">
            <p class="small bot-side-head"><b>{ b.side.label() }</b>{ status_chip(b.status) }</p>
            <p class="small"><span class="muted">{ "Avis du jour : " }</span>{ today_text(b) }</p>
            <p class="kv small"><span>{ "Précision mesurée" }</span><b>{ precision_short(s) }</b></p>
            <p class="kv small"><span>{ "Hasard (référence)" }</span><b>{ pct(s.reference) }</b></p>
            <p class="kv small"><span>{ "Signaux (rythme)" }</span><b>{ format!("{} · {rhythm}", s.signals) }</b></p>
            <p class="kv small"><span>{ "Écart moyen face au hasard" }</span><b>{ points(s.excess) }</b></p>
            <p class="kv small"><span>{ "t par jour" }</span><b>{ format!("{} (requis {})", if s.t.is_none() { "—".into() } else { plain(s.t, 1) }, plain(Some(p.req), 2)) }</b></p>
            <details class="small">
                <summary>{ "Détails" }</summary>
                <p class="muted small">{ format!("Réussite : {}.", b.hit) }</p>
                <p class="muted small">{ format!("{}.", b.models) }</p>
                <p class="muted small">
                    { format!(
                        "Test de {} à {}{coverage} ; seuil ouvert dans {} réentraînements sur {} ; probabilité moyenne annoncée {} pour {} de réussite ; taux de base {}, entrée au hasard {}.",
                        month_year(s.from),
                        month_year(s.to),
                        b.open_blocks,
                        b.blocks,
                        pct(s.mean_prob),
                        pct(s.precision),
                        pct(s.base_rate),
                        pct(s.random_rate)
                    ) }
                </p>
                if s.excess_median.is_some() {
                    <p class="muted small">{ format!("Face à la médiane du groupe : {}.", points(s.excess_median)) }</p>
                }
                <p class="muted small">{ format!("Autres actifs (entraînement) : {}.", precision_text(&b.extra)) }</p>
            </details>
        </li>
    }
}

/// « 0,4 % » (coverage is often under 1 %).
fn pct_fine(c: f64) -> String {
    format!("{}\u{a0}%", plain(Some(c), if c < 1.0 { 2 } else { 1 }))
}

#[derive(Properties, PartialEq)]
pub struct V4AvisProps {
    pub v: V4View,
}

/// The decision's bot line: the group's selective bots today, compactly (those speaking first), and whether they count.
#[component]
pub fn V4Avis(p: &V4AvisProps) -> Html {
    let v = &p.v;
    if !v.available {
        return html! { <p class="small muted">{ v.note.clone() }</p> };
    }
    let speaking: Vec<_> = v.signals.iter().filter(|s| s.action.is_some()).collect();
    html! {
        <div class="bot-v4-avis">
            <p class="small">
                <b>{ "Bots sélectifs : " }</b>
                { if speaking.is_empty() { format!("pas d'avis aujourd'hui ({} bots)", v.signals.len()) } else { format!("{} avis sur {} bots", speaking.len(), v.signals.len()) } }
            </p>
            if !speaking.is_empty() {
                <ul class="bot-cands">
                    { for speaking.iter().map(|s| html! {
                        <li key={s.id.clone()} class="small">
                            <span class="bot-side-head">{ s.label.clone() }{ status_chip(s.status) }</span>
                            <span class="muted">{ format!(
                                " {} · précision mesurée {} ({}–{}) contre {} au hasard · {} signaux par an",
                                s.side.avis(),
                                pct(s.precision),
                                pct(s.wilson_low),
                                pct(s.wilson_high),
                                pct(s.reference),
                                plain(s.per_year, 1)
                            ) }</span>
                        </li>
                    }) }
                </ul>
            }
            <p class="small muted">{ v.note.clone() }</p>
        </div>
    }
}
