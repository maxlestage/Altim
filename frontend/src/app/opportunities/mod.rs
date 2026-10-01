//! Opportunités (Opportunities.tsx): the selection's universe scanned by
//! category (`/api/opportunities`), with the reason for each asset; filters saved in "altim.opportunities.v1".
use std::cell::Cell;
use std::rc::Rc;

use altim_core::js::{fr, number_to_string, round};
use altim_core::types::Kind;
use altim_core::web::trading::opportunities::{
    OPP_CATEGORIES, OPPORTUNITIES_KEY, OppCategory, OppFilters, OpportunityReport, compact_usd, count_by_category, filter_items, opportunities_url,
    read_saved, saved_json,
};
use yew::prelude::*;

use crate::api::{Pending, get_pending};
use crate::route::use_on_link;
use crate::state::{local_get, local_set};
use crate::ui::Segmented;

/// Thresholds in dollars (the sources' currency), labelled in the display currency at render time.
const CAPS_USD: [Option<f64>; 4] = [None, Some(1e10), Some(5e10), Some(2e11)];
const RANKS: [(&str, Option<f64>); 4] = [("Tous", None), ("Top 20", Some(20.0)), ("Top 50", Some(50.0)), ("Top 100", Some(100.0))];
const LIQ_USD: [Option<f64>; 4] = [None, Some(1e6), Some(1e7), Some(1e8)];
const VOL: [(&str, Option<f64>); 4] = [("Toutes", None), ("≤ 2 % / jour", Some(2.0)), ("≤ 4 % / jour", Some(4.0)), ("≤ 8 % / jour", Some(8.0))];

fn caps() -> Vec<(String, Option<f64>)> {
    CAPS_USD.iter().map(|v| (v.map(|v| format!("≥ {}", crate::money::compact(v))).unwrap_or_else(|| "Toutes".into()), *v)).collect()
}

fn liq() -> Vec<(String, Option<f64>)> {
    LIQ_USD.iter().map(|v| (v.map(|v| format!("≥ {} / jour", crate::money::compact(v))).unwrap_or_else(|| "Toutes".into()), *v)).collect()
}

fn fixed(list: &[(&str, Option<f64>)]) -> Vec<(String, Option<f64>)> {
    list.iter().map(|(l, v)| (l.to_string(), *v)).collect()
}

fn fr1(v: f64) -> String {
    fr(v, 0, 1)
}

fn signed(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr1(v.abs()))
}

#[derive(Properties, PartialEq)]
struct SelectProps {
    label: AttrValue,
    value: Option<f64>,
    options: Vec<(String, Option<f64>)>,
    on_change: Callback<Option<f64>>,
}

#[component]
fn Select(p: &SelectProps) -> Html {
    let key = |v: Option<f64>| v.map(number_to_string).unwrap_or_default();
    let onchange = {
        let cb = p.on_change.clone();
        Callback::from(move |e: Event| {
            let v = e.target_unchecked_into::<web_sys::HtmlSelectElement>().value();
            cb.emit(if v.is_empty() { None } else { Some(altim_core::web::trading::js_number(&v)) });
        })
    };
    html! {
        <label class="field opp-field">
            <span>{ p.label.clone() }</span>
            <select value={key(p.value)} {onchange}>
                { for p.options.iter().map(|(l, v)| html! {
                    <option key={l.clone()} value={key(*v)} selected={*v == p.value}>{ l.clone() }</option>
                }) }
            </select>
        </label>
    }
}

/// Opportunities of the moment: the selection's universe scanned by category, with the reason for each asset.
#[component]
pub fn Opportunities() -> Html {
    let m = crate::money::use_money();
    let on_link = use_on_link();
    let saved = use_state(|| read_saved(local_get(OPPORTUNITIES_KEY).as_deref()));
    let (market, filters) = (saved.0, saved.1.clone());
    let report = use_state(|| None::<Rc<OpportunityReport>>);
    let error = use_state(|| None::<String>);
    let pending = use_state(|| false);

    use_effect_with((*saved).clone(), |(market, filters)| local_set(OPPORTUNITIES_KEY, &saved_json(*market, filters)));

    {
        let (report, error, pending) = (report.clone(), error.clone(), pending.clone());
        use_effect_with(market, move |market| {
            let alive = Rc::new(Cell::new(true));
            report.set(None);
            error.set(None);
            let (a, url) = (alive.clone(), opportunities_url(*market));
            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    match get_pending::<OpportunityReport>(&url).await {
                        Ok(Pending::Pending) if a.get() => {
                            pending.set(true);
                            gloo_timers::future::TimeoutFuture::new(5_000).await;
                            if !a.get() {
                                return;
                            }
                        }
                        Ok(Pending::Ready(r)) if a.get() => {
                            pending.set(false);
                            report.set(Some(Rc::new(r)));
                            return;
                        }
                        Err(e) if a.get() => {
                            error.set(Some(e.0));
                            return;
                        }
                        _ => return,
                    }
                }
            });
            move || alive.set(false)
        });
    }

    let set_filters = {
        let saved = saved.clone();
        Callback::from(move |f: Box<dyn Fn(&mut OppFilters)>| {
            let (mk, mut fl) = (saved.0, saved.1.clone());
            f(&mut fl);
            saved.set((mk, fl));
        })
    };
    let shown = report.as_ref().map(|r| filter_items(&r.items, &filters)).unwrap_or_default();
    let counts = report.as_ref().map(|r| count_by_category(&r.items, &filters));
    let info = |c: OppCategory| report.as_ref().and_then(|r| r.categories.iter().find(|x| x.id == c));
    let limited: Vec<_> = report
        .as_ref()
        .map(|r| r.categories.iter().filter(|c| filters.categories.contains(&c.id) && (c.note.is_some() || c.error.is_some())).cloned().collect())
        .unwrap_or_default();
    let crypto = market == Kind::Crypto;

    let on_market = {
        let saved = saved.clone();
        Callback::from(move |v: AttrValue| saved.set((if v == "crypto" { Kind::Crypto } else { Kind::Stock }, saved.1.clone())))
    };
    let chips = OPP_CATEGORIES.iter().enumerate().map(|(k, c)| {
        let on = filters.categories.contains(c);
        let toggle = {
            let (set, c) = (set_filters.clone(), *c);
            Callback::from(move |_| {
                set.emit(Box::new(move |f: &mut OppFilters| {
                    if f.categories.contains(&c) {
                        f.categories.retain(|x| *x != c);
                    } else {
                        f.categories.push(c);
                    }
                }))
            })
        };
        let count = counts.map(|n| format!(" · {}", n[k])).unwrap_or_default();
        html! {
            <button key={c.as_str()} class={classes!("chip", "pick", on.then_some("on"))} aria-pressed={on.to_string()} onclick={toggle} title={info(*c).map(|i| i.rule.clone())}>
                { format!("{}{count}", c.short()) }
            </button>
        }
    });
    let select = |label: &'static str, value: Option<f64>, options: Vec<(String, Option<f64>)>, apply: fn(&mut OppFilters, Option<f64>)| {
        let set = set_filters.clone();
        let on_change = Callback::from(move |v: Option<f64>| set.emit(Box::new(move |f: &mut OppFilters| apply(f, v))));
        html! { <Select {label} {value} {options} {on_change} /> }
    };

    html! {
        <section class="app-screen opportunities">
            <div class="screen-top">
                <div>
                    <h1>{ "Opportunités du moment" }</h1>
                    <p class="muted small">{ "Ce qui bouge de façon notable aujourd'hui, catégorie par catégorie, avec la raison mesurée. Des pistes à examiner, pas des ordres d'achat." }</p>
                </div>
            </div>

            <Segmented label="Marché" value={AttrValue::from(market.as_str())} options={vec![(AttrValue::from("stock"), AttrValue::from("Actions")), (AttrValue::from("crypto"), AttrValue::from("Cryptos"))]} on_change={on_market} />

            <div class="card opp-filters">
                <h2 class="card-title">{ "Catégories" }</h2>
                <div class="opp-chips" role="group" aria-label="Catégories">{ for chips }</div>
                <div class="opp-selects">
                    if crypto {
                        { select("Rang (capitalisation)", filters.max_rank, fixed(&RANKS), |f, v| f.max_rank = v) }
                    } else {
                        { select("Capitalisation", filters.min_cap, caps(), |f, v| f.min_cap = v) }
                    }
                    { select("Liquidité (volume échangé)", filters.min_liquidity, liq(), |f, v| f.min_liquidity = v) }
                    { select("Volatilité max (ATR)", filters.max_volatility, fixed(&VOL), |f, v| f.max_volatility = v) }
                </div>
                { for limited.iter().map(|c| html! {
                    <p key={c.id.as_str()} class={classes!("small", if c.error.is_some() { "notice warn" } else { "muted" })}>
                        <b>{ c.id.short() }</b>{ " : " }{ c.error.clone().or_else(|| c.note.clone()).unwrap_or_default() }
                    </p>
                }) }
            </div>

            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e}") }</p>
            }
            if report.is_none() && error.is_none() {
                <div class="card">
                    <p class="muted">{ if *pending { format!("Analyse des {} en cours (environ 30 secondes la première fois)…", if crypto { "120 cryptos" } else { "150 actions" }) } else { "Chargement…".into() } }</p>
                    <div class="skeleton" />
                </div>
            }

            if let Some(r) = &*report {
                <h2 class="section-label">{ format!("Résultats · {} sur {} analysé{}", shown.len(), r.scanned, if r.scanned > 1 { "s" } else { "" }) }</h2>
                if shown.is_empty() {
                    <p class="muted">{ "Aucun actif ne remplit ces critères en ce moment." }</p>
                }
                <ul class="opp-list">
                    { for shown.iter().map(|i| {
                        let href = format!("/app/actif/{}/{}", market.as_str(), i.symbol);
                        let sub = if crypto { i.rank.filter(|r| *r != 0.0).map(|r| format!("rang {}", number_to_string(r))).unwrap_or_else(|| "crypto".into()) } else { i.sector.clone() };
                        html! {
                            <li key={i.symbol.clone()} class="card opp-card">
                                <div class="opp-head">
                                    <div class="opp-id">
                                        <a href={href.clone()} onclick={on_link.clone()}><b>{ i.name.clone() }</b></a>
                                        <small class="muted">{ format!("{} · {sub}", i.symbol) }</small>
                                    </div>
                                    <div class="opp-price mono">
                                        <b>{ crate::money::price(i.price) }</b>
                                        if let Some(ch) = i.change1d {
                                            <small class={if ch >= 0.0 { "buy" } else { "sell" }}>{ signed(ch) }</small>
                                        }
                                    </div>
                                </div>
                                <ul class="opp-hits">
                                    { for i.hits.iter().map(|h| html! {
                                        <li key={h.category.as_str()}><span class="chip">{ h.category.short() }</span>{ " " }<span class="small">{ h.reason.clone() }</span></li>
                                    }) }
                                </ul>
                                <p class="muted small opp-metrics">
                                    if let Some(v) = i.rsi14 { <span>{ format!("RSI 14 : {}", number_to_string(round(v))) }</span> }
                                    if let Some(v) = i.volatility { <span>{ format!("volatilité {} %/j", fr1(v)) }</span> }
                                    if let Some(v) = i.liquidity { <span>{ format!("échangé {}/j", compact_usd(&m, v)) }</span> }
                                    if let Some(v) = i.market_cap { <span>{ format!("capitalisation {}", compact_usd(&m, v)) }</span> }
                                </p>
                                <a class="btn btn-small" {href} onclick={on_link.clone()}>{ "Voir la fiche" }</a>
                            </li>
                        }
                    }) }
                </ul>

                <details class="card">
                    <summary>{ "Règles, sources et limites" }</summary>
                    <ul class="small opp-rules">
                        { for r.categories.iter().map(|c| html! {
                            <li key={c.id.as_str()}>
                                <b>{ c.label.clone() }</b>{ format!(" ({} analysé{}) : {}", c.analyzed, if c.analyzed > 1 { "s" } else { "" }, c.rule) }
                                if let Some(n) = c.note.as_ref().filter(|n| !n.is_empty()) { <span class="muted">{ format!(" {n}") }</span> }
                                if let Some(e) = c.error.as_ref().filter(|e| !e.is_empty()) { <span class="sell">{ format!(" {e}") }</span> }
                            </li>
                        }) }
                        { for r.not_covered.iter().map(|n| html! {
                            <li key={n.label.clone()}><b>{ format!("Non couvert — {}", n.label) }</b>{ format!(" : {}.", n.reason) }</li>
                        }) }
                    </ul>
                    <p class="muted small">{ format!("{}. Sources : {}. Séances closes uniquement.", r.universe, r.source) }</p>
                </details>
                <p class="muted small">
                    { format!("Scan calculé à {}. Conseil indicatif, pas une recommandation personnalisée : Altim ne passe aucun ordre.", crate::ui::fr_time_seconds(r.as_of)) }
                </p>
            }
        </section>
    }
}
