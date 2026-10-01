//! The Bot report (BotReportView and its cards of Bot.tsx): headline and tiles, v3 first, today's views of the
//! watched assets, v2's selection, each candidate alone, last year and extra universe, calibration, asset by asset,
//! limits. Stacked cards and rows (no table), mobile first; the per-asset list keeps the basket's order.
use std::rc::Rc;

use altim_core::engine::bot::{BotAction, BotAssetRow, BotGroupStat, BotReport, BotStats, Bucket, CandidateStat};
use altim_core::engine::validation::Verdict;
use altim_core::js::{fr, number_to_string};
use altim_core::web::bot::fr_iso;
use altim_core::web::bot::screen::{
    AssetView, AssetViews, action_ui, buy_text, calibration_rows, candidate_short, clustered_of, clustered_text, data_text, exit_text, pct0, points,
    selection_runs, sell_text, skill_text, view_line, wait_text,
};
use altim_core::web::insights::validation::{class_short, month_year, plain, signed_pct, utc_day, verdict_tone};
use yew::prelude::*;

use super::v3::BotV3Section;
use super::v4::{BotV4Section, V4Avis};
use crate::route::use_on_link;

/// Plain space before "%" in this screen (`NB` of Bot.tsx).
const NB: &str = " ";

#[derive(Properties, PartialEq)]
pub struct ActionChipProps {
    pub action: Option<BotAction>,
}

/// ACHETER / ATTENDRE / VENDRE, or "pas d'avis".
#[component]
pub fn ActionChip(p: &ActionChipProps) -> Html {
    match p.action {
        None => html! { <span class="chip muted">{ "pas d'avis" }</span> },
        Some(a) => {
            let (label, tone) = action_ui(a);
            html! { <span class={classes!("chip", "bot-action", tone)}>{ label }</span> }
        }
    }
}

pub(super) fn verdict_chip(v: Option<Verdict>, label: String) -> Html {
    html! { <span class={classes!("chip", "val-verdict", v.map(verdict_tone).unwrap_or("info"))}>{ label }</span> }
}

fn chip(a: BotAction) -> Html {
    html! { <ActionChip action={Some(a)} /> }
}

fn plural(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}

fn tile(label: &str, value: String) -> Html {
    html! {
        <div class="val-tile">
            <span class="muted small">{ label.to_string() }</span>
            <b>{ value }</b>
        </div>
    }
}

fn reasons(list: &[String]) -> Html {
    html! { <ul class="reasons">{ for list.iter().map(|x| html! { <li key={x.clone()}>{ x }</li> }) }</ul> }
}

#[derive(Properties, PartialEq)]
pub struct BotReportViewProps {
    pub report: Rc<BotReport>,
}

#[component]
pub fn BotReportView(p: &BotReportViewProps) -> Html {
    let r = &p.report;
    let on_link = use_on_link();
    let pr = &r.parameters;
    let with_candidates: Vec<&BotGroupStat> = r.groups.iter().filter(|g| !g.candidates.is_empty()).collect();
    let v3 = r.v3.as_ref();
    let calculated = crate::ui::fr_date_time_short(r.as_of as f64);
    let failures = r.failures.len();
    let extra_failures = r.extra_failures.len();
    let timing = match (&r.timing, v3) {
        (Some(t), None) => format!(
            " Calcul : {} s de téléchargement, {} s d'entraînement et de test.",
            altim_core::js::round(t.fetch_ms as f64 / 1000.0),
            altim_core::js::round(t.compute_ms as f64 / 1000.0)
        ),
        _ => String::new(),
    };
    let extra_fixed = if r.extra_fixed_on.is_empty() { String::new() } else { format!(", univers élargi le {}", fr_iso(&r.extra_fixed_on)) };
    html! {
        <>
            <div class="card val-head">
                <p class="val-headline">{ &r.headline }</p>
                <div class="val-tiles">
                    { tile("Actifs testés", format!("{} / {}", r.assets.len(), r.assets.len() + failures)) }
                    { tile("Jours testés", fr(r.overall.labelled as f64, 0, 3)) }
                    { tile("Achats / ventes", format!("{} / {}", r.overall.buy.signals, r.overall.sell.signals)) }
                </div>
                <p class="muted small">
                    { format!(
                        "Horizon {} jours · seuil : fréquence d'entraînement + {} points · coûts d'un aller-retour {}{NB}% (actions), {}{NB}% (cryptos). Calculé le {calculated}, réentraîné toutes les 12 h.",
                        pr.horizon_days,
                        plain(Some(pr.threshold_margin), 0),
                        plain(Some(pr.cost_stock_pct), 2),
                        plain(Some(pr.cost_crypto_pct), 2)
                    ) }
                </p>
                if failures > 0 {
                    <div class="notice warn small">
                        { format!("{failures} actif{s} non utilisé{s} :", s = plural(failures)) }
                        <ul class="reasons">{ for r.failures.iter().map(|f| html! { <li key={f.symbol.clone()}><b>{ &f.symbol }</b>{ format!(" — {}", f.error) }</li> }) }</ul>
                    </div>
                }
                if extra_failures > 0 {
                    <p class="muted small">
                        { format!(
                            "Univers élargi : {extra_failures} actif{s} indisponible{s} ({}).",
                            r.extra_failures.iter().map(|f| f.symbol.as_str()).collect::<Vec<_>>().join(", "),
                            s = plural(extra_failures)
                        ) }
                    </p>
                }
            </div>

            if v3.is_some() {
                <BotV3Section report={p.report.clone()} />
            }
            if r.v4.is_some() {
                <BotV4Section report={p.report.clone()} />
            }

            if v3.is_none() && !r.changes.is_empty() {
                <div class="card">
                    <h2 class="card-title">{ "Ce qui change avec la v2" }</h2>
                    { reasons(&r.changes) }
                </div>
            }

            <WatchedViews />

            if let Some(v3) = v3 {
                <h2 class="section-label">{ "Référence : sélection v2 à 20 jours" }</h2>
                <p class="muted small">
                    { format!("Le modèle de la v2 (choisi par log-loss) refait sur les nouvelles données, jugé au seuil corrigé (t ≥ {}). C'est lui qui donne les probabilités de hausse et de baisse ci-dessous ; il ne compte dans aucune décision.", plain(Some(v3.t_required), 2)) }
                </p>
            }

            <div class="card">
                <h2 class="card-title">{ "Comment il apprend et comment il est jugé" }</h2>
                { reasons(&r.method) }
                <details class="small">
                    <summary>{ format!("Les {} mesures lues à chaque clôture", r.features.len()) }</summary>
                    <ul class="reasons">{ for r.features.iter().map(|f| html! { <li key={f.id.clone()}><b>{ &f.label }</b>{ format!(" — {}", f.help) }</li> }) }</ul>
                </details>
            </div>

            <h2 class="section-label">{ if v3.is_some() { "Sélection v2 : résultats hors échantillon" } else { "Résultats hors échantillon" } }</h2>
            <p class="muted small">{ "Modèle choisi à chaque réentraînement sur une validation interne (jamais sur le test), testé sur les actifs du panier." }</p>
            <div class="val-grid">{ for r.groups.iter().map(|g| html! { <GroupCard key={g.id.id()} g={g.clone()} /> }) }</div>

            if !with_candidates.is_empty() {
                <h2 class="section-label">{ "Chaque modèle seul" }</h2>
                <p class="muted small">{ "À titre d'information, non utilisé pour choisir : ce qu'aurait donné chaque candidat retenu partout, sur les mêmes jours." }</p>
                <div class="val-grid">{ for with_candidates.iter().map(|g| html! { <CandidatesCard key={g.id.id()} g={(*g).clone()} /> }) }</div>
            }

            if r.groups.iter().any(|g| g.holdout.is_some() || g.extra.is_some()) {
                <h2 class="section-label">{ "Dernière année et univers élargi" }</h2>
                <div class="val-grid">
                    { for r.groups.iter().flat_map(|g| {
                        let holdout = g.holdout.as_ref().map(|h| html! {
                            <SubResult
                                key={format!("{}-h", g.id.id())}
                                title={format!("{} · 12 derniers mois", g.label)}
                                note={format!("Du {} au {}, présentés à part (même modèle choisi ; rien n'est choisi sur cette période).", utc_day(h.from), utc_day(h.to))}
                                s={h.stats.clone()}
                            />
                        });
                        let extra = g.extra.as_ref().map(|x| html! {
                            <SubResult
                                key={format!("{}-x", g.id.id())}
                                title={format!("{} · actifs d'entraînement hors panier", g.label)}
                                note={format!("{} actifs fixés d'avance, hors du test principal ; eux aussi jugés hors échantillon.", g.universe.extra)}
                                s={x.clone()}
                            />
                        });
                        [holdout, extra].into_iter().flatten()
                    }) }
                </div>
            }

            <h2 class="section-label">{ "Calibration" }</h2>
            <p class="muted small">
                { "Quand le bot annonce une probabilité, la fréquence observée ensuite devrait être proche. Chaque ligne : jours de test dont la probabilité tombait dans la tranche." }
            </p>
            <div class="val-grid">
                { for r.groups.iter().flat_map(|g| [
                    html! { <Calibration key={format!("{}-up", g.id.id())} title={format!("{} · hausse", g.label)} buckets={g.stats.calibration_up.clone()} skill={g.stats.brier_skill_up} /> },
                    html! { <Calibration key={format!("{}-down", g.id.id())} title={format!("{} · baisse", g.label)} buckets={g.stats.calibration_down.clone()} skill={g.stats.brier_skill_down} /> },
                ]) }
            </div>

            <h2 class="section-label">{ "Actif par actif" }</h2>
            <p class="muted small">{ "Dans l'ordre du panier, jamais classés par performance. « Aujourd'hui » : avis du modèle retenu, entraîné sur tout l'historique connu." }</p>
            <ul class="val-assets">{ for r.assets.iter().map(|a| html! { <AssetRow key={a.symbol.clone()} a={a.clone()} /> }) }</ul>

            <div class="card">
                <h2 class="card-title">{ "Limites" }</h2>
                { reasons(&r.limits) }
                <p class="muted small">
                    { format!("Panier fixé le {}{extra_fixed}. Source : {}.{timing} ", fr_iso(&r.basket_fixed_on), r.source) }
                    <a href="/app/validation" onclick={on_link} class="link">{ "Voir la validation du signal →" }</a>
                </p>
            </div>
        </>
    }
}

#[derive(Properties, PartialEq)]
pub struct GroupCardProps {
    pub g: BotGroupStat,
}

#[component]
pub fn GroupCard(p: &GroupCardProps) -> Html {
    let g = &p.g;
    let s = &g.stats;
    let data = data_text(g);
    let exit = exit_text(Some(&s.sell.exit));
    let live = g.model.as_ref().map(|m| m.candidate);
    html! {
        <div class="card val-card">
            <h3 class="val-title">{ &g.label }</h3>
            <p class="muted small">
                { format!(
                    "{} actif{s} testé{s} · test du {} au {} · {} réentraînement{}",
                    g.assets,
                    month_year(g.test_from),
                    month_year(g.test_to),
                    g.trained_blocks,
                    plural(g.trained_blocks),
                    s = plural(g.assets)
                ) }
            </p>
            if let Some(d) = data {
                <p class="muted small">{ format!("Données : {d}.") }</p>
            }
            <div class="bot-side">
                <p class="small bot-side-head">{ chip(BotAction::Buy) }{ " " }{ verdict_chip(s.buy.verdict, s.buy.verdict_label.clone()) }</p>
                <p class="small">{ buy_text(&s.buy) }</p>
                if let Some(c) = clustered_of(&s.buy.clustered) {
                    <p class="muted small">{ clustered_text(Some(c), s.buy.t_stat) }</p>
                }
                <p class="kv small"><span>{ "Achats gagnants / jours gagnants" }</span><b>{ format!("{} / {}", pct0(s.buy.hit_rate), pct0(s.buy.baseline_hit_rate)) }</b></p>
                <p class="kv small"><span>{ "Achats cumulés / détention (médianes)" }</span><b>{ format!("{} / {}", signed_pct(s.buy.median_bot_return, 1), signed_pct(s.buy.median_hold_return, 1)) }</b></p>
            </div>
            <div class="bot-side">
                <p class="small bot-side-head">{ chip(BotAction::Sell) }{ " " }{ verdict_chip(s.sell.verdict, s.sell.verdict_label.clone()) }</p>
                <p class="small">{ sell_text(&s.sell) }</p>
                if let Some(c) = clustered_of(&s.sell.clustered) {
                    <p class="muted small">{ clustered_text(Some(c), s.sell.t_stat) }</p>
                }
                <p class="kv small"><span>{ "Suivies d'une baisse / tous les jours" }</span><b>{ format!("{} / {}", pct0(s.sell.fall_rate), pct0(s.sell.baseline_fall_rate)) }</b></p>
                <p class="kv small"><span>{ "Pire recul moyen ensuite / au hasard" }</span><b>{ format!("{} / {}", signed_pct(s.sell.mean_drawdown, 1), signed_pct(s.sell.baseline_drawdown, 1)) }</b></p>
                if let Some(e) = exit {
                    <p class="small">{ e }</p>
                }
            </div>
            <div class="bot-side">
                <p class="small bot-side-head">{ chip(BotAction::Wait) }</p>
                <p class="small">{ wait_text(&s.wait) }</p>
            </div>
            if !g.selection.is_empty() {
                <div class="bot-side">
                    <p class="small"><b>{ "Modèle retenu à chaque réentraînement" }</b></p>
                    <ul class="bot-runs">
                        { for selection_runs(&g.selection).iter().map(|x| html! {
                            <li key={x.from.to_string()} class="small">
                                <span class="muted">
                                    { month_year(Some(x.from)) }{ if x.count > 1 { format!(" → {} ({} fois)", month_year(Some(x.to)), x.count) } else { String::new() } }
                                </span>{ " " }
                                <b>{ x.chosen.map(candidate_short).unwrap_or("aucun (trop peu de données)") }</b>
                            </li>
                        }) }
                    </ul>
                    if let Some(c) = live {
                        <p class="muted small">{ format!("Aujourd'hui : {}, choisi de la même façon sur la dernière année connue.", candidate_short(c)) }</p>
                    }
                </div>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct CandidatesCardProps {
    pub g: BotGroupStat,
}

/// Each candidate's own out-of-sample result (for information, never used to choose): stacked rows, no table.
#[component]
pub fn CandidatesCard(p: &CandidatesCardProps) -> Html {
    html! {
        <div class="card val-card">
            <h3 class="val-title">{ &p.g.label }</h3>
            <ul class="bot-cands">{ for p.g.candidates.iter().map(candidate_row) }</ul>
        </div>
    }
}

/// "Achats : trop peu…" (the verdict label with a small first letter).
fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn candidate_row(c: &CandidateStat) -> Html {
    let s = &c.stats;
    html! {
        <li key={format!("{:?}", c.id)}>
            <p class="small bot-side-head">
                <b>{ &c.label }</b>{ " " }<span class="chip muted">{ format!("retenu {} fois sur {}", c.chosen_blocks, c.trained_blocks) }</span>
            </p>
            <p class="muted small">{ &c.description }</p>
            <p class="kv small"><span>{ format!("{} achats · écart au hasard", s.buy.signals) }</span><b>{ format!("{} (t {})", points(s.buy.excess), plain(s.buy.t_stat, 1)) }</b></p>
            <p class="kv small"><span>{ format!("{} ventes · baisse évitée", s.sell.signals) }</span><b>{ format!("{} (t {})", points(s.sell.avoided), plain(s.sell.t_stat, 1)) }</b></p>
            <p class="kv small">
                <span>{ "Précision hausse / baisse" }</span><b>{ format!("{}{NB}% / {}{NB}%", plain(s.brier_skill_up, 1), plain(s.brier_skill_down, 1)) }</b>
            </p>
            <p class="small bot-side-head">
                { verdict_chip(s.buy.verdict, format!("Achats : {}", lower_first(&s.buy.verdict_label))) }
                { verdict_chip(s.sell.verdict, format!("Ventes : {}", lower_first(&s.sell.verdict_label))) }
            </p>
        </li>
    }
}

#[derive(Properties, PartialEq)]
struct SubResultProps {
    title: AttrValue,
    note: AttrValue,
    s: BotStats,
}

/// The last 12 months, or the extra training assets: the same figures, shorter.
#[component]
fn SubResult(p: &SubResultProps) -> Html {
    let s = &p.s;
    html! {
        <div class="card val-card">
            <h3 class="val-title">{ p.title.clone() }</h3>
            <p class="muted small">{ p.note.clone() }</p>
            <p class="small bot-side-head">{ chip(BotAction::Buy) }{ " " }{ verdict_chip(s.buy.verdict, s.buy.verdict_label.clone()) }</p>
            <p class="small">{ buy_text(&s.buy) }</p>
            <p class="small bot-side-head">{ chip(BotAction::Sell) }{ " " }{ verdict_chip(s.sell.verdict, s.sell.verdict_label.clone()) }</p>
            <p class="small">{ sell_text(&s.sell) }</p>
            if let Some(e) = exit_text(Some(&s.sell.exit)) {
                <p class="muted small">{ e }</p>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct CalibrationProps {
    pub title: AttrValue,
    pub buckets: Vec<Bucket>,
    pub skill: Option<f64>,
}

/// Predicted vs realised per probability bucket: two thin bars per row, the numbers written next to them.
#[component]
pub fn Calibration(p: &CalibrationProps) -> Html {
    let width = |v: Option<f64>| format!("width: {}%;", number_to_string(v.unwrap_or(0.0).clamp(0.0, 100.0)));
    html! {
        <div class="card val-card">
            <h3 class="val-title">{ p.title.clone() }</h3>
            <p class="muted small">{ format!("Précision : {}.", skill_text(p.skill)) }</p>
            <ul class="bot-calib">
                { for calibration_rows(&p.buckets).into_iter().map(|b| html! {
                    <li key={b.label.clone()}>
                        <p class="small"><b>{ &b.label }</b>{ " " }<span class="muted">{ format!("· {} jours", fr(b.rows as f64, 0, 3)) }</span></p>
                        <p class="small">{ format!("prévu {} · observé ", pct0(b.predicted)) }<b>{ pct0(b.realised) }</b></p>
                        <div class="bot-bars" role="img" aria-label={format!("Prévu {}, observé {}", pct0(b.predicted), pct0(b.realised))}>
                            <span class="pred" style={width(b.predicted)} />
                            <span class="real" style={width(b.realised)} />
                        </div>
                    </li>
                }) }
            </ul>
            <p class="muted small"><span class="bot-key pred" />{ " prévu · " }<span class="bot-key real" />{ " observé" }</p>
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct AssetRowProps {
    pub a: BotAssetRow,
}

#[component]
pub fn AssetRow(p: &AssetRowProps) -> Html {
    let a = &p.a;
    let on_link = use_on_link();
    let mut test = format!("Test : {} achat{}", a.buys, plural(a.buys));
    if a.buy_excess.is_some() {
        test.push_str(&format!(" ({} vs hasard)", points(a.buy_excess)));
    }
    test.push_str(&format!(" · {} vente{}", a.sells, plural(a.sells)));
    if let Some(s) = a.sell_avoided {
        test.push_str(&format!(" (cours ensuite {} vs hasard)", points(Some(-s))));
    }
    test.push_str(&format!(
        " · attente {} · achats cumulés {}, détention {}",
        pct0(a.wait_share),
        signed_pct(a.bot_return, 1),
        signed_pct(a.hold_return, 1)
    ));
    if a.out_share.is_some() {
        test.push_str(&format!(
            " · hors marché après VENDRE {} du temps, pire baisse {} contre {} en gardant",
            pct0(a.out_share),
            signed_pct(a.bot_max_drawdown, 1),
            signed_pct(a.hold_max_drawdown, 1)
        ));
    }
    test.push_str(&format!(" ({}{})", a.source, a.years.map(|y| format!(", {} ans", plain(Some(y), 1))).unwrap_or_default()));
    html! {
        <li class="val-asset">
            <div class="val-asset-top">
                <a href={format!("/app/actif/{}/{}", a.kind.as_str(), a.symbol)} onclick={on_link} class="val-sym"><b>{ &a.symbol }</b>{ " " }<span class="muted small">{ &a.name }</span></a>
                <span class="chip muted">{ class_short(a.class) }</span>
                <ActionChip action={a.now.action} />
            </div>
            <p class="small">{ format!("Aujourd'hui : hausse {}, baisse {}", pct0(a.now.up), pct0(a.now.down)) }</p>
            if let Some(v) = &a.v3 {
                <p class="small bot-v3-now">
                    <span class="muted">{ "v3 :" }</span>{ " hausse/baisse 20 j " }<ActionChip action={v.absolute20} />{ " 60 j " }<ActionChip action={v.absolute60} />
                    { " · entre pairs 20 j " }<ActionChip action={v.peers20} />{ " 60 j " }<ActionChip action={v.peers60} />
                </p>
            }
            if let Some(v) = &a.v4 {
                <p class="small bot-v3-now">
                    <span class="muted">{ "Bots sélectifs :" }</span>
                    { format!(" {}", if v.bots.is_empty() { "pas d'avis".to_string() } else { v.bots.iter().map(|id| altim_core::engine::bot_v4::short_label(id)).collect::<Vec<_>>().join(", ") }) }
                </p>
            }
            <p class="muted small">{ test }</p>
        </li>
    }
}

/// Today's view of the watched assets (cached report only; nothing is trained here).
#[component]
fn WatchedViews() -> Html {
    let app = crate::state::app::use_app_state();
    let data = use_state(|| None::<Rc<AssetViews>>);
    let error = use_state(|| None::<String>);
    let items: Vec<(String, altim_core::types::Kind)> = app.watchlist.iter().map(|w| (w.symbol.clone(), w.kind)).collect();
    let key = items.iter().map(|(s, k)| format!("{s}:{}", k.as_str())).collect::<Vec<_>>().join(",");
    {
        let (data, error) = (data.clone(), error.clone());
        use_effect_with(key, move |_| {
            let alive = Rc::new(std::cell::Cell::new(true));
            if !items.is_empty() {
                let a = alive.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let r = crate::api::get::<AssetViews>(&altim_core::web::bot::bot_views_url(&items)).await;
                    if !a.get() {
                        return;
                    }
                    match r {
                        Ok(d) => data.set(Some(Rc::new(d))),
                        Err(e) => error.set(Some(e.0)),
                    }
                });
            }
            move || alive.set(false)
        });
    }
    if app.watchlist.is_empty() {
        return html! {};
    }
    html! {
        <div class="card">
            <h2 class="card-title">{ "Vos actifs aujourd'hui" }</h2>
            if let Some(e) = &*error {
                <p class="muted small">{ format!("Avis indisponibles ({e}).") }</p>
            }
            if data.is_none() && error.is_none() {
                <p class="muted small">{ "Chargement…" }</p>
            }
            if let Some(d) = &*data {
                <ul class="bot-views">{ for d.views.iter().map(|v| html! { <ViewRow key={format!("{}:{}", v.kind.as_str(), v.symbol)} v={v.clone()} /> }) }</ul>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct ViewRowProps {
    v: AssetView,
}

#[component]
fn ViewRow(p: &ViewRowProps) -> Html {
    let on_link = use_on_link();
    let (v, view) = (&p.v, &p.v.view);
    html! {
        <li>
            <div class="val-asset-top">
                <a href={format!("/app/actif/{}/{}", v.kind.as_str(), v.symbol)} onclick={on_link} class="val-sym"><b>{ &v.symbol }</b></a>
                <ActionChip action={view.action} />
                <span class={classes!("chip", (!view.counts).then_some("muted"))}>{ if view.counts { "compte" } else { "ne compte pas" } }</span>
            </div>
            <p class="small muted">{ view_line(view) }</p>
            if let Some(v3) = view.v3.as_ref().filter(|x| x.available) {
                <p class="small bot-v3-now">
                    <span class="muted">{ "v3 :" }</span>{ " " }
                    { for v3.signals.iter().map(|s| html! {
                        <span key={format!("{:?}-{}", s.family, s.horizon)} class="bot-v3-sig">
                            { format!("{} {} j ", if s.family == altim_core::engine::bot_v3::Family::Peers { "entre pairs" } else { "hausse/baisse" }, s.horizon) }
                            <ActionChip action={s.action} />
                        </span>
                    }) }
                </p>
            }
            if let Some(v4) = view.v4.as_ref() {
                <V4Avis v={v4.clone()} />
            }
        </li>
    }
}
