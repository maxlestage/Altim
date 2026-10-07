//! Radar (Radar.tsx): the watchlist with live prices, the full decision of each asset (from this browser's cache,
//! re-read every 15 minutes), the technical signal of the chosen timeframe, the configuration changes, what is buyable now, the macro
//! context and the positions that became dangerous.
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::engine::decision_types::Verdict;
use altim_core::engine::macro_ctx::MacroLevel;
use altim_core::js::round;
use altim_core::types::Kind;
use altim_core::web::danger::{DANGERS_KEY, parse_dangers};
use altim_core::web::decision::config_changes::transition_title;
use altim_core::web::decision::format::{rating_tone, short_date_time};
use altim_core::web::decision::radar::{RadarSort, SORT_KEY, merge_rows, opportunities, sorted};
use altim_core::web::decision::reports::{BuyAlertRow, FearGreed, MacroInfo, RadarRow};
use altim_core::web::store::WatchItem;
use yew::prelude::*;

use crate::app::asset::{DecisionBadge, DecisionNote, api, interval_chooser, interval_label, store};
use crate::app::bot::{BotRadarCard, BriefCard};
use crate::app::holdings::CompareCard;
use crate::live::{LiveBadge, LivePrice, use_live};
use crate::route::use_on_link;
use crate::state::{local_get, local_set};
use crate::ui::{Change, ReliabilityBadge, Segmented, Sparkline, technical_text};

/// The decisions of the radar are re-read at most this often (they are heavier than the signals).
const DECISION_EVERY: f64 = 15.0 * 60_000.0;

#[derive(Default, PartialEq)]
struct Rows(HashMap<String, RadarRow>);

enum RowsAction {
    Clear,
    /// A refresh: an asset in error keeps its last valid data.
    Merge(Vec<RadarRow>),
}

impl Reducible for Rows {
    type Action = RowsAction;
    fn reduce(self: Rc<Self>, action: RowsAction) -> Rc<Self> {
        match action {
            RowsAction::Clear => Rc::new(Rows::default()),
            RowsAction::Merge(data) => Rc::new(Rows(merge_rows(&self.0, data))),
        }
    }
}

fn items_of(watchlist: &[WatchItem]) -> Vec<(String, Kind)> {
    watchlist.iter().map(|w| (w.symbol.clone(), w.kind)).collect()
}

fn asset_href(kind: Kind, symbol: &str) -> String {
    format!("/app/actif/{}/{symbol}", kind.as_str())
}

#[component]
pub fn Radar() -> Html {
    let on_link = use_on_link();
    let app = crate::state::app::use_app_state();
    let money = crate::money::use_money();
    let _seen = store::use_decisions_seen();
    let transitions_state = store::use_transitions();
    let watchlist = app.watchlist.clone();
    let interval = app.interval;
    let rows = use_reducer(Rows::default);
    let loading = use_state(|| false);
    let updated = use_state(|| None::<f64>);
    let error = use_state(|| None::<String>);
    let fg = use_state(|| None::<FearGreed>);
    let macro_info = use_state(|| None::<MacroInfo>);
    let buyable = use_state(|| None::<Vec<BuyAlertRow>>);
    let busy = use_mut_ref(|| false);
    let live = use_live(items_of(&watchlist));
    // Order of the list: the user's own, the largest moves first, or the strongest decisions first (remembered).
    let sort = use_state(|| RadarSort::parse(local_get(SORT_KEY).as_deref()));
    let choose_sort = {
        let sort = sort.clone();
        Callback::from(move |v: AttrValue| {
            let s = RadarSort::parse(Some(&v));
            sort.set(s);
            local_set(SORT_KEY, s.as_str());
        })
    };
    let show_all = use_state(|| false);

    {
        let (rows, loading, updated, error, busy) = (rows.clone(), loading.clone(), updated.clone(), error.clone(), busy.clone());
        use_effect_with((watchlist.clone(), interval), move |(watchlist, interval)| {
            rows.dispatch(RowsAction::Clear);
            let items = items_of(watchlist);
            let interval = *interval;
            let alive = Rc::new(Cell::new(true));
            let refresh = {
                let alive = alive.clone();
                Rc::new(move || {
                    if *busy.borrow() || items.is_empty() {
                        return;
                    }
                    *busy.borrow_mut() = true;
                    loading.set(true);
                    let (items, rows, loading, updated, error, busy, alive) =
                        (items.clone(), rows.clone(), loading.clone(), updated.clone(), error.clone(), busy.clone(), alive.clone());
                    wasm_bindgen_futures::spawn_local(async move {
                        let r = api::radar(&items, interval).await;
                        if alive.get() {
                            match r {
                                Ok(data) => {
                                    rows.dispatch(RowsAction::Merge(data));
                                    error.set(None);
                                    updated.set(Some(js_sys::Date::now()));
                                }
                                Err(e) => error.set(Some(if e.0.is_empty() { "Réseau indisponible".into() } else { e.0 })),
                            }
                            loading.set(false);
                        }
                        *busy.borrow_mut() = false;
                    });
                })
            };
            refresh();
            let timer = gloo::timers::callback::Interval::new(60_000, move || {
                if crate::hooks::visible() {
                    refresh();
                }
            });
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }

    {
        let (fg, macro_info) = (fg.clone(), macro_info.clone());
        use_effect_with((), move |_| {
            let alive = Rc::new(Cell::new(true));
            {
                let (fg, a) = (fg.clone(), alive.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(s) = api::sentiment("BTC", Kind::Crypto).await {
                        if a.get() {
                            fg.set(s.fear_greed);
                        }
                    }
                });
            }
            let load_macro = {
                let alive = alive.clone();
                move || {
                    let (m, a) = (macro_info.clone(), alive.clone());
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Ok(x) = api::macro_info().await {
                            if a.get() {
                                m.set(Some(x));
                            }
                        }
                    });
                }
            };
            load_macro();
            let timer = gloo::timers::callback::Interval::new(300_000, move || {
                if crate::hooks::visible() {
                    load_macro();
                }
            });
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }

    // Same rule as the notifications of the iPhone, Apple Watch and Android apps, refreshed every 5 minutes.
    {
        let buyable = buyable.clone();
        use_effect_with(watchlist.clone(), move |watchlist| {
            let alive = Rc::new(Cell::new(true));
            let mut timer = None;
            if watchlist.is_empty() {
                buyable.set(Some(Vec::new()));
            } else {
                let items = items_of(watchlist);
                let load = {
                    let alive = alive.clone();
                    move || {
                        let (b, a, items) = (buyable.clone(), alive.clone(), items.clone());
                        wasm_bindgen_futures::spawn_local(async move {
                            let r = api::alerts(&items).await;
                            if a.get() {
                                b.set(r.ok().map(|x| x.into_iter().filter(|x| x.buy).collect()));
                            }
                        });
                    }
                };
                load();
                timer = Some(gloo::timers::callback::Interval::new(300_000, move || {
                    if crate::hooks::visible() {
                        load();
                    }
                }));
            }
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }

    // Decisions of the watched assets (market data only), re-read every 15 minutes: each one goes through the
    // configuration diff. A fresher personal decision seen on the asset page stays in the cache. One whose texts are
    // in another currency than the display (read before the rate was known, or before a change in Réglages) is
    // re-read at once, so the line under its chip shows the same currency as the prices.
    let usd = money.currency() == altim_core::web::money::Currency::Usd;
    use_effect_with((watchlist.clone(), usd), move |(watchlist, _)| {
        let alive = Rc::new(Cell::new(true));
        let list: Rc<RefCell<Vec<WatchItem>>> = Rc::new(RefCell::new(watchlist.iter().take(20).cloned().collect()));
        let load = {
            let alive = alive.clone();
            move || {
                let (alive, list) = (alive.clone(), list.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let now = js_sys::Date::now();
                    let due: Vec<WatchItem> = list
                        .borrow()
                        .iter()
                        .filter(|w| {
                            store::cached(w.kind, &w.symbol).is_none_or(|c| {
                                now - c.at >= DECISION_EVERY || c.decision.texts_in_usd().is_some_and(|u| u != crate::money::currency_is_usd())
                            })
                        })
                        .cloned()
                        .collect();
                    // Two at a time: the server fetches fundamentals and order books for each one.
                    for pair in due.chunks(2) {
                        if !alive.get() {
                            return;
                        }
                        let calls = pair.iter().map(|w| async move { (w, api::decision(&w.symbol, w.kind, None, None).await) });
                        for (w, r) in futures::future::join_all(calls).await {
                            if !alive.get() {
                                return;
                            }
                            // Unavailable: compared at the next refresh.
                            if let Ok(d) = r {
                                if store::cached(w.kind, &w.symbol).and_then(|c| c.personal) == Some(true) {
                                    store::record_configuration(&d, false, js_sys::Date::now());
                                } else {
                                    store::cache_decision(&d, false);
                                }
                            }
                        }
                    }
                });
            }
        };
        load();
        let timer = gloo::timers::callback::Interval::new(DECISION_EVERY as u32, move || {
            if crate::hooks::visible() {
                load();
            }
        });
        move || {
            alive.set(false);
            drop(timer);
        }
    });

    // A verdict pushed by the live connection that differs from the cached decision: that decision is re-read at
    // once (configuration diff, chip, note), instead of waiting for the 15-minute round. Once per pushed verdict.
    {
        let handled = use_mut_ref(std::collections::HashSet::<String>::new);
        use_effect_with(live.verdicts.clone(), move |verdicts| {
            for v in verdicts.values() {
                let sig = format!("{}:{}:{}:{}", v.kind.as_str(), v.symbol, v.verdict, v.chip_note.as_deref().unwrap_or(""));
                let Some(c) = store::cached(v.kind, &v.symbol) else { continue };
                let same = serde_json::to_value(c.decision.d.verdict).ok().and_then(|x| x.as_str().map(String::from)).as_deref()
                    == Some(v.verdict.as_str())
                    && c.decision.d.chip_note.as_deref().unwrap_or("") == v.chip_note.as_deref().unwrap_or("");
                if same || c.personal == Some(true) || !handled.borrow_mut().insert(sig) {
                    continue;
                }
                let (symbol, kind) = (v.symbol.clone(), v.kind);
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(d) = api::decision(&symbol, kind, None, None).await {
                        store::cache_decision(&d, false);
                    }
                });
            }
        });
    }
    // What can be bought now: pushed by the live connection when it changes, else the 5-minute reading.
    let pushed_buyable: Option<Vec<BuyAlertRow>> = live.alert_rows::<BuyAlertRow>().map(|v| v.into_iter().filter(|x| x.buy).collect());
    let buyable_now = pushed_buyable.as_ref().or(buyable.as_ref());

    let dangers = parse_dangers(local_get(DANGERS_KEY).as_deref());
    let transitions = &transitions_state.transitions;
    let fresh: HashMap<String, Rc<altim_core::web::decision::CachedDecision>> =
        watchlist.iter().filter_map(|w| store::fresh(w.kind, &w.symbol).map(|c| (w.key(), c))).collect();
    let card_tone = |w: &WatchItem| fresh.get(&w.key()).map(|c| rating_tone(c.decision.rating(), c.decision.d.verdict)).unwrap_or("");
    // Assets whose full decision is ACHETER or ZONE D'ACHAT (not the 4 h technical signal alone).
    let opps = opportunities(&watchlist, |w| fresh.get(&w.key()).map(|c| &**c));
    let rows_now = &rows.0;
    let list = sorted(
        &watchlist,
        *sort,
        |w| live.get(&w.symbol, w.kind).and_then(|t| t.change).or(rows_now.get(&w.key()).and_then(|r| r.change)),
        |w| fresh.get(&w.key()).map(|c| &**c),
    );
    let label = interval_label(interval);
    let toggle_all = {
        let show_all = show_all.clone();
        Callback::from(move |_| show_all.set(!*show_all))
    };
    let clear = Callback::from(|_| {
        let ok = web_sys::window().and_then(|w| w.confirm_with_message("Effacer l'historique des changements ?").ok()).unwrap_or(false);
        if ok {
            store::clear_transitions();
        }
    });

    html! {
        <section class="app-screen">
            <div class="screen-top">
                <div>
                    <h1>{ "Radar" }</h1>
                    <LiveBadge status={live.status} last={live.last} />
                    <p class="muted small">
                        { match *updated {
                            Some(t) => format!("Signaux recalculés à {}", crate::ui::fr_time_seconds(t)),
                            None => "Interrogation des sources…".into(),
                        } }
                        { if *loading && updated.is_some() { " · actualisation…" } else { "" } }
                    </p>
                </div>
                if let Some(fg) = &*fg {
                    <div class={format!("fg {}", if fg.value < 45.0 { "down" } else if fg.value > 55.0 { "up" } else { "" })} title="Indice Fear & Greed crypto (contexte, hors score)">
                        <b>{ altim_core::js::number_to_string(fg.value) }</b>
                        <small>{ fg.label.clone() }</small>
                    </div>
                }
            </div>

            { interval_chooser(interval) }

            <BriefCard />
            <BotRadarCard />

            <a href="/app/alertes" onclick={on_link.clone()} class="btn btn-ghost btn-small alerts-link">{ "🔔 Alertes : achetables, alertes de prix, notifications" }</a>

            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e} — nouvelle tentative automatique.") }</p>
            }

            if let Some(m) = macro_info.as_ref().filter(|m| m.report.level != MacroLevel::Calm) {
                <div class={format!("notice {}", if m.report.level == MacroLevel::High { "danger" } else { "warn" })} role="status">
                    <b>{ format!("Contexte macro {} ({}/100)", if m.report.level == MacroLevel::High { "très tendu" } else { "tendu" }, altim_core::js::number_to_string(m.report.score)) }</b>
                    <ul class="small">{ for m.report.factors.iter().take(3).map(|f| html! { <li key={f.code.clone()}>{ f.text.clone() }</li> }) }</ul>
                    <small>{ "Les zones d'achat techniques résistent mal aux crises : tailles réduites, achats échelonnés." }</small>
                </div>
            }

            if let Some(d) = dangers.as_ref().filter(|d| !d.items.is_empty()) {
                <div class="notice danger" role="status">
                    <b>{ format!("⚠ {} dans vos avoirs", if d.items.len() > 1 { "Positions devenues dangereuses" } else { "Position devenue dangereuse" }) }</b>
                    <ul class="small">
                        { for d.items.iter().map(|x| html! {
                            <li key={x.id.clone()}><b>{ x.symbol.clone() }</b>{ format!(" : {}", x.reasons.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join(" ")) }</li>
                        }) }
                    </ul>
                    <small>
                        { format!("Mesuré le {} sur ", short_date_time(d.at)) }
                        <a href="/app/avoirs" onclick={on_link.clone()} class="link">{ "Mes avoirs" }</a>{ " (ouvrez-le pour actualiser)." }
                    </small>
                </div>
            }

            if !transitions.is_empty() {
                <div class="card">
                    <h2 class="card-title">{ format!("Changements de configuration · {}", transitions.len()) }</h2>
                    <ul class="insights">
                        { for transitions.iter().take(if *show_all { transitions.len() } else { 5 }).map(|t| {
                            let cls = match t.to.verdict {
                                Verdict::Buy | Verdict::BuyZone => "good",
                                Verdict::Sell | Verdict::Trim => "danger",
                                _ => "info",
                            };
                            html! {
                                <li key={format!("{}:{}:{}:{}", t.kind.as_str(), t.symbol, t.personal, altim_core::js::number_to_string(t.at))} class={format!("insight {cls}")}>
                                    <span aria-hidden="true">{ "↻" }</span>
                                    <span>
                                        <a href={asset_href(t.kind, &t.symbol)} onclick={on_link.clone()} class="link"><b>{ transition_title(t) }</b></a>
                                        <br />
                                        <small class="muted">
                                            { format!(
                                                "{} · niveau {} → {} · configuration précédente vue le {}{}",
                                                short_date_time(t.at), t.from.level_label, t.to.level_label, short_date_time(t.since), if t.personal { " · mode personnel" } else { "" }
                                            ) }
                                        </small>
                                        if !t.changes.is_empty() {
                                            <br /><small>{ format!("Pourquoi le signal a changé : {}.", t.changes.join(" ; ")) }</small>
                                        }
                                        if !t.missing.is_empty() {
                                            <br /><small>{ format!("Conditions manquantes : {}.", t.missing.join(" ; ")) }</small>
                                        }
                                        if !t.triggers.is_empty() {
                                            <br /><small>{ format!("Ce qui changerait la décision : {}.", t.triggers.join(" ; ")) }</small>
                                        }
                                    </span>
                                </li>
                            }
                        }) }
                    </ul>
                    <div class="row-actions">
                        if transitions.len() > 5 {
                            <button class="link-btn" onclick={toggle_all}>{ if *show_all { "Voir moins".to_string() } else { format!("Voir les {}", transitions.len()) } }</button>
                        }
                        <button class="link-btn" onclick={clear}>{ "Effacer" }</button>
                    </div>
                    <small class="muted">
                        { "Comparaison avec la dernière décision vue dans ce navigateur. Nouvelle analyse toutes les 15 minutes tant que le radar est ouvert, et à chaque ouverture d'une fiche. 50 derniers changements conservés ici uniquement." }
                    </small>
                </div>
            }

            if let Some(b) = buyable_now.filter(|b| !b.is_empty()) {
                <div class="card buyable">
                    <h2 class="card-title">{ format!("Achetables maintenant · {}", b.len()) }</h2>
                    <ul class="buyable-list">
                        { for b.iter().map(|a| html! {
                            <li key={format!("{}:{}", a.kind.as_str(), a.symbol)}>
                                <a href={asset_href(a.kind, &a.symbol)} onclick={on_link.clone()} class="opp-row">
                                    <b>{ a.name.clone() }</b>
                                    <span class="mono">{ a.price.map(crate::money::price).unwrap_or_else(|| "—".into()) }</span>
                                    <span class="badge buy">{ if a.strong { "ACHETER" } else { "ZONE D'ACHAT" } }</span>
                                </a>
                                <small class="muted">{ a.reasons.iter().chain(a.cautions.iter()).cloned().collect::<Vec<_>>().join(" ") }</small>
                            </li>
                        }) }
                    </ul>
                    <small class="muted">
                        { "Listés seulement quand la décision complète de l'actif dit ACHETER ou ZONE D'ACHAT (signal 4 h quelle que soit l'unité choisie ci-dessus, ou zone Fibonacci, sans source en désaccord, choc ni zone cassée) : la même règle que les notifications des apps. Conseil indicatif." }
                    </small>
                </div>
            }

            if !opps.is_empty() {
                <div class="card opportunities">
                    <h2 class="card-title">{ "Opportunités détectées" }</h2>
                    { for opps.iter().map(|(w, d)| html! {
                        <a key={w.key()} href={asset_href(w.kind, &w.symbol)} onclick={on_link.clone()} class="opp-row">
                            <b>{ w.name.clone() }</b>
                            <span class="muted">{ format!("confiance {}", round(d.decision.d.confidence)) }</span>
                            <DecisionBadge kind={w.kind} symbol={w.symbol.clone()} />
                        </a>
                    }) }
                </div>
            }

            if watchlist.len() > 1 {
                <Segmented
                    label="Trier le radar"
                    value={AttrValue::Static(sort.as_str())}
                    options={vec![("mine".into(), "Mon ordre".into()), ("change".into(), "Variation".into()), ("signal".into(), "Décision".into())]}
                    on_change={choose_sort}
                />
            }
            <ul class="asset-list">
                { for list.iter().map(|w| {
                    let r = rows_now.get(&w.key());
                    let t = live.get(&w.symbol, w.kind).cloned();
                    // The sparkline ends on the live price: the chart moves with the market.
                    let mut spark = r.map(|r| r.sparkline.clone()).unwrap_or_default();
                    if let (false, Some(t)) = (spark.is_empty(), &t) {
                        spark.push(t.price);
                    }
                    html! {
                        <li key={w.key()}>
                            <a href={asset_href(w.kind, &w.symbol)} onclick={on_link.clone()} class={format!("asset-card {}", card_tone(w))}>
                                <div class="asset-id">
                                    <b>{ w.name.clone() }</b>
                                    <small>{ format!("{} · {}", w.symbol, if w.kind == Kind::Crypto { "Crypto" } else { "Action" }) }</small>
                                </div>
                                <Sparkline values={spark} />
                                <div class="asset-price">
                                    <b><LivePrice tick={t.clone()} fallback={r.and_then(|r| r.price)} format={Callback::from(crate::money::price)} /></b>
                                    <Change value={t.as_ref().and_then(|t| t.change).or(r.and_then(|r| r.change))} />
                                    if t.as_ref().and_then(|t| t.market.as_deref()) == Some("closed") {
                                        <small class="market-closed" title="Bourse de New York fermée : dernier cours connu">{ "Bourse fermée" }</small>
                                    }
                                </div>
                                <div class="asset-meta">
                                    <DecisionBadge kind={w.kind} symbol={w.symbol.clone()} />
                                    { match r {
                                        Some(r) if r.signal.is_some() => html! {
                                            <small class="muted" title={format!("Signal technique sur bougies de {label} : un indice parmi d'autres de la décision")}>
                                                { format!("technique {label} : {}", technical_text(r.signal.as_ref().map(|s| s.action).unwrap_or(altim_core::engine::signal::Action::Hold))) }
                                            </small>
                                        },
                                        Some(r) if r.error.is_some() => html! { <small class="muted">{ "signal technique indisponible" }</small> },
                                        _ => html! { <span class="skeleton-line" /> },
                                    } }
                                    if let Some(rel) = r.and_then(|r| r.reliability.clone()) {
                                        <ReliabilityBadge {rel} />
                                    }
                                    { match (&t, r.and_then(|r| r.price_sources.clone())) {
                                        (Some(t), _) => html! { <small class="muted">{ format!("prix {}/{} sources", t.agreeing, t.total) }</small> },
                                        (None, Some(s)) if !s.is_empty() => html! { <small class="muted">{ format!("prix {s} sources") }</small> },
                                        _ => html! {},
                                    } }
                                    <DecisionNote kind={w.kind} symbol={w.symbol.clone()} />
                                </div>
                            </a>
                        </li>
                    }
                }) }
            </ul>
            if watchlist.is_empty() {
                <p class="muted">
                    { "Votre radar est vide. " }<a href="/app/reglages" onclick={on_link.clone()} class="link">{ "Ajouter des actifs" }</a>
                </p>
            }

            if watchlist.len() >= 2 {
                <CompareCard assets={items_of(&watchlist)} />
            }
        </section>
    }
}
