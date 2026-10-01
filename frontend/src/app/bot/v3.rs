//! « Bot Altim » v3 on the Bot screen (BotV3.tsx): what changed, the pre-registration date, the corrected threshold
//! (K tests), the 4 headline results (2 families × horizons 20 / 60) with raw and required t, the forward test, each
//! configuration alone for information, the volatility-managed trend vs holding, and the time / memory of the
//! computation. Stacked cards and rows (no table), mobile first.
use std::rc::Rc;

use altim_core::engine::bot::BotReport;
use altim_core::engine::bot_v3::{Family, SideStats, V3Config, V3Group, V3Horizon, VolManaged};
use altim_core::js::fr;
use altim_core::web::bot::fr_iso;
use altim_core::web::bot::screen::{
    Run, choice_runs, compute_text, family_label, forward_signals, forward_text, headline_configs, judged, pending_text, points, proven, t_line,
    t_vs_required, v3_short, v3_side_text, v3_verdict_label, vol_rows,
};
use altim_core::web::insights::validation::{month_year, plain, signed_pct, verdict_tone};
use yew::prelude::*;

fn chip(s: &SideStats, required: f64) -> Html {
    html! { <span class={classes!("chip", "val-verdict", s.verdict.map(verdict_tone).unwrap_or("info"))}>{ v3_verdict_label(s, required) }</span> }
}

fn list(items: &[String]) -> Html {
    html! { <ul class="reasons">{ for items.iter().map(|x| html! { <li key={x.clone()}>{ x }</li> }) }</ul> }
}

#[derive(Properties, PartialEq)]
pub struct BotV3SectionProps {
    pub report: Rc<BotReport>,
}

#[component]
pub fn BotV3Section(p: &BotV3SectionProps) -> Html {
    let Some(v3) = p.report.v3.as_ref() else { return html! {} };
    let req = v3.t_required;
    let fwd = forward_signals(v3);
    let compute = compute_text(p.report.timing.as_ref());
    let prereg = fr_iso(&v3.prereg_date);
    let horizons = v3.parameters.horizons.iter().map(|h| h.to_string()).collect::<Vec<_>>().join(" et ");
    html! {
        <>
            <div class="card">
                <h2 class="card-title">{ format!("Bot v3 · pré-enregistré le {prereg}") }</h2>
                <div class="val-tiles">
                    <div class="val-tile"><span class="muted small">{ "Seuil corrigé" }</span><b>{ format!("t ≥ {}", plain(Some(req), 2)) }</b></div>
                    <div class="val-tile"><span class="muted small">{ "Tests comptés (v1 à v3)" }</span><b>{ v3.k.total }</b></div>
                    <div class="val-tile"><span class="muted small">{ "Test sur l'avenir" }</span><b>{ format!("{fwd} {}", if fwd > 1 { "signaux" } else { "signal" }) }</b></div>
                </div>
                <p class="muted small">
                    { format!(
                        "Protocole écrit avant tout calcul et figé : {} tests en v1, {} en v2, {} en v3. Avec autant d'essais, un t de 2 arrive par hasard ; un avantage n'est dit démontré qu'au-delà de t = {} (Bonferroni, 5 %). Horizons jugés à part : {horizons} jours de bourse.",
                        v3.k.v1,
                        v3.k.v2,
                        v3.k.v3,
                        plain(Some(req), 2)
                    ) }
                </p>
                if !v3.after_prereg.is_empty() {
                    <div class="notice warn small">
                        { "Modifié après le pré-enregistrement :" }
                        { list(&v3.after_prereg) }
                    </div>
                }
                <details class="small">
                    <summary>{ "Ce qui change avec la v3" }</summary>
                    { list(&v3.changes) }
                </details>
            </div>

            <h2 class="section-label">{ "Résultats v3 hors échantillon" }</h2>
            <p class="muted small">
                { format!("Panier fixe, signaux datés jusqu'au {prereg} ; modèle choisi à chaque réentraînement sur une validation interne, jamais sur le test.") }
            </p>
            <div class="val-grid">{ for v3.groups.iter().map(|g| html! { <V3GroupCard key={g.id.id()} g={g.clone()} {req} /> }) }</div>

            <h2 class="section-label">{ format!("Depuis le {prereg} (test sur l'avenir)") }</h2>
            <div class="card">
                <p class="small">{ &v3.forward_headline }</p>
                <p class="muted small">
                    { format!("Le seul test vraiment neuf : ces signaux n'existaient pas quand le protocole a été figé. Même calcul, même seuil ; un avantage passé qu'il contredit (après au moins {} signaux) cesse de compter.", v3.parameters.min_signals) }
                </p>
                <ul class="bot-cands">
                    { for v3.groups.iter().flat_map(|g| headline_configs(g).into_iter().map(move |(horizon, c)| html! {
                        <li key={format!("{}-{horizon}-{}", g.id.id(), c.id)}>
                            <p class="small"><b>{ &g.label }</b>{ format!(" · {} · {horizon} jours", family_label(c.family)) }</p>
                            <p class="muted small">{ forward_text(&c.forward, req) }</p>
                        </li>
                    })) }
                </ul>
            </div>

            <h2 class="section-label">{ "Chaque configuration seule" }</h2>
            <p class="muted small">{ "À titre d'information, non utilisé pour choisir : chaque candidat retenu partout, et la sélection v2 refaite sur les nouvelles données." }</p>
            <div class="val-grid">
                { for v3.groups.iter().flat_map(|g| g.horizons.iter().map(move |h| html! {
                    <ConfigsCard key={format!("{}-{}", g.id.id(), h.horizon)} title={format!("{} · {} jours", g.label, h.horizon)} configs={h.configs.clone()} {req} />
                })) }
            </div>

            if v3.groups.iter().any(|g| g.vol_managed.is_some()) {
                <h2 class="section-label">{ "Tendance à volatilité gérée" }</h2>
                <p class="muted small">
                    { "Règle publiée, sans apprentissage : investi seulement au-dessus de la moyenne 200 jours, exposition réduite quand la volatilité dépasse sa médiane passée ; comparée à la simple détention, sur les mêmes jours de test." }
                </p>
                <div class="val-grid">
                    { for v3.groups.iter().filter_map(|g| g.vol_managed.as_ref().map(|v| html! { <VolCard key={g.id.id()} label={g.label.clone()} v={v.clone()} /> })) }
                </div>
            }

            <div class="card">
                <h2 class="card-title">{ "Comment la v3 est jugée" }</h2>
                { list(&v3.method) }
                <details class="small">
                    <summary>{ format!("Les {} candidats", v3.candidates.len()) }</summary>
                    <ul class="reasons">
                        { for v3.candidates.iter().map(|c| html! {
                            <li key={c.id.id()}><b>{ &c.label }</b>{ format!(" ({}) — {}", family_label(c.family), c.description) }</li>
                        }) }
                    </ul>
                </details>
                <details class="small">
                    <summary>{ "Limites de la v3" }</summary>
                    { list(&v3.limits) }
                </details>
                if let Some(c) = compute {
                    <p class="muted small">
                        { format!("Calcul : {c} ; {} réentraînements, jusqu'à {} jours × actifs par groupe et horizon.", v3.compute.blocks, fr(v3.compute.max_rows as f64, 0, 3)) }
                    </p>
                }
            </div>
        </>
    }
}

#[derive(Properties, PartialEq)]
pub struct V3GroupCardProps {
    pub g: V3Group,
    pub req: f64,
}

#[component]
pub fn V3GroupCard(p: &V3GroupCardProps) -> Html {
    let g = &p.g;
    let peers = g.peers_from.map(|t| format!(" · classement entre pairs possible depuis {}", month_year(Some(t)))).unwrap_or_default();
    html! {
        <div class="card val-card">
            <h3 class="val-title">{ &g.label }</h3>
            <p class="muted small">
                { format!(
                    "{} actifs testés, {} de plus à l'entraînement · historique médian {} ans{peers}",
                    g.universe.basket,
                    g.universe.extra,
                    plain(g.universe.median_years, 1)
                ) }
            </p>
            { for g.horizons.iter().map(|h| horizon(h, p.req)) }
        </div>
    }
}

fn horizon(h: &V3Horizon, req: f64) -> Html {
    html! {
        <div key={h.horizon.to_string()} class="bot-v3-horizon">
            <p class="small">
                <b>{ format!("À {} jours", h.horizon) }</b>{ " " }
                <span class="muted">{ format!("· test de {} à {} · {} réentraînements", month_year(h.test_from), month_year(h.test_to), h.blocks) }</span>
            </p>
            { for h.configs.iter().filter(|c| c.headline).map(|c| {
                let runs = choice_runs(&h.selection, c.family == Family::Peers);
                headline_config(c, req, &runs)
            }) }
        </div>
    }
}

fn headline_config(c: &V3Config, req: f64, runs: &[Run<altim_core::engine::bot_v3::V3Candidate>]) -> Html {
    let m = &c.main;
    let pending: Vec<String> = [true, false].iter().filter_map(|buy| pending_text(c, *buy, req)).collect();
    let chosen = runs
        .iter()
        .map(|r| format!("{}{}", r.chosen.map(v3_short).unwrap_or("aucun"), if r.count > 1 { format!(" ({} fois)", r.count) } else { String::new() }))
        .collect::<Vec<_>>()
        .join(" → ");
    let vs = |x: bool| if x { " · face à la médiane" } else { "" };
    html! {
        <div key={c.id.clone()} class="bot-side">
            <p class="small"><b>{ family_label(c.family) }</b></p>
            <p class="small">{ v3_side_text(c.family, true, &m.buy, req) }</p>
            <p class="small bot-side-head"><span class="muted">{ format!("ACHETER{}", vs(m.buy_vs_mean.is_some())) }</span>{ " " }{ chip(&m.buy, req) }</p>
            if let Some(s) = &m.buy_vs_mean {
                { control(s, req, proven(m, true)) }
            }
            <p class="small">{ v3_side_text(c.family, false, &m.sell, req) }</p>
            <p class="small bot-side-head"><span class="muted">{ format!("VENDRE{}", vs(m.sell_vs_mean.is_some())) }</span>{ " " }{ chip(&m.sell, req) }</p>
            if let Some(s) = &m.sell_vs_mean {
                { control(s, req, proven(m, false)) }
            }
            { for pending.iter().map(|t| html! { <p key={t.clone()} class="notice warn small">{ t }</p> }) }
            if m.hold_assets > 0 {
                <p class="kv small">
                    <span>{ "Achats cumulés / détention (médianes)" }</span>
                    <b>{ format!("{} / {}", signed_pct(m.median_bot_return, 0), signed_pct(m.median_hold_return, 0)) }</b>
                </p>
            }
            <details class="small">
                <summary>{ "Modèle retenu à chaque réentraînement" }</summary>
                <p class="muted small">{ chosen }</p>
            </details>
        </div>
    }
}

/// Family B against the group's equal-weight mean (control added after the first real run), and what counts.
fn control(s: &SideStats, req: f64, ok: bool) -> Html {
    html! {
        <div class="bot-v3-control">
            <p class="small bot-side-head">
                <span class="muted">{ format!("Face à la moyenne du groupe (contrôle ajouté après coup) : {} ({})", points(s.excess), t_line(s, req)) }</span>
                { chip(s, req) }
            </p>
            <p class="small">
                <b>{ if ok { "Sur le passé : avantage face aux deux références." } else { "Retenu : non démontré (il faut les deux références) ; ne compte pas." } }</b>
            </p>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct ConfigsCardProps {
    pub title: AttrValue,
    pub configs: Vec<V3Config>,
    pub req: f64,
}

/// Every configuration of a group and horizon (headline included), compact rows.
#[component]
pub fn ConfigsCard(p: &ConfigsCardProps) -> Html {
    let req = p.req;
    html! {
        <div class="card val-card">
            <h3 class="val-title">{ p.title.clone() }</h3>
            <ul class="bot-cands">
                { for p.configs.iter().map(|c| {
                    let m = &c.main;
                    html! {
                        <li key={c.id.clone()}>
                            <p class="small bot-side-head">
                                <b>{ &c.label }</b>
                                if c.headline {
                                    <span class="chip">{ "principal" }</span>
                                }
                                if c.candidate.is_some() {
                                    <span class="chip muted">{ format!("retenu {} fois sur {}", c.chosen_blocks, c.trained_blocks) }</span>
                                }
                            </p>
                            <p class="kv small"><span>{ format!("{} achats", m.buy.signals) }</span><b>{ format!("{} ({})", points(m.buy.excess), t_vs_required(m.buy.t, req)) }</b></p>
                            <p class="kv small"><span>{ format!("{} ventes", m.sell.signals) }</span><b>{ format!("{} ({})", points(m.sell.excess), t_vs_required(m.sell.t, req)) }</b></p>
                            if m.buy_vs_mean.is_some() {
                                <p class="kv small">
                                    <span>{ "Face à la moyenne" }</span>
                                    <b>{ format!(
                                        "{} / {} (t {} / {})",
                                        points(judged(m, true).excess),
                                        points(judged(m, false).excess),
                                        plain(judged(m, true).t, 1),
                                        plain(judged(m, false).t, 1)
                                    ) }</b>
                                </p>
                            }
                            <p class="small bot-side-head">{ chip(&m.buy, req) }{ chip(&m.sell, req) }</p>
                        </li>
                    }
                }) }
            </ul>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct VolCardProps {
    pub label: AttrValue,
    pub v: VolManaged,
}

#[component]
pub fn VolCard(p: &VolCardProps) -> Html {
    let v = &p.v;
    html! {
        <div class="card val-card">
            <h3 class="val-title">{ p.label.clone() }</h3>
            <p class="muted small">{ format!("{} actifs du panier, de {} à {} · portefeuille à parts égales", v.assets, month_year(v.from), month_year(v.to)) }</p>
            <ul class="bot-vol">
                { for vol_rows(v).into_iter().map(|(label, managed, hold)| html! {
                    <li key={label} class="small">
                        <span>{ label }</span>
                        <span>{ "gérée " }<b>{ managed }</b></span>
                        <span class="muted">{ format!("détention {hold}") }</span>
                    </li>
                }) }
            </ul>
            <p class="small">{ &v.text }</p>
        </div>
    }
}
