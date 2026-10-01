//! Sélection (Selection.tsx): which stocks or cryptos to buy, ranked by
//! what was measured to work, checked, with an entry plan and an amount from the budget (typed in the display
//! currency, saved with it in "altim.selection.budget").
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::engine::screener::{Criterion, Edge, HORIZON_LIST, Horizon};
use altim_core::js::{fr, fr_sig, number_to_string, round};
use altim_core::types::Kind;
use altim_core::web::money::{Currency, symbol};
use altim_core::web::store::WatchItem;
use altim_core::web::trading::selection::{
    BUDGET_KEY, HORIZON_KEY, MARKET_KEY, ORDER, SelectionCandidate, SelectionReport, allocate, budget_json, initial_budget, parse_budget, pct,
    pick_quantity, rank_label, rank_score, rank_text, read_horizon, read_market, selection_url, typed_budget, usd,
};
use wasm_bindgen::JsValue;
use yew::prelude::*;

use crate::api::{Pending, get_pending};
use crate::live::{LiveBadge, LivePrice, LiveTick, use_live};
use crate::route::use_on_link;
use crate::state::app::{set_app_state, use_app_state};
use crate::state::holdings::use_holdings;
use crate::state::{local_get, local_set};
use crate::ui::Segmented;

/// "3 septembre 2024" (`toLocaleDateString("fr-FR", { day: "numeric", month: "long", year: "numeric" })`).
fn fr_long_date(ms: f64) -> String {
    let o = js_sys::Object::new();
    for (k, v) in [("day", "numeric"), ("month", "long"), ("year", "numeric")] {
        let _ = js_sys::Reflect::set(&o, &k.into(), &v.into());
    }
    js_sys::Date::new(&JsValue::from_f64(ms)).to_locale_date_string("fr-FR", &o).into()
}

#[derive(Properties, PartialEq)]
struct BarProps {
    value: f64,
}

#[component]
fn Bar(p: &BarProps) -> Html {
    let tone = if p.value >= 70.0 {
        "good"
    } else if p.value >= 40.0 {
        "mid"
    } else {
        "low"
    };
    html! {
        <div class="crit-bar" role="img" aria-label={format!("{} sur 100", number_to_string(round(p.value)))}>
            <i class={tone} style={format!("width: {}%;", number_to_string(3f64.max(p.value)))} />
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct PickCardProps {
    c: SelectionCandidate,
    report: Rc<SelectionReport>,
    live: Option<LiveTick>,
    amount: Option<f64>,
}

#[component]
fn PickCard(p: &PickCardProps) -> Html {
    let m = crate::money::use_money();
    let on_link = use_on_link();
    let c = &p.c;
    let open = use_state(|| c.rank <= 3);
    let state = use_app_state();
    let report = &p.report;
    let kind = report.market;
    let in_radar = state.watchlist.iter().any(|w| w.key() == altim_core::web::store::asset_key(&c.symbol, kind));
    let rank_by = report.rank_by;
    let price = p.live.as_ref().map(|t| t.price).unwrap_or(c.price);
    let plan = c.plan.as_ref();
    let buy_at = plan.and_then(|pl| pl.limit).unwrap_or(price);
    // Whole shares for a stock; cryptos are divisible.
    let qty = pick_quantity(p.amount, buy_at, kind);
    let loss = plan.filter(|_| qty != 0.0).map(|pl| qty * (buy_at - pl.stop));
    let href = format!("/app/actif/{}/{}", kind.as_str(), c.symbol);
    let toggle = {
        let open = open.clone();
        Callback::from(move |_| open.set(!*open))
    };
    let add = {
        let item = WatchItem { symbol: c.symbol.clone(), kind, name: c.name.clone() };
        Callback::from(move |_| {
            let item = item.clone();
            set_app_state(move |s| s.watchlist.push(item))
        })
    };
    let amount_text = || {
        if qty > 0.0 {
            let what = if kind == Kind::Stock {
                format!("{} action{}", number_to_string(qty), if qty > 1.0 { "s" } else { "" })
            } else {
                format!("{} {}", fr_sig(qty, 6), c.symbol)
            };
            let loss = loss.filter(|l| *l != 0.0 && !l.is_nan()).map(|l| format!(" · perte max ≈ {} au stop", usd(&m, l))).unwrap_or_default();
            format!("{what}{loss}")
        } else {
            "moins d'une action : fractionnée chez votre courtier".into()
        }
    };
    html! {
        <li class="card pick">
            <div class="pick-head">
                <span class="pick-rank" aria-label={format!("Rang {}", c.rank)}>{ c.rank }</span>
                <div class="pick-id">
                    <a href={href.clone()} onclick={on_link.clone()}><b>{ c.name.clone() }</b></a>
                    <small class="muted">{ format!("{} · {}", c.symbol, c.sector) }</small>
                </div>
                <div class="pick-price mono">
                    <b><LivePrice tick={p.live.clone()} fallback={Some(c.price)} format={Callback::from(crate::money::price)} /></b>
                    <small class="muted">{ format!("{} {}/100", rank_label(rank_by, report.rank_rule), number_to_string(rank_score(c, rank_by, report.rank_rule))) }</small>
                </div>
            </div>

            if let Some(plan) = plan {
                <div class="pick-plan">
                    <div>
                        <small>{ "Entrée" }</small>
                        <b>{ match plan.limit.filter(|l| *l != 0.0 && !l.is_nan()) { Some(l) => format!("ordre limite {}", usd(&m, l)), None => format!("maintenant ≈ {}", usd(&m, price)) } }</b>
                        if plan.limit.is_some_and(|l| l != 0.0 && !l.is_nan()) {
                            <small class="muted">{ format!("ou maintenant ≈ {}", usd(&m, price)) }</small>
                        }
                    </div>
                    <div><small>{ "Stop" }</small><b class="sell">{ usd(&m, plan.stop) }</b><small class="muted">{ pct((plan.stop / buy_at - 1.0) * 100.0) }</small></div>
                    <div><small>{ "Objectif" }</small><b class="buy">{ usd(&m, plan.target) }</b><small class="muted">{ pct((plan.target / buy_at - 1.0) * 100.0) }</small></div>
                    if let Some(amount) = p.amount.filter(|a| *a > 0.0) {
                        <div class="pick-amount">
                            <small>{ "Montant suggéré" }</small>
                            <b>{ usd(&m, amount) }</b>
                            <small class="muted">{ amount_text() }</small>
                        </div>
                    }
                </div>
            }

            <button class="link-btn pick-toggle" aria-expanded={open.to_string()} onclick={toggle}>{ if *open { "Masquer le détail" } else { "Pourquoi celle-ci ? Le détail" } }</button>
            if *open {
                <div class="pick-detail">
                    <ul class="crit-list">
                        { for ORDER.iter().map(|k: &Criterion| {
                            let score = *c.scores.get(*k);
                            html! {
                                <li key={format!("{k:?}")} class={if *k == rank_by { "main" } else { "" }}>
                                    <div class="crit-head">
                                        <span>{ report.criteria.get(*k).clone() }</span>
                                        <small class="crit-role">{ report.roles.get(*k).clone() }</small>
                                        <b class="mono">{ number_to_string(round(score)) }</b>
                                    </div>
                                    <Bar value={score} />
                                    <small class="muted">{ c.why.get(*k).clone() }</small>
                                </li>
                            }
                        }) }
                    </ul>
                    <ul class="checks">
                        { for c.checks.iter().map(|x| html! {
                            <li key={x.label.clone()} class={if x.ok { "ok" } else { "ko" }}>
                                <span aria-hidden="true">{ if x.ok { "✔" } else { "⚠" } }</span>{ " " }<b>{ x.label.clone() }</b>{ format!(" : {}", x.detail) }
                            </li>
                        }) }
                        if let Some(t) = &c.track {
                            <li class="info">
                                <span aria-hidden="true">{ "ℹ" }</span>{ " " }<b>{ "Signaux d'Altim sur ce titre" }</b>
                                { format!(" : {} achats passés, {} % gagnants, {} en moyenne", number_to_string(t.trades), number_to_string(round(t.win_rate)), pct(t.avg_return)) }
                            </li>
                        }
                    </ul>
                    <div class="pick-actions">
                        <a class="btn btn-small" {href} onclick={on_link}>{ "Voir la fiche complète" }</a>
                        if !in_radar {
                            <button class="link-btn" onclick={add}>{ "+ Ajouter au radar" }</button>
                        }
                    </div>
                </div>
            }
        </li>
    }
}

/// Which stocks or cryptos to buy: ranked by what was measured to work, checked, with an entry plan and an amount.
#[component]
pub fn Selection() -> Html {
    let m = crate::money::use_money();
    let on_link = use_on_link();
    let state = use_app_state();
    let risk = state.risk;
    let horizon = use_state(|| read_horizon(local_get(HORIZON_KEY).as_deref()));
    let market = use_state(|| read_market(local_get(MARKET_KEY).as_deref()));
    let holdings = use_holdings();
    let cash = holdings.cash;
    let report = use_state(|| None::<Rc<SelectionReport>>);
    let error = use_state(|| None::<String>);
    let pending = use_state(|| false);
    // The typed budget keeps the currency it was typed in (a rate arriving later never re-reads it in another one).
    let budget_input = {
        let m = m.clone();
        use_state(move || initial_budget(parse_budget(local_get(BUDGET_KEY).as_deref()), cash, &m))
    };
    let (budget_text, budget_currency) = (*budget_input).clone();
    let typed = typed_budget(&budget_text);
    let budget = {
        let b = m.convert(typed, budget_currency, Currency::Usd);
        if b.is_nan() { 0.0 } else { b }
    };
    let wealth = cash + holdings.holdings.iter().map(|h| h.quantity * h.average_price).sum::<f64>();

    {
        let (report, error, pending) = (report.clone(), error.clone(), pending.clone());
        use_effect_with((*horizon, *market), move |(horizon, market)| {
            let alive = Rc::new(Cell::new(true));
            report.set(None);
            error.set(None);
            let (a, url) = (alive.clone(), selection_url(*horizon, *market));
            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    match get_pending::<SelectionReport>(&url).await {
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
    use_effect_with((*horizon, *market), |(horizon, market)| {
        local_set(MARKET_KEY, market.as_str());
        local_set(HORIZON_KEY, horizon.as_str());
    });
    use_effect_with((typed.to_bits(), budget_currency), move |_| {
        if typed > 0.0 {
            local_set(BUDGET_KEY, &budget_json(typed, budget_currency));
        }
    });

    let live_items: Vec<(String, Kind)> =
        report.as_ref().map(|r| r.buy.iter().chain(r.watch.iter()).map(|c| (c.symbol.clone(), r.market)).collect()).unwrap_or_default();
    let live = use_live(live_items);
    let amounts: HashMap<String, f64> = match report.as_ref() {
        Some(r) if budget > 0.0 => allocate(&r.buy, budget, risk.max_position_percent),
        _ => HashMap::new(),
    };
    let crypto = *market == Kind::Crypto;
    let h = *horizon;

    let on_budget = {
        let budget_input = budget_input.clone();
        Callback::from(move |e: InputEvent| {
            let text = e.target_unchecked_into::<web_sys::HtmlInputElement>().value();
            budget_input.set((text, crate::money::currency()));
        })
    };
    let on_market = {
        let market = market.clone();
        Callback::from(move |v: AttrValue| market.set(if v == "crypto" { Kind::Crypto } else { Kind::Stock }))
    };
    let placeholder = if wealth > 0.0 {
        let v = m.convert(cash, Currency::Usd, budget_currency);
        number_to_string(round(if v.is_nan() || v == 0.0 { 0.0 } else { v }))
    } else {
        "10000".into()
    };
    let rank = report.as_ref().map(|r| rank_text(r.rank_rule)).unwrap_or("…");
    let evidence = report.as_ref().map(|r| r.evidence.clone()).unwrap_or_default();
    let scanned = |default: usize| report.as_ref().map(|r| r.scanned).unwrap_or(default);

    let validation = report.as_ref().and_then(|r| r.validation.clone().map(|v| (r.clone(), v))).map(|(r, v)| {
        let limits = if matches!(h, Horizon::M30 | Horizon::H1 | Horizon::H5) {
            format!(
                "Durées courtes : rejouées sur {} seulement ; à ces échelles les prix sont surtout du bruit et les frais pèsent lourd. Ce n'est pas une garantie.",
                if crypto { "quelques jours à 3 semaines" } else { "60 jours" }
            )
        } else if crypto {
            "Limites honnêtes : l'historique ne couvre qu'environ 2 ans et demi (les plateformes gardent 1 000 jours), et la liste est celle des cryptos qui existent encore aujourd'hui, ce qui embellit les chiffres. Les cryptos restent très risquées. Ce n'est pas une garantie.".into()
        } else {
            "Limite honnête : la liste est celle des plus grandes sociétés d'aujourd'hui, qui ont par définition réussi, ce qui gonfle ces chiffres. Entre fin 2021 et 2023, la force relative n'a presque rien apporté ; l'essentiel de l'avance vient de 2023–2026. Ce n'est pas une garantie.".into()
        };
        let from = fr_long_date(v.from.unwrap_or(f64::NAN));
        html! {
            <div class={classes!("card", "validation", if v.edge == Edge::Clear { "good" } else { "weak" })}>
                <h2 class="card-title">{ "Ce que cette méthode aurait donné" }</h2>
                if v.edge == Edge::None {
                    <p class="notice danger small">
                        <b>{ format!("Pas d'avance mesurée pour {}.", h.label()) }</b>
                        { format!(" Une fois les frais payés ({} % l'aller-retour), ce classement n'a pas fait mieux que de choisir au hasard. Il est affiché à titre indicatif : ne misez pas dessus.", fr(v.cost, 0, 2)) }
                    </p>
                }
                if v.edge == Edge::Weak {
                    <p class="notice warn small">
                        <b>{ "Avance faible et irrégulière." }</b>
                        { " En moyenne la sélection a fait mieux, mais seulement environ une fois sur deux : quelques très bons choix tirent la moyenne. Une durée plus longue est plus fiable." }
                    </p>
                }
                <p>
                    { "Rejouée " }<b>{ format!("{} fois", v.periods) }</b>{ format!(" depuis {from} (sélection de 10, gardée {}) :", r.hold_text) }
                    <b>{ format!(" {}", pct(v.top)) }</b>{ " en moyenne pour la sélection contre " }<b>{ pct(v.universe) }</b>
                    { format!(" pour l'ensemble des {} {}", r.scanned, if crypto { "cryptos" } else { "actions" }) }
                    if let Some(b) = v.benchmark {
                        { " et " }<b>{ pct(b) }</b>{ " pour le simple achat de Bitcoin" }
                    }
                    { " ; la sélection a fait mieux que l'ensemble " }<b>{ format!("{} % du temps", number_to_string(round(v.beat_rate))) }</b>{ "." }
                </p>
                if crypto && v.top < 0.0 {
                    <p class="notice warn small">{ "Sur cette période, la sélection a perdu moins que les autres cryptos, mais elle a quand même perdu : quand presque toutes les cryptos baissent, bien choisir limite la casse sans l'éviter." }</p>
                }
                <p class="muted small">{ limits }</p>
            </div>
        }
    });

    html! {
        <section class="app-screen selection">
            <div class="screen-top">
                <div>
                    <h1>{ if crypto { "Quelles cryptos acheter" } else { "Quelles actions acheter" } }</h1>
                    <p class="muted small">
                        { if crypto { "Les 120 plus grandes cryptos (hors stablecoins et jetons adossés)" } else { "Les plus grandes sociétés américaines" } }
                        { ", classées et vérifiées pour votre horizon." }
                    </p>
                </div>
                <LiveBadge status={live.status} last={live.last} />
            </div>

            <a class="notice opp-link" href="/app/opportunites" onclick={on_link.clone()}>
                <b>{ "Opportunités du moment →" }</b>{ " retournements, cassures, volumes anormaux, survendus, fondamentaux qui évoluent." }
            </a>

            <Segmented label="Marché" value={AttrValue::from(market.as_str())} options={vec![(AttrValue::from("stock"), AttrValue::from("Actions")), (AttrValue::from("crypto"), AttrValue::from("Cryptos"))]} on_change={on_market} />

            <div class="horizon-grid" role="radiogroup" aria-label="Durée de détention">
                { for HORIZON_LIST.iter().map(|x| {
                    let on = *x == h;
                    let set = { let horizon = horizon.clone(); let x = *x; Callback::from(move |_| horizon.set(x)) };
                    html! { <button key={x.as_str()} role="radio" aria-checked={on.to_string()} class={if on { "on" } else { "" }} onclick={set}>{ x.label() }</button> }
                }) }
            </div>

            <div class="card method">
                <h2 class="card-title">{ "Comment Altim choisit" }</h2>
                <ol>
                    if crypto {
                        <li><b>{ format!("{} cryptos", scanned(110)) }</b>{ " parmi les 120 plus grandes (classement CoinGecko), sans stablecoins ni jetons adossés (WBTC, stETH, or…)." }</li>
                        <li><b>{ format!("Classement : {rank}") }</b>{ ", le critère qui a le mieux marché sur le passé pour cette durée. " }{ evidence.clone() }</li>
                        <li><b>{ "Pas de limite par secteur" }</b>{ ", mais les cryptos bougent souvent ensemble : le montant par ligne reste plafonné." }</li>
                    } else {
                        <li><b>{ format!("{} grandes actions", scanned(150)) }</b>{ " analysées chaque jour (capitalisation, secteur : Nasdaq)." }</li>
                        <li><b>{ format!("Classement : {rank}") }</b>{ ", le critère qui a le mieux marché sur le passé pour cette durée. " }{ evidence.clone() }</li>
                        <li><b>{ "Au plus 3 actions par secteur" }</b>{ ", pour ne pas tout miser sur un seul thème." }</li>
                    }
                    <li><b>{ "Vérifications" }</b>{ " de chaque finaliste : prix recoupés sur plusieurs sources, garde-fou marché, tendance de fond. Un titre qui échoue passe « à surveiller »." }</li>
                    <li><b>{ "Plan" }</b>{ format!(" pour une détention de {} : prix d'entrée (zone d'achat Fibonacci), stop selon la volatilité de la période, objectif à 2 fois le risque, montant.", h.label()) }</li>
                </ol>
            </div>

            if report.as_ref().is_some_and(|r| r.market_closed) {
                <p class="notice warn small">{ "Bourse de New York fermée : ce classement vient de la dernière séance ; il changera à la réouverture." }</p>
            }

            { validation.unwrap_or_default() }

            <div class="card budget">
                <label class="field">
                    <span>{ format!("Budget à investir ({})", symbol(budget_currency)) }</span>
                    <input inputmode="decimal" value={budget_text.clone()} {placeholder} oninput={on_budget} />
                </label>
                <p class="muted small">
                    { format!("Réparti pour que chaque ligne risque la même somme si son stop est touché (une action volatile reçoit moins), sans dépasser {} % du budget par ligne (Réglages).", number_to_string(risk.max_position_percent)) }
                </p>
            </div>

            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e}") }</p>
            }
            if report.is_none() && error.is_none() {
                <div class="card">
                    <p class="muted">{ if *pending { format!("Analyse des {} en cours (environ 30 secondes la première fois)…", if crypto { "120 cryptos" } else { "150 actions" }) } else { "Chargement de la sélection…".into() } }</p>
                    <div class="skeleton" />
                </div>
            }

            if let Some(r) = &*report {
                <h2 class="section-label">{ format!("À acheter · {}", r.buy.len()) }</h2>
                <ol class="pick-list">
                    { for r.buy.iter().map(|c| html! {
                        <PickCard key={c.symbol.clone()} c={c.clone()} report={r.clone()} live={live.get(&c.symbol, r.market).cloned()} amount={amounts.get(&c.symbol).copied()} />
                    }) }
                </ol>

                if !r.watch.is_empty() {
                    <h2 class="section-label">{ format!("À surveiller · {}", r.watch.len()) }</h2>
                    <ul class="watch-list">
                        { for r.watch.iter().map(|c| html! {
                            <li key={c.symbol.clone()} class="card">
                                <a href={format!("/app/actif/{}/{}", r.market.as_str(), c.symbol)} onclick={on_link.clone()}><b>{ c.name.clone() }</b></a>{ " " }<small class="muted">{ c.symbol.clone() }</small>
                                <p class="small">{ format!("⚠ {}", c.reason) }</p>
                            </li>
                        }) }
                    </ul>
                }
                if !r.set_aside.is_empty() {
                    <details class="card">
                        <summary>{ format!("Écartées faute de données fiables · {}", r.set_aside.len()) }</summary>
                        <ul class="small">
                            { for r.set_aside.iter().map(|x| html! { <li key={x.symbol.clone()}><b>{ x.symbol.clone() }</b>{ format!(" : {}", x.reason) }</li> }) }
                        </ul>
                    </details>
                }
                <p class="muted small">
                    { format!("Sélection calculée à {}, prix en direct. Conseil indicatif, pas une recommandation personnalisée : Altim ne passe aucun ordre.", crate::ui::fr_time_seconds(r.as_of)) }
                </p>
            }
        </section>
    }
}
