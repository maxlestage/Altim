//! Journal (Journal.tsx, journal-store.ts): every entry with its automatic
//! review, and the profile of the results. `JournalNote` and `JournalToggle` are used by the order sheets and by
//! « Mes avoirs », with `store::record_real_trade`.
pub mod store;

use std::collections::HashMap;
use std::rc::Rc;

use altim_core::js::{fr as js_fr, number_to_string, round};
use altim_core::types::{Candle, Kind};
use altim_core::web::trading::journal::{
    ClosedInfo, EntryReview, FirstHit, GroupStat, HorizonReview, JournalProfile, JournalSide, LevelSource, MIN_SAMPLE, REVIEW_DAYS, ReviewStatus,
    SOURCE_PAPER, journal_profile, review_entry,
};
use altim_core::web::trading::paper_ui::{fr_date, price as paper_price, reason_label};
use yew::prelude::*;

use crate::app::common::{PortfolioTab, PortfolioTabs};
use crate::route::use_on_link;
use store::{delete_journal_entry, set_journal_note, use_journal};

use altim_core::web::sorting::Sorting;
pub use store::record_real_trade;

fn fr(v: f64, d: usize) -> String {
    js_fr(v, 0, d)
}
fn signed(v: f64, d: usize) -> String {
    let s = if v > 0.0 {
        "+"
    } else if v < 0.0 {
        "−"
    } else {
        ""
    };
    format!("{s}{}", fr(v.abs(), d))
}
fn r_text(v: Option<f64>) -> String {
    v.map(|v| format!("{} R", signed(v, 2))).unwrap_or_else(|| "—".into())
}
fn pct_text(v: Option<f64>) -> String {
    v.map(|v| format!("{} %", signed(v, 1))).unwrap_or_else(|| "—".into())
}
fn step_icon(state: &str) -> &'static str {
    match state {
        "ok" => "✓",
        "no" => "✕",
        _ => "?",
    }
}
fn macro_label(level: Option<&str>) -> String {
    match level {
        Some("calm") => "calme".into(),
        Some("tense") => "tendu".into(),
        Some("high") => "très tendu".into(),
        Some(other) => other.into(),
        None => "null".into(),
    }
}

/// The value of the element an input event comes from (inputs and text areas alike).
pub fn target_value(e: &Event) -> String {
    e.target().and_then(|t| js_sys::Reflect::get(&t, &"value".into()).ok()).and_then(|v| v.as_string()).unwrap_or_default()
}

#[derive(Properties, PartialEq)]
pub struct JournalNoteProps {
    pub value: AttrValue,
    pub on_change: Callback<String>,
}

/// "Pourquoi je suis entré" (optional), in the order sheets.
#[component]
pub fn JournalNote(p: &JournalNoteProps) -> Html {
    let oninput = {
        let cb = p.on_change.clone();
        Callback::from(move |e: InputEvent| cb.emit(target_value(&e)))
    };
    html! {
        <label class="field">
            <span>{ "Pourquoi j'entre (facultatif, pour le journal)" }</span>
            <textarea rows="2" maxlength="1000" value={p.value.clone()} {oninput} placeholder="ex. rebond sur le support, objectif 1 visé" />
        </label>
    }
}

#[derive(Properties, PartialEq)]
pub struct JournalToggleProps {
    pub checked: bool,
    pub on_change: Callback<bool>,
    pub note: AttrValue,
    pub on_note: Callback<String>,
    pub text: AttrValue,
}

/// Checkbox "inscrire au journal" + the note, in « Mes avoirs ».
#[component]
pub fn JournalToggle(p: &JournalToggleProps) -> Html {
    let onchange = {
        let cb = p.on_change.clone();
        Callback::from(move |e: Event| cb.emit(e.target_unchecked_into::<web_sys::HtmlInputElement>().checked()))
    };
    html! {
        <div class="journal-toggle">
            <label class="check-row">
                <input type="checkbox" checked={p.checked} {onchange} />
                <span>{ p.text.clone() }</span>
            </label>
            if p.checked {
                <JournalNote value={p.note.clone()} on_change={p.on_note.clone()} />
            }
        </div>
    }
}

/// Daily candles by "kind:symbol": absent = loading, None = unavailable.
#[derive(Default, PartialEq)]
struct Candles(HashMap<String, Option<Rc<Vec<Candle>>>>);

impl Reducible for Candles {
    type Action = (String, Option<Rc<Vec<Candle>>>);
    fn reduce(self: Rc<Self>, (k, v): Self::Action) -> Rc<Self> {
        let mut m = self.0.clone();
        m.insert(k, v);
        Rc::new(Candles(m))
    }
}

/// Journal screen (/app/journal): every entry with its automatic review, and the profile of the results.
#[component]
pub fn Journal() -> Html {
    let on_link = use_on_link();
    let saved = use_journal();
    let paper = crate::app::simulation::store::use_paper();
    let entries = &saved.state.entries;
    let candles = use_reducer(Candles::default);
    let horizon = use_state(|| 10u32);
    let mut keys: Vec<String> = entries.iter().map(|e| format!("{}:{}", e.kind.as_str(), e.symbol)).collect();
    keys.sort_dyn();
    keys.dedup();
    let assets_key = keys.join(",");

    // Daily candles of every asset of the journal, once per opening (cached by the server).
    {
        let candles = candles.clone();
        use_effect_with(assets_key, move |assets_key| {
            let alive = Rc::new(std::cell::Cell::new(true));
            for k in assets_key.split(',').filter(|k| !k.is_empty()) {
                let Some((kind, symbol)) = k.split_once(':') else { continue };
                let Some(kind) = Kind::parse(kind) else { continue };
                let (candles, a, k, symbol) = (candles.clone(), alive.clone(), k.to_string(), symbol.to_string());
                wasm_bindgen_futures::spawn_local(async move {
                    let r = store::daily_candles(&symbol, kind).await;
                    if a.get() {
                        candles.dispatch((k, r.ok().map(Rc::new)));
                    }
                });
            }
            move || alive.set(false)
        });
    }

    let closed_by_id: HashMap<String, ClosedInfo> = paper
        .state
        .as_ref()
        .map(|p| {
            p.trades.iter().map(|t| (t.id.clone(), ClosedInfo { at: t.closed_at, price: t.exit, reason: reason_label(t.reason).into() })).collect()
        })
        .unwrap_or_default();
    let now = js_sys::Date::now();
    let loaded = |e: &altim_core::web::trading::journal::JournalEntry| candles.0.get(&format!("{}:{}", e.kind.as_str(), e.symbol)).cloned();
    let reviews: Vec<EntryReview> = entries
        .iter()
        .map(|e| {
            let c = loaded(e).flatten();
            let closed = if e.source == SOURCE_PAPER { e.ref_id.as_ref().and_then(|id| closed_by_id.get(id).cloned()) } else { None };
            review_entry(e, c.as_deref().map(|v| v.as_slice()).unwrap_or(&[]), now, closed)
        })
        .collect();
    let profile = journal_profile(&reviews, *horizon);
    let mut newest: Vec<&EntryReview> = reviews.iter().collect();
    newest.sort_by_dyn(|a, b| b.entry.created_at.partial_cmp(&a.entry.created_at).unwrap_or(std::cmp::Ordering::Equal));
    let n = entries.len();

    html! {
        <section class="app-screen journal-screen">
            <PortfolioTabs active={PortfolioTab::Journal} />
            <div class="screen-top">
                <div>
                    <h1>{ "Journal" }</h1>
                    <p class="muted small">{ format!("Enregistré uniquement dans ce navigateur · {n} entrée{}", if n > 1 { "s" } else { "" }) }</p>
                </div>
            </div>
            <p class="muted small">
                { "Chaque achat simulé et chaque achat ou vente réel enregistré dans « Mes avoirs » est noté ici avec la décision affichée à ce moment-là, puis revu automatiquement à 3, 10 et 30 jours sur les bougies journalières suivantes. Des faits, pas des jugements : un bon trade peut perdre, un mauvais peut gagner." }
            </p>
            if let Some(e) = &saved.error {
                <p class="notice warn" role="alert">{ format!("⚠ {e}") }</p>
            }

            if entries.is_empty() {
                <div class="card empty-card">
                    <h2>{ "Aucune entrée pour l'instant" }</h2>
                    <p class="muted">
                        { "Simulez un achat depuis la carte « Décision » d'un actif, ou enregistrez un achat ou une vente dans " }
                        <a href="/app/avoirs" onclick={on_link} class="link">{ "Mes avoirs" }</a>{ " : l'entrée apparaîtra ici." }
                    </p>
                </div>
            } else {
                <ProfileCard profile={Rc::new(profile)} horizon={*horizon} on_horizon={{ let h = horizon.clone(); Callback::from(move |d| h.set(d)) }} />
                <h2 class="section-label">{ "Entrées" }</h2>
                <ul class="journal-list">
                    { for newest.iter().map(|r| {
                        let l = loaded(&r.entry);
                        html! { <EntryCard key={r.entry.id.clone()} r={Rc::new((*r).clone())} loaded={l.map(|x| x.is_some())} /> }
                    }) }
                </ul>
            }
        </section>
    }
}

#[derive(Properties, PartialEq)]
struct ProfileCardProps {
    profile: Rc<JournalProfile>,
    horizon: u32,
    on_horizon: Callback<u32>,
}

#[component]
fn ProfileCard(p: &ProfileCardProps) -> Html {
    let profile = &p.profile;
    let h = p.horizon;
    let s = |n: usize| if n > 1 { "s" } else { "" };
    html! {
        <div class="card">
            <h2 class="card-title">{ "Votre profil" }</h2>
            <div class="agenda-chips" role="group" aria-label="Horizon">
                { for REVIEW_DAYS.iter().map(|d| {
                    let on = *d == h;
                    let click = { let cb = p.on_horizon.clone(); let d = *d; Callback::from(move |_| cb.emit(d)) };
                    html! { <button key={d.to_string()} class={classes!("chip", "pick", on.then_some("on"))} aria-pressed={on.to_string()} onclick={click}>{ format!("À {d} jours") }</button> }
                }) }
            </div>
            <p class="muted small">
                { format!("{} achat{} sur {} revu{} à {h} jours", profile.reviewed, s(profile.reviewed), profile.purchases, s(profile.reviewed)) }
                { if profile.pending > 0 { format!(" · {} pas encore à {h} jours", profile.pending) } else { String::new() } }
                { ". Résultats en R (multiples du risque pris, entrée − stop) et en pire recul : un gain obtenu en prenant plus de risque ne compte pas davantage. Sortie supposée au premier niveau touché (stop ou objectif 1), sinon au dernier cours." }
            </p>
            if let Some(all) = &profile.all {
                <GroupRows title="Ensemble" groups={vec![all.clone()]} />
                <GroupRows title="Par note à l'entrée" groups={profile.by_rating.clone()} />
                <GroupRows title="Plan respecté ou non" groups={profile.by_plan.clone()} />
                <GroupRows title="Par régime de marché" groups={profile.by_regime.clone()} />
            } else {
                <p class="muted small">{ format!("Aucun achat n'a encore {h} jours d'historique après l'entrée.") }</p>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct GroupRowsProps {
    title: AttrValue,
    groups: Vec<GroupStat>,
}

#[component]
fn GroupRows(p: &GroupRowsProps) -> Html {
    if p.groups.is_empty() {
        return html! {};
    }
    html! {
        <div class="journal-groups">
            <h3 class="small">{ p.title.clone() }</h3>
            <ul>
                { for p.groups.iter().map(|g| {
                    let avg = g.avg_r.unwrap_or(0.0);
                    let tone = if avg > 0.0 { "up" } else if avg < 0.0 { "down" } else { "" };
                    html! {
                        <li key={g.key.clone()} class="journal-group">
                            <div class="journal-group-head">
                                <b>{ g.label.clone() }</b>
                                <span class="muted small">
                                    { format!("{} achat{}{}", g.n, if g.n > 1 { "s" } else { "" }, if g.with_stop < g.n { format!(" ({} avec stop)", g.with_stop) } else { String::new() }) }
                                </span>
                                if g.low_sample {
                                    <span class="chip muted">{ format!("échantillon trop faible (< {MIN_SAMPLE})") }</span>
                                }
                            </div>
                            <div class="journal-figures">
                                <div><small>{ "Résultat moyen" }</small><b class={tone}>{ r_text(g.avg_r) }</b></div>
                                <div><small>{ "Médiane" }</small><b>{ r_text(g.median_r) }</b></div>
                                <div><small>{ "Positifs" }</small><b>{ g.win_rate.map(|w| format!("{} %", fr(w, 0))).unwrap_or_else(|| "—".into()) }</b></div>
                                <div><small>{ "Recul moyen" }</small><b>{ if g.avg_mae_r.is_some() { r_text(g.avg_mae_r) } else { pct_text(g.avg_mae_pct) } }</b></div>
                                <div><small>{ "Pire recul" }</small><b>{ r_text(g.worst_mae_r) }</b></div>
                                <div><small>{ "Variation moy." }</small><b>{ pct_text(g.avg_return_pct) }</b></div>
                            </div>
                        </li>
                    }
                }) }
            </ul>
        </div>
    }
}

fn horizon_row(h: &HorizonReview, buy: bool) -> Html {
    let days = format!("{} j", h.days);
    match h.status {
        ReviewStatus::Pending => {
            return html! { <li key={h.days.to_string()} class="muted small"><b>{ days }</b>{ format!(" : disponible le {}.", fr_date(h.available_at, false)) }</li> };
        }
        ReviewStatus::NoData => {
            return html! { <li key={h.days.to_string()} class="muted small"><b>{ days }</b>{ " : pas de bougie journalière après l'entrée dans l'historique disponible." }</li> };
        }
        ReviewStatus::Ready => {}
    }
    let fd = h.first_days.map(number_to_string).unwrap_or_default();
    let first = match h.first {
        FirstHit::Stop => format!("stop touché en {fd} j"),
        FirstHit::Target1 => format!("objectif 1 atteint en {fd} j{}", if h.target2 { ", puis objectif 2" } else { "" }),
        FirstHit::None => "ni stop ni objectif".into(),
    };
    let mfe_r = h.mfe_r.map(|r| format!(" ({})", r_text(Some(r)))).unwrap_or_default();
    let mae_r = h.mae_r.map(|r| format!(" ({})", r_text(Some(r)))).unwrap_or_default();
    html! {
        <li key={h.days.to_string()} class="small">
            <b>{ days }</b>
            { format!(" : {} au dernier cours · plus haut {}{mfe_r} · plus bas {}{mae_r}", pct_text(h.return_pct), pct_text(h.mfe_pct), pct_text(h.mae_pct)) }
            if buy {
                { format!(" · {first}") }
                if let Some(r) = h.result_r {
                    { " · résultat selon le plan " }<b>{ r_text(Some(r)) }</b>
                }
            }
        </li>
    }
}

#[derive(Properties, PartialEq)]
struct EntryCardProps {
    r: Rc<EntryReview>,
    /// None: loading; Some(false): unavailable; Some(true): loaded.
    loaded: Option<bool>,
}

#[component]
fn EntryCard(p: &EntryCardProps) -> Html {
    let m = crate::money::use_money();
    let on_link = use_on_link();
    let r = &p.r;
    let e = &r.entry;
    let d = e.decision.as_ref();
    let editing = use_state(|| false);
    let note = use_state(|| e.note.clone());
    let buy = e.side == JournalSide::Buy;
    let mk = &e.market;
    let cls = match r.coherent {
        None => None,
        Some(true) => Some("buy"),
        Some(false) => Some("hold"),
    };
    let fmt = |v: f64| paper_price(&m, v);
    let save = {
        let (editing, note, id) = (editing.clone(), note.clone(), e.id.clone());
        Callback::from(move |_| {
            set_journal_note(&id, &note);
            editing.set(false);
        })
    };
    let cancel = {
        let (editing, note, original) = (editing.clone(), note.clone(), e.note.clone());
        Callback::from(move |_| {
            note.set(original.clone());
            editing.set(false);
        })
    };
    let edit = {
        let editing = editing.clone();
        Callback::from(move |_| editing.set(true))
    };
    let remove = {
        let (id, name) = (e.id.clone(), e.name.clone());
        Callback::from(move |_| {
            let ok = web_sys::window()
                .and_then(|w| w.confirm_with_message(&format!("Supprimer cette entrée du journal ({name}) ?")).ok())
                .unwrap_or(false);
            if ok {
                delete_journal_entry(&id);
            }
        })
    };
    let stress = mk
        .macro_score
        .map(|s| format!("{}/100 ({})", number_to_string(round(s)), macro_label(mk.macro_level.as_deref())))
        .unwrap_or_else(|| "inconnu".into());
    html! {
        <li class={classes!("card", "journal-entry", cls)}>
            <div class="journal-head">
                <a href={format!("/app/actif/{}/{}", e.kind.as_str(), e.symbol)} onclick={on_link} class="holding-name">
                    <b>{ format!("{} · {}", if buy { "Achat" } else { "Vente" }, e.name) }</b>
                    <small class="muted mono">{ format!("{} · {}", e.symbol, fr_date(e.created_at, true)) }</small>
                </a>
                <span class={classes!("chip", (e.source != SOURCE_PAPER).then_some("muted"))}>{ if e.source == SOURCE_PAPER { "simulé" } else { "réel" } }</span>
            </div>
            <div class="journal-figures">
                <div><small>{ "Prix" }</small><b class="mono">{ fmt(e.price) }</b></div>
                if buy {
                    <div>
                        <small>{ format!("Stop{}", if r.levels.stop_source == LevelSource::Plan { " (plan)" } else { "" }) }</small>
                        <b class="mono">{ r.levels.stop.map(fmt).unwrap_or_else(|| "aucun".into()) }</b>
                    </div>
                    <div>
                        <small>{ format!("Objectifs{}", if r.levels.targets_source == LevelSource::Plan { " (plan)" } else { "" }) }</small>
                        <b class="mono">{ if r.levels.targets.is_empty() { "aucun".into() } else { r.levels.targets.iter().map(|t| fmt(*t)).collect::<Vec<_>>().join(" · ") } }</b>
                    </div>
                    <div><small>{ "Gain visé" }</small><b>{ r_text(r.plan_r) }</b></div>
                }
            </div>
            <p class="small"><span class="muted">{ "Signal utilisé : " }</span>{ e.signal.clone() }</p>

            <details class="journal-details">
                <summary>{ "Pourquoi, et le contexte de marché" }</summary>
                if let Some(d) = d {
                    <p class="small">
                        <b>{ d.rating_label.clone().unwrap_or_else(|| d.label.clone()) }</b>
                        { format!(" · confiance {}/100", number_to_string(round(d.confidence))) }
                        { d.score.map(|s| format!(" · score {}/100", number_to_string(round(s)))).unwrap_or_default() }
                        { format!(" · décision du {}", fr_date(d.as_of, true)) }
                    </p>
                    <p class="small">{ d.headline.clone() }</p>
                    if d.degraded {
                        <p class="notice warn small">{ format!("⚠ Signal dégradé : {}", d.degraded_headline.clone().unwrap_or_default()) }</p>
                    }
                    if !d.vetoes.is_empty() {
                        <p class="notice danger small">{ format!("⛔ Interdictions d'achat actives : {}", d.vetoes.join(", ")) }</p>
                    }
                    if !d.pros.is_empty() {
                        <ul class="reasons">{ for d.pros.iter().map(|x| html! { <li key={x.clone()}>{ format!("＋ {x}") }</li> }) }</ul>
                    }
                    if !d.cons.is_empty() {
                        <ul class="reasons">{ for d.cons.iter().map(|x| html! { <li key={x.clone()}>{ format!("－ {x}") }</li> }) }</ul>
                    }
                    <p class="small muted">{ format!("Configuration « {} » : {}/{}", d.setup.name, number_to_string(d.setup.met), number_to_string(d.setup.total)) }</p>
                    <ul class="dec-chips">
                        { for d.setup.steps.iter().map(|s| html! {
                            <li key={s.label.clone()} class="dec-chip"><span aria-hidden="true">{ step_icon(&s.state) }</span>{ format!(" {}", s.label) }</li>
                        }) }
                    </ul>
                    if let Some(pl) = &d.plan {
                        <p class="small muted">
                            { format!(
                                "Plan : zone {} – {}, stop {}, objectif 1 {}{}, rapport gain / risque {}.",
                                fmt(altim_core::web::trading::js_min(pl.zone_from, pl.zone_to)),
                                fmt(altim_core::web::trading::js_max(pl.zone_from, pl.zone_to)),
                                fmt(pl.stop),
                                fmt(pl.target1),
                                pl.target2.map(|t| format!(", objectif 2 {}", fmt(t))).unwrap_or_default(),
                                fr(pl.risk_reward, 1)
                            ) }
                        </p>
                    }
                } else {
                    <p class="small muted">{ "Aucune décision chargée pour cet actif à ce moment-là." }</p>
                }
                <ul class="dec-chips">
                    <li class="dec-chip">{ format!("Régime : {}", mk.regime_label.clone().unwrap_or_else(|| "inconnu".into())) }</li>
                    <li class="dec-chip">{ format!("Stress macro : {stress}") }</li>
                    <li class="dec-chip">{ format!("ATR : {}", mk.atr_pct.map(|v| format!("{} %/jour", fr(v, 2))).unwrap_or_else(|| "inconnu".into())) }</li>
                    <li class="dec-chip">{ format!("Volume relatif : {}", mk.relative_volume.map(|v| format!("{}×", fr(v, 2))).unwrap_or_else(|| "inconnu".into())) }</li>
                    <li class="dec-chip">{ format!("Événements à 7 j : {}", mk.events.map(number_to_string).unwrap_or_else(|| "inconnu".into())) }</li>
                </ul>
            </details>

            if *editing {
                <div>
                    <JournalNote value={AttrValue::from((*note).clone())} on_change={{ let note = note.clone(); Callback::from(move |v| note.set(v)) }} />
                    <div class="row-actions">
                        <button class="btn btn-small" onclick={save}>{ "Enregistrer" }</button>
                        <button class="btn btn-small btn-ghost" onclick={cancel}>{ "Annuler" }</button>
                    </div>
                </div>
            } else {
                <p class="small">
                    <span class="muted">{ if buy { "Pourquoi je suis entré : " } else { "Pourquoi j'ai vendu : " } }</span>
                    if e.note.is_empty() {
                        <i class="muted">{ "pas de note" }</i>
                    } else {
                        { e.note.clone() }
                    }
                    { " " }
                    <button class="link-btn" onclick={edit}>{ if e.note.is_empty() { "Ajouter" } else { "Modifier" } }</button>
                </p>
            }

            <div class="journal-review">
                <h3 class="small">{ "Revue automatique" }</h3>
                if p.loaded == Some(false) {
                    <p class="notice warn small">{ "⚠ Cours journaliers indisponibles pour l'instant : revue non calculée." }</p>
                }
                if p.loaded.is_none() {
                    <p class="muted small">{ "Chargement des cours…" }</p>
                }
                <ul class="reasons">{ for r.horizons.iter().map(|h| horizon_row(h, buy)) }</ul>
                if let Some(c) = &r.closed {
                    <p class="small">
                        { format!("Clôturée le {} ({}) à {}", fr_date(c.at, false), c.reason, fmt(c.price)) }
                        if let Some(rr) = r.realized_r {
                            { " : " }<b>{ r_text(Some(rr)) }</b>{ " réalisé" }
                        }
                        { "." }
                    </p>
                }
                if !r.worked.is_empty() {
                    <h4 class="small">{ "Qu'est-ce qui a fonctionné ?" }</h4>
                    <ul class="insights">
                        { for r.worked.iter().map(|t| html! { <li key={t.clone()} class="insight good"><span aria-hidden="true">{ "✔" }</span><span>{ t.clone() }</span></li> }) }
                    </ul>
                }
                if !r.failed.is_empty() {
                    <h4 class="small">{ "Qu'est-ce qui n'a pas fonctionné ?" }</h4>
                    <ul class="insights">
                        { for r.failed.iter().map(|t| html! { <li key={t.clone()} class="insight warning"><span aria-hidden="true">{ "⚠" }</span><span>{ t.clone() }</span></li> }) }
                    </ul>
                }
                <h4 class="small">{ "Le signal était-il cohérent avec les données disponibles à ce moment-là ?" }</h4>
                <p class="small">
                    <b>{ match r.coherent { None => "Non vérifiable", Some(true) => "Oui", Some(false) => "Non" } }</b>
                    { if r.coherent == Some(false) { " : au moins un point ci-dessous ne l'était pas." } else { "" } }
                </p>
                <ul class="journal-checks">
                    { for r.coherence.iter().map(|c| html! {
                        <li key={c.code} class="small">
                            <span aria-hidden="true">{ match c.ok { None => "?", Some(true) => "✓", Some(false) => "✕" } }</span>
                            <span><b>{ c.label.clone() }</b>{ format!(" — {}", c.detail) }</span>
                        </li>
                    }) }
                </ul>
            </div>
            <div class="holding-actions">
                <button class="link-btn danger" onclick={remove}>{ "Supprimer" }</button>
            </div>
        </li>
    }
}
