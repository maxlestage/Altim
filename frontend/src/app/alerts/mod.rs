//! « Alertes » (Alerts.tsx, the phones' Alerts tab): what can be bought now (same rule as the notifications), the
//! price alerts chosen on the asset pages, the notifications of this browser, and the journal of the alerts with
//! what each gave. Store and checks: `store`, `notify`; the asset page's button: `PriceAlertButton`.
mod notify;
mod sheet;
mod store;

use std::collections::HashMap;
use std::rc::Rc;

use altim_core::types::Kind;
use altim_core::web::insights::alerts::{
    BuyAlert, CHECK_MS, PriceTarget, change_since, journal_summary, rearm, target_label, target_status, threshold_text, uniq,
};
use altim_core::web::store::asset_key;
use yew::prelude::*;

pub use notify::{run_check, start_checks};
pub use sheet::PriceAlertButton;
pub(crate) use store::news as news_of;
pub use store::{alerts_state, set_alerts, use_alerts};

use crate::live::use_live;
use crate::route::use_on_link;
use crate::ui::Change;
use altim_core::web::sorting::Sorting;

/// "30 sept., 14:05" (`toLocaleString("fr-FR", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })`).
fn fr_time(ms: f64) -> String {
    let o = js_sys::Object::new();
    for (k, v) in [("day", "numeric"), ("month", "short"), ("hour", "2-digit"), ("minute", "2-digit")] {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v.into());
    }
    js_sys::Date::new(&ms.into()).to_locale_string("fr-FR", &o).into()
}

#[component]
pub fn Alerts() -> Html {
    let s = use_alerts();
    let app = crate::state::app::use_app_state();
    let stored = crate::state::holdings::use_stored_holdings();
    let _m = crate::money::use_money();
    let on_link = use_on_link();
    let key = |(s, k): &(String, Kind)| asset_key(s, *k);
    let mine: Vec<(String, Kind)> = {
        let held = stored.assets();
        let all: Vec<(String, Kind)> = app.watchlist.iter().chain(held.iter()).map(|w| (w.symbol.clone(), w.kind)).collect();
        uniq(&all, key)
    };
    let mine_key = mine.iter().map(key).collect::<Vec<_>>().join(",");
    let buyable = use_state(|| None::<Rc<Vec<BuyAlert>>>);
    let error = use_state(|| None::<String>);
    let perm = use_state(notify::permission);
    let check_error = use_state(|| None::<String>);
    let tracked: Vec<(String, Kind)> = {
        let all: Vec<(String, Kind)> =
            s.targets.iter().map(|t| (t.symbol.clone(), t.kind)).chain(s.journal.iter().map(|e| (e.symbol.clone(), e.kind))).collect();
        uniq(&all, key)
    };
    let tracked_key = tracked.iter().map(key).collect::<Vec<_>>().join(",");
    let live = use_live(tracked.clone());
    let quotes = use_state(|| Rc::new(HashMap::<String, f64>::new()));

    {
        let (buyable, error, mine) = (buyable.clone(), error.clone(), mine.clone());
        use_effect_with(mine_key, move |_| {
            let alive = Rc::new(std::cell::Cell::new(true));
            if mine.is_empty() {
                buyable.set(Some(Rc::new(Vec::new())));
            } else {
                let a = alive.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let r = store::buy_alerts(&mine).await;
                    if !a.get() {
                        return;
                    }
                    match r {
                        Ok(list) => {
                            let mut list: Vec<BuyAlert> = list.into_iter().filter(|x| x.buy).collect();
                            list.sort_by_key_dyn(|x| !x.strong);
                            buyable.set(Some(Rc::new(list)));
                            error.set(None);
                        }
                        Err(e) => error.set(Some(e.0)),
                    }
                });
            }
            move || alive.set(false)
        });
    }
    {
        let (quotes, tracked) = (quotes.clone(), tracked.clone());
        use_effect_with(tracked_key, move |_| {
            if !tracked.is_empty() {
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(q) = store::quotes(&tracked).await {
                        quotes.set(Rc::new(q.iter().map(|x| (asset_key(&x.symbol, x.kind), x.price)).collect()));
                    }
                });
            }
        });
    }

    let price = |symbol: &str, kind: Kind| live.get(symbol, kind).map(|t| t.price).or_else(|| quotes.get(&asset_key(symbol, kind)).copied());
    let mut prices: HashMap<String, f64> = (**quotes).clone();
    for (sym, k) in &tracked {
        if let Some(t) = live.get(sym, *k) {
            prices.insert(asset_key(sym, *k), t.price);
        }
    }
    let summary = journal_summary(&s.journal, &prices, js_sys::Date::now(), 3_600_000.0);

    let turn_on = |buy: bool| {
        let (s, perm, check_error) = (s.clone(), perm.clone(), check_error.clone());
        Callback::from(move |_: Event| {
            let next = !if buy { s.notify.buy } else { s.notify.news };
            let (perm, check_error) = (perm.clone(), check_error.clone());
            wasm_bindgen_futures::spawn_local(async move {
                if next && *perm != "granted" {
                    perm.set(notify::ask_permission().await);
                }
                set_alerts(|cur| if buy { cur.notify.buy = next } else { cur.notify.news = next });
                if next {
                    check_error.set(run_check(js_sys::Date::now()).await);
                }
            });
        })
    };
    let toggle_strong = Callback::from(|_: Event| set_alerts(|cur| cur.notify.strong_only = !cur.notify.strong_only));
    let clear = Callback::from(|_| {
        let ok = web_sys::window().and_then(|w| w.confirm_with_message("Vider le journal des alertes ?").ok()).unwrap_or(false);
        if ok {
            set_alerts(|cur| cur.journal.clear());
        }
    });

    let last = s.last_check.filter(|t| *t != 0.0).map(|t| format!(" Dernière vérification : {}.", fr_time(t))).unwrap_or_default();
    html! {
        <section class="app-screen">
            <a href="/app" onclick={on_link.clone()} class="back">{ "← Radar" }</a>
            <div class="screen-top"><h1>{ "Alertes" }</h1></div>

            <div class="card">
                <h2 class="card-title">{ "Notifications de ce navigateur" }</h2>
                if !notify::notifications_supported() {
                    <p class="notice warn small">{ "Ce navigateur n'affiche pas de notifications : les alertes restent visibles ici et dans le journal." }</p>
                } else if *perm == "denied" {
                    <p class="notice warn small">{ "Notifications refusées pour Altim : autorisez-les dans les réglages du site de votre navigateur." }</p>
                }
                <label class="check-row">
                    <input type="checkbox" checked={s.notify.buy} onchange={turn_on(true)} />
                    <span>{ "Me prévenir quand je peux acheter (radar et avoirs)" }</span>
                </label>
                if s.notify.buy {
                    <label class="check-row">
                        <input type="checkbox" checked={s.notify.strong_only} onchange={toggle_strong} />
                        <span>{ "Seulement les achats conseillés (signal + zone)" }</span>
                    </label>
                }
                <label class="check-row">
                    <input type="checkbox" checked={s.notify.news} onchange={turn_on(false)} />
                    <span>{ "Me prévenir des actualités importantes" }</span>
                </label>
                <p class="muted small">
                    { format!("Vérifié toutes les {} minutes tant qu'un onglet Altim est ouvert (même en arrière-plan) : un navigateur fermé ne peut pas prévenir, contrairement aux applications iPhone et Android. Les alertes de prix sont vérifiées de la même façon.", CHECK_MS / 60_000) }
                    { last }
                </p>
                if let Some(e) = &*check_error {
                    <p class="notice warn small">{ format!("⚠ Vérification impossible : {e}. Nouvel essai à la prochaine vérification.") }</p>
                }
            </div>

            <div class="card">
                <h2 class="card-title">{ "Achetables maintenant" }</h2>
                { match (&*error, &*buyable) {
                    (Some(e), _) => html! { <p class="notice warn small">{ format!("⚠ {e}") }</p> },
                    (None, None) => html! { <p class="muted small">{ "Vérification du radar et des avoirs…" }</p> },
                    (None, Some(list)) if list.is_empty() => html! {
                        <p class="muted small">{ "Rien d'achetable pour l'instant : aucune décision complète ne dit ACHETER ou ZONE D'ACHAT." }</p>
                    },
                    (None, Some(list)) => html! {
                        <ul class="alert-list">
                            { for list.iter().map(|a| html! {
                                <li key={a.asset_key()}>
                                    <a href={format!("/app/actif/{}/{}", a.kind.as_str(), a.symbol)} onclick={on_link.clone()} class="alert-row">
                                        <span>
                                            <b class="mono">{ &a.symbol }</b>{ " " }
                                            <span class={classes!("badge", if a.strong { "buy" } else { "hold" })}>{ if a.strong { "ACHETER" } else { "ZONE D'ACHAT" } }</span>
                                        </span>
                                        <span class="mono">{ a.price.map(crate::money::price).unwrap_or_else(|| "—".into()) }</span>
                                    </a>
                                    <ul class="reasons small">{ for a.reasons.iter().chain(a.cautions.iter()).map(|r| html! { <li key={r.clone()}>{ r }</li> }) }</ul>
                                </li>
                            }) }
                        </ul>
                    },
                } }
            </div>

            <div class="card">
                <h2 class="card-title">{ "Alertes de prix" }</h2>
                if s.targets.is_empty() {
                    <p class="muted small">
                        { format!("Aucune alerte de prix. Sur la fiche d'un actif, bouton « Alerte de prix » : « préviens-moi si BTC passe sous 80 000 {} ».", crate::money::symbol()) }
                    </p>
                }
                <ul class="alert-list">
                    { for s.targets.iter().map(|t| {
                        let now_price = price(&t.symbol, t.kind);
                        let rearm_cb = {
                            let (id, d) = (t.id.clone(), crate::money::display());
                            Callback::from(move |_| {
                                let d = d.clone();
                                let id = id.clone();
                                set_alerts(move |cur| {
                                    cur.targets = cur.targets.iter().map(|x| if x.id == id { rearm(x, now_price, &d) } else { x.clone() }).collect()
                                })
                            })
                        };
                        let remove = {
                            let id = t.id.clone();
                            Callback::from(move |_| {
                                let id = id.clone();
                                set_alerts(move |cur| cur.targets.retain(|x| x.id != id))
                            })
                        };
                        html! {
                            <li key={t.id.clone()}>
                                <TargetRow t={t.clone()} price={now_price} />
                                <div class="row-actions">
                                    if t.triggered.is_some() {
                                        <button class="link-btn" onclick={rearm_cb}>{ "Réarmer" }</button>
                                    }
                                    <button class="link-btn danger" onclick={remove}>{ "Supprimer" }</button>
                                </div>
                            </li>
                        }
                    }) }
                </ul>
            </div>

            <div class="card">
                <h2 class="card-title">{ "Journal des alertes" }</h2>
                if let Some(sum) = summary {
                    <p class="small">
                        { format!("Depuis leur envoi, {} alerte(s) d'achat sur {} sont en hausse ({} %), variation moyenne ", sum.up, sum.count, altim_core::js::round(sum.up_share)) }
                        <Change value={Some(sum.average)} />{ "." }
                    </p>
                    <p class="muted small">{ "Mesure simple depuis chaque alerte, sans frais ni règle de sortie, sur vos propres alertes : une indication, pas une preuve. Les alertes de moins d'une heure et vos alertes de prix ne comptent pas." }</p>
                }
                if s.journal.is_empty() {
                    <p class="muted small">{ "Les alertes reçues s'afficheront ici, avec ce qu'elles ont donné depuis." }</p>
                }
                <ul class="alert-list">
                    { for s.journal.iter().take(100).map(|e| html! {
                        <li key={e.id.clone()}>
                            <a href={format!("/app/actif/{}/{}", e.kind.as_str(), e.symbol)} onclick={on_link.clone()} class="alert-row">
                                <span class="small">{ &e.title }<br /><small class="muted">{ format!("{} · {}", fr_time(e.date), crate::money::price(e.price)) }</small></span>
                                <Change value={change_since(e, price(&e.symbol, e.kind))} />
                            </a>
                        </li>
                    }) }
                </ul>
                if !s.journal.is_empty() {
                    <button class="link-btn danger" onclick={clear}>{ "Vider le journal" }</button>
                }
            </div>
        </section>
    }
}

#[derive(Properties, PartialEq)]
struct TargetRowProps {
    t: PriceTarget,
    price: Option<f64>,
}

#[component]
fn TargetRow(p: &TargetRowProps) -> Html {
    let m = crate::money::use_money();
    let on_link = use_on_link();
    let t = &p.t;
    let label = target_label(t, threshold_text);
    // Compared in the alert's own currency (the dollar price converted at the current rate).
    let shown = p.price.map(|x| m.convert(x, altim_core::web::money::Currency::Usd, t.currency)).filter(|x| x.is_finite());
    html! {
        <a href={format!("/app/actif/{}/{}", t.kind.as_str(), t.symbol)} onclick={on_link} class="alert-row">
            <span class="small">
                <b>{ &t.symbol }</b>{ " · " }{ label.to_lowercase() }
                <br />
                <small class={if t.triggered.is_some() { "up" } else { "muted" }}>{ target_status(t, shown, fr_time) }</small>
            </span>
        </a>
    }
}
