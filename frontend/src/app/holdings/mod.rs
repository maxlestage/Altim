//! Mes avoirs (MyHoldings.tsx): the real portfolio entered by the user (localStorage "altim.holdings.v1") and its full
//! analysis — value, gains, a recommendation per line, risk, sectors, limits, stress scenarios, « Et si… ? » — with
//! the tools (rebalancing, sale after tax, projection). Engines in `altim_core::web::portfolio`.
//! Components reused by other screens are exported here: `AssetPicker` (Réglages), `CompareCard` (Radar),
//! `DcaCard` and `PositionCard` (asset screen).
mod add;
mod api;
mod browser;
mod dca;
mod form;
mod history;
mod picker;
mod risk_cards;
mod search;
mod sector;
mod tools;
mod whatif;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::engine::reliability::ReliabilityLevel;
use altim_core::engine::signal::Candle;
use altim_core::js::{fr, iso_date, number_to_string, to_fixed};
use altim_core::types::Kind;
use altim_core::web::danger::{DANGERS_KEY, Danger, dangers_json};
use altim_core::web::money::Currency;
use altim_core::web::portfolio::holdings::{
    Allocation, InsightLevel, LineAnalysis, LineSignal, MarketInput, PortfolioAnalysis, Recommendation, analyze_portfolio, insight_text, market_key,
    reason_text,
};
use altim_core::web::portfolio::portfolio_risk::{
    BETA_DAYS, BetaEstimate, ClusterInput, LimitCheck, LimitLevel, STRESS_SCENARIOS, StressResult, benchmark, check_limits, correlated_clusters,
    daily_change, dangerous_positions, estimate_beta, stress_test,
};
use altim_core::web::portfolio::view::{cash_note, cost_note, holdings_csv, input_text, parse_decimal};
use altim_core::web::store::{Holding, HoldingsState, StoredHolding, shown};
use yew::prelude::*;

pub use add::AddHoldings;
pub use dca::DcaCard;
pub use form::HoldingForm;
pub use history::HistoryCard;
pub use picker::{AssetPicker, kind_label};
pub use risk_cards::{LimitsCard, StressCard};
pub use search::AssetSearch;
pub use sector::SectorCard;
pub use tools::{CompareCard, PositionCard, ProjectionCard, RebalanceCard, SaleCard};
pub use whatif::WhatIfCard;

use crate::app::common::{FxNote, PortfolioTab, PortfolioTabs};
use crate::live::{LiveBadge, LivePrice, use_live};
use crate::route::use_on_link;
use crate::state::holdings::{set_holdings, use_holdings, use_stored_holdings};
use crate::state::{local_set, now};
use crate::ui::Change;
use altim_core::web::sorting::Sorting;

/// `money(v, 2, 2)`.
fn usd(v: f64) -> String {
    crate::money::money(v)
}

/// UTC date of the exported files' names ("altim-avoirs-2026-09-30.csv").
fn today() -> String {
    iso_date(now() as i64)
}

fn rec_class(r: Recommendation) -> &'static str {
    match r {
        Recommendation::Sell | Recommendation::Protect => "sell",
        Recommendation::Lighten | Recommendation::Hold => "hold",
        Recommendation::Strengthen => "buy",
        Recommendation::Unknown => "unknown",
    }
}

fn level_icon(l: InsightLevel) -> &'static str {
    match l {
        InsightLevel::Danger => "⛔",
        InsightLevel::Warning => "⚠",
        InsightLevel::Info => "ℹ",
        InsightLevel::Good => "✔",
    }
}

/// Market data of the lines and daily candles of the benchmarks not held (betas of the stress tests).
#[derive(Clone, PartialEq, Default)]
struct Market {
    by_asset: Rc<HashMap<String, MarketInput>>,
    bench: Rc<HashMap<String, Vec<Candle>>>,
}

/// What the screen shows beyond the analysis: stress scenarios, limits of the settings, dangerous positions.
#[derive(Clone, PartialEq)]
struct RiskView {
    stress: Rc<Vec<StressResult>>,
    limits: Rc<Vec<LimitCheck>>,
    dangers: Vec<Danger>,
}

/// Loads the radar signals (daily and 4 h) and the daily candles of the assets, plus the benchmarks' candles.
async fn load_market(holdings: &[Holding]) -> Result<Market, crate::api::ApiError> {
    let mut unique: Vec<(String, Kind)> = Vec::new();
    for h in holdings {
        if !unique.iter().any(|(s, k)| *s == h.symbol && *k == h.kind) {
            unique.push((h.symbol.clone(), h.kind));
        }
    }
    let mut kinds: Vec<Kind> = Vec::new();
    for (_, k) in &unique {
        if !kinds.contains(k) {
            kinds.push(*k);
        }
    }
    let benchmarks: Vec<(String, Kind)> = kinds
        .iter()
        .map(|k| benchmark(*k))
        .filter(|b| !unique.iter().any(|(s, k)| s == b.symbol && *k == b.kind))
        .map(|b| (b.symbol.to_string(), b.kind))
        .collect();
    let candles = futures::future::join_all(unique.iter().map(|(s, k)| api::candles(s, *k, "1d")));
    let bench_candles = futures::future::join_all(benchmarks.iter().map(|(s, k)| api::candles(s, *k, "1d")));
    let (day, short, candles, bench_candles) = futures::join!(api::radar(&unique, "1d"), api::radar(&unique, "4h"), candles, bench_candles);
    let day = day?;
    let short = short.unwrap_or_default();
    let bench: HashMap<String, Vec<Candle>> = benchmarks
        .iter()
        .zip(bench_candles)
        .filter_map(|((s, k), c)| c.ok().filter(|c| !c.candles.is_empty()).map(|c| (market_key(*k, s), c.candles)))
        .collect();
    let mut by_asset = HashMap::new();
    for ((s, k), c) in unique.iter().zip(candles) {
        let d = day.iter().find(|r| r.symbol == *s && r.kind == *k);
        let sh = short.iter().find(|r| r.symbol == *s && r.kind == *k);
        let (dl, sl) = (d.and_then(|r| r.reliability).map(|r| r.level), sh.and_then(|r| r.reliability).map(|r| r.level));
        // The least reliable of the two timeframes wins (caution).
        let reliability =
            if dl == Some(ReliabilityLevel::Low) || sl == Some(ReliabilityLevel::Low) { Some(ReliabilityLevel::Low) } else { dl.or(sl) };
        let signal = |r: Option<&altim_core::web::portfolio::wire::RadarRow>| {
            r.and_then(|r| r.signal).map(|s| LineSignal { action: s.action, score: s.score })
        };
        by_asset.insert(
            market_key(*k, s),
            MarketInput {
                price: d.and_then(|r| r.price).or(sh.and_then(|r| r.price)).unwrap_or(0.0),
                daily: c.map(|c| c.candles).unwrap_or_default(),
                day_signal: signal(d),
                short_signal: signal(sh),
                reliability,
            },
        );
    }
    Ok(Market { by_asset: Rc::new(by_asset), bench: Rc::new(bench) })
}

/// Market data of one held line for the asset screen (same inputs as here: daily and 4 h radar signals, daily
/// candles). None when the daily signal or the candles fail (the 4 h signal is optional).
pub async fn held_market(symbol: &str, kind: Kind) -> Option<MarketInput> {
    let item = [(symbol.to_string(), kind)];
    let (day, short, daily) = futures::join!(api::radar(&item, "1d"), api::radar(&item, "4h"), api::candles(symbol, kind, "1d"));
    let (day, daily) = (day.ok()?, daily.ok()?);
    let (d, s) = (day.first(), short.ok().and_then(|s| s.into_iter().next()));
    let (dl, sl) = (d.and_then(|r| r.reliability).map(|r| r.level), s.as_ref().and_then(|r| r.reliability).map(|r| r.level));
    let signal =
        |r: Option<&altim_core::web::portfolio::wire::RadarRow>| r.and_then(|r| r.signal).map(|s| LineSignal { action: s.action, score: s.score });
    Some(MarketInput {
        price: d.and_then(|r| r.price).unwrap_or(0.0),
        daily: daily.candles,
        day_signal: signal(d),
        short_signal: signal(s.as_ref()),
        reliability: if dl == Some(ReliabilityLevel::Low) || sl == Some(ReliabilityLevel::Low) { Some(ReliabilityLevel::Low) } else { dl },
    })
}

/// Real portfolio entered by the user (localStorage) and full analysis.
#[component]
pub fn MyHoldings() -> Html {
    let m = crate::money::use_money();
    let usd_h = use_holdings();
    let stored = use_stored_holdings();
    let cur = m.currency();
    let app = crate::state::app::use_app_state();
    let on_link = use_on_link();
    let market = use_state(Market::default);
    let loading = use_state(|| false);
    let error = use_state(|| None::<String>);
    let editing = use_state(|| None::<StoredHolding>);
    let adding = use_state(|| false);
    let cash_initial =
        if stored.cash != 0.0 && !stored.cash.is_nan() { input_text(shown(stored.cash, stored.cash_currency, &m)) } else { String::new() };
    let cash_text = use_state(|| cash_initial.clone());
    {
        // The display currency or the rate changed: the field follows (unless being typed in).
        let cash_text = cash_text.clone();
        use_effect_with(cash_initial.clone(), move |c| cash_text.set(c.clone()));
    }
    let file_input = use_node_ref();
    let holdings = usd_h.holdings.clone();
    let mut keys: Vec<String> = holdings.iter().map(|h| market_key(h.kind, &h.symbol)).collect();
    keys.sort_dyn();
    let symbols_key = keys.join(",");

    {
        let (market, loading, error, holdings) = (market.clone(), loading.clone(), error.clone(), holdings.clone());
        use_effect_with(symbols_key, move |_| {
            let alive = Rc::new(Cell::new(true));
            let mut timer = None;
            if holdings.is_empty() {
                market.set(Market::default());
            } else {
                let load = {
                    let alive = alive.clone();
                    Rc::new(move || {
                        let (market, loading, error, holdings, alive) =
                            (market.clone(), loading.clone(), error.clone(), holdings.clone(), alive.clone());
                        loading.set(true);
                        wasm_bindgen_futures::spawn_local(async move {
                            let r = load_market(&holdings).await;
                            if !alive.get() {
                                return;
                            }
                            match r {
                                Ok(m) => {
                                    market.set(m);
                                    error.set(None);
                                }
                                Err(e) => error.set(Some(e.0)),
                            }
                            loading.set(false);
                        });
                    })
                };
                load();
                timer = Some(gloo::timers::callback::Interval::new(120_000, move || {
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

    // Live prices: the value, gains and suggested amounts follow the market tick by tick.
    let live = use_live(holdings.iter().map(|h| (h.symbol.clone(), h.kind)).collect());
    let live_market: HashMap<String, MarketInput> = market
        .by_asset
        .iter()
        .map(|(k, v)| match live.ticks.get(k) {
            Some(t) => (k.clone(), MarketInput { price: t.price, ..v.clone() }),
            None => (k.clone(), v.clone()),
        })
        .collect();
    let analysis: Rc<PortfolioAnalysis> = use_memo((holdings.clone(), usd_h.cash, live_market), |(h, cash, lm)| analyze_portfolio(h, *cash, lm));
    let ready = holdings.is_empty() || !market.by_asset.is_empty();

    // Risk beyond the analysis: betas, stress scenarios, limits of the settings, dangerous positions.
    let daily: Rc<HashMap<String, Vec<Candle>>> = use_memo(market.clone(), |m| {
        let mut d = (*m.bench).clone();
        for (k, v) in m.by_asset.iter().filter(|(_, v)| !v.daily.is_empty()) {
            d.insert(k.clone(), v.daily.clone());
        }
        d
    });
    let stops: HashMap<String, f64> = holdings.iter().filter_map(|h| h.stop.map(|s| (h.id.clone(), s))).collect();
    let betas: Rc<HashMap<String, BetaEstimate>> = use_memo((holdings.clone(), daily.clone()), |(holdings, daily)| {
        let mut b = HashMap::new();
        for h in holdings {
            let k = market_key(h.kind, &h.symbol);
            let r = benchmark(h.kind);
            // The benchmark itself: beta 1 by definition.
            let e = if h.symbol == r.symbol && h.kind == r.kind {
                BetaEstimate { beta: 1.0, days: 0, estimated: false, reference: true }
            } else {
                let none: Vec<Candle> = Vec::new();
                estimate_beta(daily.get(&k).unwrap_or(&none), daily.get(&market_key(r.kind, r.symbol)).unwrap_or(&none), BETA_DAYS)
            };
            b.insert(k, e);
        }
        b
    });
    let risk_view: Option<RiskView> = (ready && !holdings.is_empty()).then(|| {
        let mut weights: Vec<(String, ClusterInput)> = Vec::new();
        for l in &analysis.lines {
            let k = market_key(l.kind, &l.symbol);
            let d = daily.get(&k).map(Vec::as_slice).unwrap_or(&[]);
            match weights.iter_mut().find(|w| w.0 == k) {
                Some(w) => w.1.weight += l.weight,
                None => weights.push((k, ClusterInput { symbol: l.symbol.clone(), weight: l.weight, daily: d })),
            }
        }
        let inputs: Vec<ClusterInput> = weights.into_iter().map(|w| w.1).collect();
        let clusters = correlated_clusters(&inputs, BETA_DAYS);
        let today = daily_change(&analysis, &daily, now() as i64);
        RiskView {
            stress: Rc::new(stress_test(&analysis, &betas, &STRESS_SCENARIOS)),
            limits: Rc::new(check_limits(&analysis, &app.risk, &clusters, today.as_ref(), &stops)),
            dangers: dangerous_positions(&analysis, &app.risk, &daily, &stops),
        }
    });
    let danger_by_id: HashMap<&str, &Danger> = risk_view.iter().flat_map(|r| r.dangers.iter().map(|d| (d.id.as_str(), d))).collect();
    // Kept for the Radar (the web app has no notifications); only once the market data is loaded.
    let danger_key = serde_json::to_string(
        &risk_view
            .as_ref()
            .map(|r| r.dangers.iter().map(|d| (d.id.clone(), d.reasons.iter().map(|x| x.code).collect::<Vec<_>>())).collect::<Vec<_>>()),
    )
    .unwrap_or_default();
    {
        let (empty, dangers, loaded) = (holdings.is_empty(), risk_view.as_ref().map(|r| r.dangers.clone()), !market.by_asset.is_empty());
        use_effect_with(danger_key, move |_| {
            if empty {
                local_set(DANGERS_KEY, &dangers_json(&[], now()));
            } else if let (Some(d), true) = (dangers, loaded) {
                local_set(DANGERS_KEY, &dangers_json(&d, now()));
            }
        });
    }

    // Typed in the display currency and saved with it (an untouched field keeps its saved currency).
    let save_cash = {
        let (cash_text, initial) = (cash_text.clone(), cash_initial.clone());
        Callback::from(move |_: FocusEvent| {
            if *cash_text == initial {
                return;
            }
            let v = parse_decimal(&cash_text);
            if v.is_finite() && v >= 0.0 {
                set_holdings(|s| {
                    s.cash = v;
                    s.cash_currency = Some(cur);
                });
            } else {
                cash_text.set(initial.clone());
            }
        })
    };
    let on_cash = {
        let cash_text = cash_text.clone();
        Callback::from(move |e: InputEvent| cash_text.set(browser::input_value(&e)))
    };
    let cash_enter = Callback::from(|e: KeyboardEvent| {
        if e.key() == "Enter" {
            let _ = e.target_unchecked_into::<web_sys::HtmlElement>().blur();
        }
    });

    // Spreadsheet export (French Excel: ";" separator, decimal comma); texts starting like a formula are neutralised.
    let download_csv = {
        let (analysis, unconverted, m) = (analysis.clone(), usd_h.unconverted.clone(), m.clone());
        Callback::from(move |_: MouseEvent| {
            browser::download(&format!("altim-avoirs-{}.csv", today()), "text/csv;charset=utf-8", &holdings_csv(&analysis, &unconverted, &m))
        })
    };
    let download = {
        let stored = stored.clone();
        Callback::from(move |_: MouseEvent| browser::download(&format!("altim-avoirs-{}.json", today()), "application/json", &stored.export()))
    };
    let pick_file = {
        let file_input = file_input.clone();
        Callback::from(move |_: MouseEvent| {
            if let Some(i) = file_input.cast::<web_sys::HtmlElement>() {
                i.click();
            }
        })
    };
    let on_file = Callback::from(|e: Event| {
        let input: web_sys::HtmlInputElement = e.target_unchecked_into();
        wasm_bindgen_futures::spawn_local(async move {
            let Some(text) = browser::file_text(&input).await else { return };
            match HoldingsState::import(&text, now()) {
                Ok(n) => {
                    set_holdings(move |s| {
                        s.holdings = n.holdings;
                        s.cash = n.cash;
                        s.cash_currency = n.cash_currency;
                    });
                    browser::alert("Avoirs importés.");
                }
                Err(err) => browser::alert(err),
            }
            input.set_value("");
        });
    });
    let open_add = {
        let adding = adding.clone();
        Callback::from(move |_: MouseEvent| adding.set(true))
    };

    let a = &*analysis;
    let unconverted = &usd_h.unconverted;
    let stored_line = |id: &str| stored.holdings.iter().find(|h| h.id == id);
    let group = |kind: Kind| {
        let lines: Vec<&LineAnalysis> = a.lines.iter().filter(|l| l.kind == kind).collect();
        if lines.is_empty() {
            return html! {};
        }
        let value: f64 = lines.iter().map(|l| l.value).sum();
        html! {
            <div key={kind.as_str()} class="holding-group">
                <h2 class="section-label group-head">
                    <span>{ format!("{} · {}", kind_label(kind), lines.len()) }</span>
                    if ready {
                        <span class="mono">{ usd(value) }</span>
                    }
                </h2>
                <ul class="holding-list">
                    { for lines.iter().map(|l| {
                        let danger = danger_by_id.get(l.id.as_str());
                        let tick = live.get(&l.symbol, l.kind).cloned();
                        let closed = tick.as_ref().and_then(|t| t.market.as_deref()) == Some("closed");
                        let unconv = unconverted.contains(&l.symbol);
                        let note = stored_line(&l.id).and_then(|h| cost_note(h, &m));
                        let edit = {
                            let (editing, h) = (editing.clone(), stored_line(&l.id).cloned());
                            Callback::from(move |_: MouseEvent| editing.set(h.clone()))
                        };
                        let remove = {
                            let (id, name) = (l.id.clone(), l.name.clone());
                            Callback::from(move |_: MouseEvent| {
                                if browser::confirm(&format!("Supprimer {name} de vos avoirs ?")) {
                                    let id = id.clone();
                                    set_holdings(move |s| s.holdings.retain(|h| h.id != id));
                                }
                            })
                        };
                        let user_stop = stops.get(&l.id).copied().filter(|s| *s != 0.0);
                        html! {
                            <li key={l.id.clone()} class={classes!("card", "holding", if danger.is_some() { "sell" } else { rec_class(l.recommendation) })}>
                                <div class="holding-head">
                                    <a href={format!("/app/actif/{}/{}", l.kind.as_str(), l.symbol)} onclick={on_link.clone()} class="holding-name">
                                        <b>{ l.name.clone() }</b>
                                        <small class="muted mono">
                                            { format!("{} {} · PRU {}", fr(l.quantity, 0, 8), l.symbol, if unconv { "non converti".to_string() } else { crate::money::price(l.average_price) }) }
                                        </small>
                                        if let Some(n) = note {
                                            <small class="muted">{ n }</small>
                                        }
                                    </a>
                                    <span class={classes!("rec", format!("rec-{}", rec_class(l.recommendation)))}>{ l.recommendation.label() }</span>
                                </div>
                                <div class="holding-figures">
                                    <div><small>{ "Valeur" }</small><b>{ usd(l.value) }</b></div>
                                    if unconv {
                                        <div><small>{ "Gain / perte" }</small><b class="muted">{ "non calculé" }</b></div>
                                    } else {
                                        <div>
                                            <small>{ "Gain / perte" }</small>
                                            <b class={if l.pnl >= 0.0 { "up" } else { "down" }}>{ format!("{}{}", if l.pnl >= 0.0 { "+" } else { "−" }, usd(l.pnl.abs())) }</b>
                                            <Change value={Some(l.pnl_percent)} />
                                        </div>
                                    }
                                    <div>
                                        <small>{ format!("Cours{}", if closed { " (fermé)" } else { "" }) }</small>
                                        <b><LivePrice {tick} fallback={l.price.filter(|p| *p != 0.0)} format={Callback::from(crate::money::price)} /></b>
                                    </div>
                                </div>
                                <div class="weight" role="img" aria-label={format!("Poids {} % du patrimoine", to_fixed(l.weight, 1))}>
                                    <div class="weight-track"><i style={format!("width: {}%;", number_to_string(l.weight.min(100.0)))} /></div>
                                    <small>{ format!("{} % du patrimoine", to_fixed(l.weight, 1)) }</small>
                                </div>
                                if let Some(d) = danger {
                                    <div class="notice danger small" role="status">
                                        <b>{ "⚠ Position devenue dangereuse" }</b>
                                        <ul class="reasons">{ for d.reasons.iter().map(|r| html! { <li key={format!("{:?}", r.code)}>{ r.text.clone() }</li> }) }</ul>
                                    </div>
                                }
                                <ul class="reasons">
                                    { for l.reasons.iter().map(|r| html! { <li key={r.clone()}>{ reason_text(r) }</li> }) }
                                    if let Some(s) = user_stop {
                                        <li>
                                            { "Votre stop : " }<b>{ crate::money::price(s) }</b>
                                            { match l.price.filter(|p| *p != 0.0) {
                                                Some(p) => format!(" ({} % sous le cours)", fr((p - s) / p * 100.0, 0, 1)),
                                                None => String::new(),
                                            } }
                                            { "." }
                                        </li>
                                    }
                                    if let (Recommendation::Lighten, true, Some(p)) = (l.recommendation, l.trim_value > 0.0, l.price.filter(|p| *p != 0.0)) {
                                        <li>{ format!("Suggestion : vendre environ {} (≈ {} {}).", usd(l.trim_value), fr(l.trim_value / p, 0, 6), l.symbol) }</li>
                                    }
                                    if let (Some(stop), Some(loss)) = (l.stop.filter(|s| *s != 0.0), l.loss_at_stop) {
                                        <li>
                                            { "Stop de protection conseillé : " }<b>{ crate::money::price(stop) }</b>
                                            { format!(" (2 × la volatilité journalière) — perte limitée à ≈ {} depuis le cours actuel.", usd(loss)) }
                                        </li>
                                    }
                                </ul>
                                <div class="holding-actions">
                                    <button class="link-btn" onclick={edit}>{ "Modifier" }</button>
                                    <button class="link-btn danger" onclick={remove}>{ "Supprimer" }</button>
                                </div>
                            </li>
                        }
                    }) }
                </ul>
            </div>
        }
    };

    let daily_loss = risk_view.as_ref().is_some_and(|r| r.limits.iter().any(|c| c.code == "daily_loss" && c.level == LimitLevel::Danger));
    let unconverted_list: Vec<String> = unconverted.iter().cloned().chain(usd_h.cash_unconverted.then(|| "liquidités".to_string())).collect();
    html! {
        <section class="app-screen">
            <PortfolioTabs active={PortfolioTab::Real} />
            <div class="screen-top">
                <div>
                    <h1>{ "Mes avoirs" }</h1>
                    <p class="muted small">
                        { "Enregistrés uniquement dans ce navigateur" }
                        { if usd_h.updated_at != 0.0 { format!(" · modifiés le {}", browser::fr_date(usd_h.updated_at)) } else { String::new() } }
                    </p>
                </div>
                <button class="btn btn-small" onclick={open_add.clone()}>{ "+ Ajouter" }</button>
            </div>

            if holdings.is_empty() {
                <div class="card empty-card">
                    <h2>{ "Renseignez ce que vous possédez déjà" }</h2>
                    <p class="muted">
                        { "Ajoutez en une fois toutes vos cryptos et toutes vos actions, avec leur quantité et votre prix d'achat moyen. Altim calcule alors votre patrimoine, vos gains, vos risques et vous dit, ligne par ligne, quoi faire. Vos données restent dans ce navigateur." }
                    </p>
                    <button class="btn" onclick={open_add}>{ "Ajouter mes cryptos et actions" }</button>
                </div>
            }

            <div class="card">
                <label class="field">
                    <span>{ format!("Liquidités disponibles ({})", if cur == Currency::Eur { "€" } else { "$" }) }</span>
                    <input inputmode="decimal" value={(*cash_text).clone()} placeholder="0" oninput={on_cash} onblur={save_cash} onkeydown={cash_enter} />
                </label>
                if let Some(n) = cash_note(stored.cash, stored.cash_currency, usd_h.cash_unconverted, &m) {
                    <p class="muted small">{ n }</p>
                }
            </div>

            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e} — nouvelle tentative automatique.") }</p>
            }
            if !unconverted_list.is_empty() {
                <p class="notice warn" role="status">
                    { format!(
                        "⚠ Taux EUR/USD indisponible : {} saisi(es) en € ne peuvent pas être converti(es) ; plus-values de ces lignes non calculées, liquidités comptées à 0 jusqu'au retour du taux.",
                        unconverted_list.join(", ")
                    ) }
                </p>
            }

            if !holdings.is_empty() {
                <div class="card summary-card">
                    <div class="summary-top">
                        <small class="muted">{ "Patrimoine total" }</small>
                        <LiveBadge status={live.status} last={live.last} />
                    </div>
                    <b class="mono big">{ if ready { usd(a.total) } else { "…".into() } }</b>
                    if ready && unconverted.is_empty() {
                        <p class="kv">
                            <span>{ "Plus-value latente" }</span>
                            <b class={if a.pnl >= 0.0 { "up" } else { "down" }}>
                                { format!("{}{} (", if a.pnl >= 0.0 { "+" } else { "−" }, usd(a.pnl.abs())) }<Change value={Some(a.pnl_percent)} />{ ")" }
                            </b>
                        </p>
                    }
                    <p class="kv small"><span>{ "Investi (prix d'achat)" }</span><b>{ if unconverted.is_empty() { usd(a.invested) } else { "non calculé".into() } }</b></p>
                    if ready {
                        <AllocationBar a={a.allocation} />
                    }
                    if *loading {
                        <p class="muted small">{ "Actualisation des cours et des signaux…" }</p>
                    }
                    <FxNote />
                </div>

                <HistoryCard holdings={holdings.clone()} />

                if ready {
                    <RebalanceCard analysis={analysis.clone()} />
                    <SaleCard analysis={analysis.clone()} />
                    <ProjectionCard start={a.total} />
                }

                if daily_loss {
                    <p class="notice danger" role="alert">{ "⛔ Limite de perte du jour atteinte : n'ouvrez plus de position aujourd'hui." }</p>
                }

                if let Some(r) = risk_view.as_ref().filter(|r| !r.dangers.is_empty()) {
                    <div class="notice danger" role="status">
                        <b>
                            { format!(
                                "{} : {}",
                                if r.dangers.len() > 1 { "Positions devenues dangereuses" } else { "Position devenue dangereuse" },
                                r.dangers.iter().map(|d| d.symbol.as_str()).collect::<Vec<_>>().join(", ")
                            ) }
                        </b>
                        <br />
                        <small>{ "Détail sur chaque ligne ci-dessous. Altim ne passe aucun ordre : à vous de décider (réduire, sortir ou accepter le risque)." }</small>
                    </div>
                }

                if ready {
                    <div class="card insights-card">
                        <h2 class="card-title">{ "Ce qu'Altim vous dit" }</h2>
                        <ul class="insights">
                            { for a.insights.iter().enumerate().map(|(n, i)| html! {
                                <li key={n} class={classes!("insight", i.level.as_str())}>
                                    <span aria-hidden="true">{ level_icon(i.level) }</span>
                                    <span>{ insight_text(i) }</span>
                                </li>
                            }) }
                        </ul>
                    </div>
                }

                { group(Kind::Crypto) }
                { group(Kind::Stock) }

                if ready {
                    <div class="card">
                        <h2 class="card-title">{ "Risque du portefeuille" }</h2>
                        <p class="kv"><span>{ "Volatilité annuelle" }</span><b>{ a.risk.volatility_annual.map(|v| format!("{} %", to_fixed(v, 1))).unwrap_or_else(|| "—".into()) }</b></p>
                        <p class="kv">
                            <span>{ "Perte possible sur 1 jour (1 fois sur 20)" }</span>
                            <b>{ a.risk.var95_day.map(|v| format!("{} ({} %)", usd(v), to_fixed(a.risk.var95_day_percent.unwrap_or(0.0), 1))).unwrap_or_else(|| "—".into()) }</b>
                        </p>
                        <p class="kv"><span>{ "Perte si tous les stops sont touchés" }</span><b>{ usd(a.risk.loss_at_stops) }</b></p>
                        <p class="kv"><span>{ "Ligne la plus lourde" }</span><b>{ format!("{} %", to_fixed(a.risk.max_weight, 1)) }</b></p>
                        <p class="kv"><span>{ "Diversification effective" }</span><b>{ format!("{} actif(s)", to_fixed(a.risk.effective_assets, 1)) }</b></p>
                        <p class="kv"><span>{ "Corrélation moyenne" }</span><b>{ a.risk.average_correlation.map(|c| to_fixed(c, 2)).unwrap_or_else(|| "—".into()) }</b></p>
                        <p class="muted small">
                            { "Calculé sur les 90 derniers jours de cours journaliers, recoupés entre plusieurs sources. Les performances passées ne préjugent pas des performances futures." }
                        </p>
                    </div>
                    <SectorCard analysis={analysis.clone()} />
                }

                if let Some(r) = &risk_view {
                    <LimitsCard checks={r.limits.clone()} />
                    <StressCard analysis={analysis.clone()} results={r.stress.clone()} betas={betas.clone()} />
                    <WhatIfCard analysis={analysis.clone()} daily={daily.clone()} />
                }
            }

            <div class="card">
                <h2 class="card-title">{ "Sauvegarde" }</h2>
                <p class="muted small">{ "Vos avoirs sont conservés dans ce navigateur (localStorage). Exportez-les pour les garder en sécurité ou les transférer." }</p>
                <div class="row-actions">
                    <button class="btn btn-ghost" onclick={download} disabled={holdings.is_empty() && usd_h.cash == 0.0}>{ "Exporter (JSON)" }</button>
                    <button class="btn btn-ghost" onclick={download_csv} disabled={holdings.is_empty() || !ready}>{ "Tableur (CSV)" }</button>
                    <button class="btn btn-ghost" onclick={pick_file}>{ "Importer" }</button>
                    <input ref={file_input} type="file" accept="application/json,.json" hidden=true onchange={on_file} />
                </div>
            </div>

            if let Some(s) = &*editing {
                <HoldingForm
                    stored={s.clone()}
                    initial={holdings.iter().find(|h| h.id == s.id).cloned().unwrap_or_else(|| Holding {
                        id: s.id.clone(), symbol: s.symbol.clone(), kind: s.kind, name: s.name.clone(), quantity: s.quantity, average_price: s.average_price, stop: s.stop,
                    })}
                    last_price={a.lines.iter().find(|l| l.id == s.id).and_then(|l| l.price)}
                    on_close={{ let editing = editing.clone(); Callback::from(move |_| editing.set(None)) }}
                />
            }
            if *adding {
                <AddHoldings on_close={{ let adding = adding.clone(); Callback::from(move |_| adding.set(false)) }} />
            }
        </section>
    }
}

#[derive(Properties, PartialEq)]
struct AllocationBarProps {
    a: Allocation,
}

/// Allocation by class: stacked bar + legend + labels (color never the only cue).
#[component]
fn AllocationBar(p: &AllocationBarProps) -> Html {
    let parts: Vec<(&str, &str, f64)> = [("crypto", "Crypto", p.a.crypto), ("stock", "Actions", p.a.stock), ("cash", "Liquidités", p.a.cash)]
        .into_iter()
        .filter(|x| x.2 > 0.05)
        .collect();
    let label = parts.iter().map(|(_, l, v)| format!("{l} {} %", to_fixed(*v, 1))).collect::<Vec<_>>().join(", ");
    html! {
        <figure class="alloc">
            <figcaption class="muted small">{ "Répartition" }</figcaption>
            <div class="alloc-bar" role="img" aria-label={label}>
                { for parts.iter().map(|(k, l, v)| html! {
                    <span key={*k} class={format!("alloc-{k}")} style={format!("flex-grow: {};", number_to_string(*v))} title={format!("{l} : {} %", to_fixed(*v, 1))} />
                }) }
            </div>
            <ul class="alloc-legend">
                { for parts.iter().map(|(k, l, v)| html! {
                    <li key={*k}><i class={format!("alloc-{k}")} />{ format!("{l} ") }<b>{ format!("{} %", to_fixed(*v, 1)) }</b></li>
                }) }
            </ul>
        </figure>
    }
}
