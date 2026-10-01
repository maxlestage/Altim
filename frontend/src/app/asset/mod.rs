//! Asset screen /app/actif/:kind/:symbol (AssetScreen.tsx): live price, the « Décision » card, the buy zones, the
//! market guard, the anomalies, « Pourquoi ça bouge ? », the notes, the strategy comparator, then the chart, the
//! technical signal, the data reliability, the backtest and the sentiment of the chosen timeframe.
//! Sub-components live in this folder; `DecisionBadge` and the decision store are shared with the Radar.
pub mod anomalies_card;
pub mod api;
pub mod decision_card;
pub mod decision_parts;
pub mod guard_card;
pub mod note_card;
pub mod store;
pub mod strategies_card;
pub mod track_details;
pub mod why_card;
pub mod zones_card;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::engine::backtest::{BacktestResult, backtest_default, track_record};
use altim_core::engine::fibonacci::{Trend, ZoneStatus, zone_state};
use altim_core::engine::reliability::{ReliabilityLevel, gate};
use altim_core::engine::signal::{Action, AnalyzeOptions, Candle, Signal, analyze};
use altim_core::js::{round, to_fixed};
use altim_core::types::{Interval, Kind};
use altim_core::web::decision::doc::{PersonalInput, WeightInput, average_cost, portfolio_weights};
use altim_core::web::decision::reports::{GuardReport, Quote, Sentiment, Snapshot, ZonesReport};
use altim_core::web::portfolio::advice::{Advice, AdviceInput, AdviceTone, GuardContext, ZoneContext, advise_asset};
use altim_core::web::portfolio::holdings::{MarketInput, analyze_portfolio};
use altim_core::web::store::{WatchItem, asset_key};
pub use decision_card::{DecisionBadge, DecisionCard, DecisionView};
use yew::prelude::*;

use crate::api::INTERVAL_LABEL;
use crate::app::alerts::PriceAlertButton;
use crate::app::holdings::{DcaCard, PositionCard, held_market};
use crate::live::{LiveBadge, LivePrice, use_live};
use crate::route::use_on_link;
use crate::state::app::{set_app_state, use_app_state};
use crate::state::holdings::use_holdings;
use crate::ui::{Change, Gauge, PriceChart, ReliabilityBadge, Segmented, technical_text};

#[derive(Properties, PartialEq)]
pub struct AssetScreenProps {
    pub kind: Kind,
    /// Upper case, validated by `app::asset_of`.
    pub symbol: AttrValue,
}

#[derive(Clone, PartialEq)]
struct Loaded {
    snap: Rc<Snapshot>,
    signal: Option<Signal>,
    quote: Option<Quote>,
}

/// The interval chooser ("1 h · 4 h · 1 j"), shared with the Radar.
pub fn interval_chooser(interval: Interval) -> Html {
    let options: Vec<(AttrValue, AttrValue)> = INTERVAL_LABEL.iter().map(|(v, l)| (AttrValue::Static(v), AttrValue::Static(l))).collect();
    let on_change = Callback::from(|v: AttrValue| {
        if let Some(i) = Interval::parse(&v) {
            set_app_state(|s| s.interval = i);
        }
    });
    html! { <Segmented label="Unité de temps" value={AttrValue::Static(interval.as_str())} {options} {on_change} /> }
}

pub fn interval_label(i: Interval) -> &'static str {
    INTERVAL_LABEL.iter().find(|(v, _)| *v == i.as_str()).map(|(_, l)| *l).unwrap_or("")
}

#[component]
pub fn AssetScreen(p: &AssetScreenProps) -> Html {
    let on_link = use_on_link();
    let app = use_app_state();
    let _m = crate::money::use_money();
    let (kind, symbol) = (p.kind, p.symbol.to_string());
    let interval = app.interval;
    let holdings = use_holdings();
    let held = holdings.holdings.iter().find(|h| h.symbol == symbol && h.kind == kind).cloned();
    let holding_prices = use_state(|| Rc::new(HashMap::<String, f64>::new()));
    // false while the prices of the held lines load: the personal decision waits for its weights.
    let prices_ready = use_state(|| false);
    let mut keys: Vec<String> = holdings.holdings.iter().map(|h| asset_key(&h.symbol, h.kind)).collect();
    keys.sort();
    let holdings_key = keys.join(",");

    // Current value of every line (same valuation as "Mes avoirs").
    {
        let (holding_prices, prices_ready) = (holding_prices.clone(), prices_ready.clone());
        let items: Vec<(String, Kind)> = holdings.holdings.iter().map(|h| (h.symbol.clone(), h.kind)).collect();
        use_effect_with(holdings_key, move |_| {
            let alive = Rc::new(Cell::new(true));
            if items.is_empty() {
                holding_prices.set(Rc::new(HashMap::new()));
                prices_ready.set(true);
            } else {
                prices_ready.set(false);
                let a = alive.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let q = api::quotes(&items).await;
                    if !a.get() {
                        return;
                    }
                    if let Ok(q) = q {
                        holding_prices.set(Rc::new(q.into_iter().map(|x| (asset_key(&x.symbol, x.kind), x.price)).collect()));
                    }
                    prices_ready.set(true);
                });
            }
            move || alive.set(false)
        });
    }

    let data = use_state(|| None::<Loaded>);
    // Asset held: its line as « Mes avoirs » sees it (daily and 4 h signals, daily candles).
    let held_mkt = use_state(|| None::<MarketInput>);
    let bt = use_state(|| None::<Rc<BacktestResult>>);
    let sent = use_state(|| None::<Sentiment>);
    let guard_report = use_state(|| None::<GuardReport>);
    let zones_report = use_state(|| None::<ZonesReport>);
    let error = use_state(|| None::<String>);
    let show_sources = use_state(|| false);

    // Buy zones by horizon and macro context: they change with the candles, refreshed every 2 minutes.
    {
        let zones_report = zones_report.clone();
        use_effect_with((symbol.clone(), kind), move |(symbol, kind)| {
            let alive = Rc::new(Cell::new(true));
            zones_report.set(None);
            let load = {
                let (alive, symbol, kind) = (alive.clone(), symbol.clone(), *kind);
                move || {
                    let (alive, symbol, zones_report) = (alive.clone(), symbol.clone(), zones_report.clone());
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Ok(z) = api::zones(&symbol, kind).await {
                            if alive.get() {
                                zones_report.set(Some(z));
                            }
                        }
                    });
                }
            };
            load();
            let timer = crate::hooks::every_visible(120_000, load);
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }

    // Market guard: independent of the timeframe, refreshed every 2 minutes.
    {
        let guard_report = guard_report.clone();
        use_effect_with((symbol.clone(), kind), move |(symbol, kind)| {
            let alive = Rc::new(Cell::new(true));
            let load = {
                let (alive, symbol, kind) = (alive.clone(), symbol.clone(), *kind);
                move || {
                    let (alive, symbol, guard_report) = (alive.clone(), symbol.clone(), guard_report.clone());
                    wasm_bindgen_futures::spawn_local(async move {
                        if let Ok(g) = api::guard(&symbol, kind).await {
                            if alive.get() {
                                guard_report.set(Some(g));
                            }
                        }
                    });
                }
            };
            load();
            let timer = crate::hooks::every_visible(120_000, load);
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }

    let quote_name = data.as_ref().and_then(|d| d.quote.as_ref()).map(|q| q.name.clone()).filter(|n| !n.is_empty());
    let name =
        app.watchlist.iter().find(|w| w.symbol == symbol && w.kind == kind).map(|w| w.name.clone()).or(quote_name).unwrap_or_else(|| symbol.clone());

    {
        let (data, bt, sent, error, held_mkt) = (data.clone(), bt.clone(), sent.clone(), error.clone(), held_mkt.clone());
        use_effect_with((symbol.clone(), kind, interval, held.is_some()), move |(symbol, kind, interval, held)| {
            let alive = Rc::new(Cell::new(true));
            error.set(None);
            bt.set(None);
            let (a, symbol, kind, interval) = (alive.clone(), symbol.clone(), *kind, *interval);
            {
                let (symbol, a) = (symbol.clone(), a.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let higher = async {
                        match interval.higher() {
                            Some(h) => api::candles(&symbol, kind, h).await.ok(),
                            None => None,
                        }
                    };
                    let quote = async { api::quotes(&[(symbol.clone(), kind)]).await.ok().and_then(|q| q.into_iter().next()) };
                    let (snap, higher, quote) = futures::join!(api::candles(&symbol, kind, interval), higher, quote);
                    if !a.get() {
                        return;
                    }
                    match snap {
                        Ok(snap) => {
                            let options = AnalyzeOptions {
                                higher: higher.as_ref().map(|h| h.candles.as_slice()),
                                interval_ms: Some(interval.step()),
                                now: None,
                            };
                            let raw = analyze(&snap.candles, &options);
                            let signal = raw.map(|s| gate(&s, &snap.reliability, &snap.quality.issues));
                            let snap = Rc::new(snap);
                            data.set(Some(Loaded { snap: snap.clone(), signal, quote }));
                            // Backtest off the first render (heavier computation).
                            let (a, bt) = (a.clone(), bt.clone());
                            gloo::timers::callback::Timeout::new(30, move || {
                                if a.get() {
                                    bt.set(Some(Rc::new(backtest_default(&snap.candles))));
                                }
                            })
                            .forget();
                        }
                        Err(e) => error.set(Some(e.0)),
                    }
                });
            }
            {
                let (a, symbol) = (a.clone(), symbol.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(s) = api::sentiment(&symbol, kind).await {
                        if a.get() {
                            sent.set(Some(s));
                        }
                    }
                });
            }
            if *held {
                wasm_bindgen_futures::spawn_local(async move {
                    if let Some(m) = held_market(&symbol, kind).await {
                        if a.get() {
                            held_mkt.set(Some(m));
                        }
                    }
                });
            }
            move || alive.set(false)
        });
    }

    // Live prices of this asset and of every held line: amounts and advice follow the market.
    let mut live_items = vec![(symbol.clone(), kind)];
    live_items.extend(holdings.holdings.iter().map(|h| (h.symbol.clone(), h.kind)));
    let live = use_live(live_items);
    let tick = live.get(&symbol, kind).cloned();
    let loaded = (*data).clone();
    let signal = loaded.as_ref().and_then(|d| d.signal.clone());
    let quote = loaded.as_ref().and_then(|d| d.quote.clone());
    let price = tick.as_ref().map(|t| t.price).or(quote.as_ref().map(|q| q.price)).or(signal.as_ref().map(|s| s.price));
    let rel = loaded.as_ref().map(|d| d.snap.reliability.clone());
    // The chart ends on the live price (candle being formed; the signal itself only uses closed candles).
    let candles: Vec<Candle> = loaded.as_ref().map(|d| d.snap.candles.clone()).unwrap_or_default();
    let chart_candles = match (candles.last(), &tick) {
        (Some(last), Some(t)) if t.time > last.time as f64 => {
            let mut c = candles.clone();
            c.push(Candle {
                time: last.time + interval.step(),
                open: last.close,
                high: last.close.max(t.price),
                low: last.close.min(t.price),
                close: t.price,
                volume: 0.0,
            });
            c
        }
        _ => candles,
    };
    // The portfolio valued at the live prices, this asset's line with its full market data (same analysis as « Mes
    // avoirs »).
    let mut valuation: HashMap<String, MarketInput> = holding_prices
        .iter()
        .map(|(k, p)| (k.clone(), MarketInput { price: live.ticks.get(k).map(|t| t.price).unwrap_or(*p), ..Default::default() }))
        .collect();
    if let (Some(_), Some(m)) = (&held, &*held_mkt) {
        valuation.insert(asset_key(&symbol, kind), MarketInput { price: tick.as_ref().map(|t| t.price).unwrap_or(m.price), ..m.clone() });
    }
    let portfolio = analyze_portfolio(&holdings.holdings, holdings.cash, &valuation);
    let line = held.as_ref().filter(|_| held_mkt.is_some()).and_then(|h| portfolio.lines.iter().find(|l| l.id == h.id));
    // Buy zone of the user's horizon, at the live price.
    let raw_zone = zones_report.as_ref().and_then(|z| z.zones.iter().find(|z| z.zone.horizon == zones_card::horizon_of(app.horizon)));
    let my_zone = raw_zone.filter(|z| z.zone.status != ZoneStatus::None).map(|z| {
        let (status, text) = match (price.filter(|p| *p != 0.0), &z.zone.swing) {
            (Some(p), Some(sw)) if sw.trend == Trend::Up => {
                let st = zone_state(sw, p);
                (st.status, st.text)
            }
            _ => (z.zone.status, z.zone.text.clone()),
        };
        ZoneContext { label: z.zone.label.clone(), status: status.as_str().into(), text, macro_note: z.macro_note.clone() }
    });
    let advice = loaded.as_ref().map(|_| {
        let mut input = AdviceInput::new(signal.as_ref(), rel.as_ref().map(|r| r.level), price, app.risk);
        input.line = line;
        input.capital = Some(portfolio.total);
        input.track = bt.as_ref().map(|b| track_record(b));
        input.symbol = symbol.clone();
        input.kind = kind;
        input.guard = guard_report.as_ref().map(|g| GuardContext {
            shock: g.result.shock.level,
            reversal_score: g.result.reversal.score,
            reversal_direction: g.result.reversal.direction,
        });
        input.zone = my_zone;
        advise_asset(&input)
    });
    let in_watchlist = app.watchlist.iter().any(|w| w.symbol == symbol && w.kind == kind);
    // Position size: the plan's stop when under the price, else the low that invalidates the buy zone, else 5 % under.
    let (stop, stop_source, target) = match (&signal, raw_zone.map(|z| &z.zone)) {
        (Some(s), _) if s.has_plan && s.stop_loss < price.unwrap_or(f64::INFINITY) => {
            (Some(s.stop_loss), "stop du plan (2 × ATR)", (s.take_profit > price.unwrap_or(0.0)).then_some(s.take_profit))
        }
        (_, Some(z)) if z.invalidation.is_some_and(|i| i != 0.0 && i < price.unwrap_or(0.0)) => {
            (z.invalidation, "plus bas qui invalide la zone d'achat", z.targets.iter().copied().find(|t| *t > price.unwrap_or(0.0)))
        }
        _ => (price.filter(|p| *p != 0.0).map(|p| p * 0.95), "5 % sous le prix (à ajuster)", None),
    };

    // Personal decision: average cost and each line's share of the portfolio (never quantities nor amounts). Prices
    // of the first load, not the live ticks, so the request does not change at every tick.
    let personal = held.as_ref().map(|_| {
        let lines: Vec<WeightInput> = holdings
            .holdings
            .iter()
            .map(|h| WeightInput { symbol: h.symbol.clone(), kind: h.kind, quantity: h.quantity, average_price: h.average_price })
            .collect();
        PersonalInput {
            cost: average_cost(&lines, &symbol, kind),
            weights: portfolio_weights(&lines, |k| holding_prices.get(k).copied(), holdings.cash, 20),
        }
    });
    let label = interval_label(interval);
    let toggle_sources = {
        let show_sources = show_sources.clone();
        Callback::from(move |_| show_sources.set(!*show_sources))
    };

    html! {
        <section class="app-screen asset-screen">
            <a href="/app" onclick={on_link.clone()} class="back">{ "← Radar" }</a>
            <div class="asset-head">
                <div>
                    <h1>{ name.clone() }</h1>
                    <small class="muted mono">{ format!("{symbol} · {}", if kind == Kind::Crypto { "Crypto" } else { "Action" }) }</small>
                </div>
                <div class="asset-head-price">
                    <b class="mono"><LivePrice tick={tick.clone()} fallback={price} format={Callback::from(crate::money::price)} /></b>
                    <Change value={tick.as_ref().and_then(|t| t.change).or(quote.as_ref().and_then(|q| q.change))} />
                    <LiveBadge status={live.status} last={live.last} />
                    if tick.as_ref().and_then(|t| t.market.as_deref()) == Some("closed") {
                        <small class="market-closed">{ "Bourse fermée · dernier cours" }</small>
                    }
                    { match (&tick, &quote) {
                        (Some(t), _) => html! { <small class="muted">{ format!("prix : {}/{} sources en direct", t.agreeing, t.total) }</small> },
                        (None, Some(q)) => html! { <small class="muted">{ format!("prix : {}/{} sources", q.agreeing, q.total) }</small> },
                        _ => html! {},
                    } }
                </div>
            </div>
            <PriceAlertButton symbol={p.symbol.clone()} {kind} name={name.clone()} price={tick.as_ref().map(|t| t.price).or(price)} />

            <DecisionCard symbol={p.symbol.clone()} {kind} {personal} ready={held.is_none() || *prices_ready} live_price={tick.as_ref().map(|t| t.price)} />

            { interval_chooser(interval) }

            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e}") }</p>
            }
            if loaded.is_none() && error.is_none() {
                <div class="skeleton tall" />
            }

            if let Some(a) = &advice {
                { advice_card(a, label, held.is_some(), in_watchlist, &on_link, &name, (&symbol, kind)) }
            }

            { match &*zones_report {
                Some(z) => html! { <zones_card::ZonesCard report={z.clone()} {price} horizon={app.horizon} /> },
                None if loaded.is_some() => html! { <div class="skeleton" aria-label="Chargement des zones d'achat" /> },
                None => html! {},
            } }

            { match &*guard_report {
                Some(g) => html! { <guard_card::GuardCard g={g.clone()} /> },
                None if loaded.is_some() => html! { <div class="skeleton" aria-label="Chargement du garde-fou" /> },
                None => html! {},
            } }
            <anomalies_card::AnomaliesCard symbol={p.symbol.clone()} {kind} />

            <why_card::WhyCard symbol={p.symbol.clone()} {kind} />

            <PositionCard
                symbol={p.symbol.clone()}
                {price}
                {stop}
                {stop_source}
                {target}
                capital={portfolio.total}
                risk_pct={app.risk.risk_per_trade_percent}
            />

            <note_card::NoteCard id={format!("{}:{symbol}", kind.as_str())} symbol={p.symbol.clone()} />

            <DcaCard symbol={p.symbol.clone()} {kind} />
            <strategies_card::StrategiesCard symbol={p.symbol.clone()} {kind} />

            if let Some(d) = &loaded {
                <div class="asset-grid">
                    <div class="card chart-box">
                        // No stop/target drawn from the technical signal: the only plan is the Décision card's.
                        <PriceChart candles={chart_candles} trades={bt.as_ref().map(|b| b.trades.clone()).unwrap_or_default()} />
                        <div class="legend">
                            <span><i class="l-price" />{ " Prix" }</span>
                            <span><i class="l-e20" />{ " EMA 20" }</span>
                            <span><i class="l-e50" />{ " EMA 50" }</span>
                            if bt.is_some() {
                                <span><i class="l-trade" />{ " Entrées backtest" }</span>
                            }
                        </div>
                    </div>

                    { match &signal {
                        Some(s) => signal_card(s, label),
                        None => html! { <p class="notice warn">{ "Historique insuffisant pour calculer un signal sur cette unité de temps." }</p> },
                    } }

                    if let Some(rel) = &rel {
                        <div class={format!("card rel-card {}", match rel.level { ReliabilityLevel::High => "high", ReliabilityLevel::Medium => "medium", ReliabilityLevel::Low => "low" })}>
                            <h2 class="card-title">{ "Fiabilité des données" }</h2>
                            <div class="rel-top">
                                <ReliabilityBadge rel={rel.clone()} />
                                <b class="mono">{ format!("{}/100", round(rel.score)) }</b>
                            </div>
                            <p class="muted small">
                                { "Bougies de " }<b>{ d.snap.source.clone() }</b>
                                { format!(", recoupées : {} source(s) concordante(s) sur {}.", d.snap.agreeing, d.snap.sources.len()) }
                                if rel.level == ReliabilityLevel::Medium && d.snap.kind == Kind::Stock {
                                    { " En intraday, une seule source indépendante est disponible pour les actions : les signaux forts sont ramenés à normaux." }
                                }
                            </p>
                            <button class="link-btn" onclick={toggle_sources} aria-expanded={show_sources.to_string()}>
                                { if *show_sources { "Masquer les sources" } else { "Voir les sources" } }
                            </button>
                            if *show_sources {
                                <ul class="source-list">
                                    { for d.snap.sources.iter().map(|s| html! {
                                        <li key={s.name.clone()}>
                                            <span class={if s.ok { "up" } else { "down" }}>{ if s.ok { "●" } else { "○" } }</span>{ format!(" {}", s.name) }
                                            <small class="muted">
                                                { match (s.ok, s.deviation.filter(|v| v.is_finite())) {
                                                    (true, Some(dev)) => format!("écart {} %", to_fixed(dev, 3)),
                                                    _ => s.error.clone().unwrap_or_else(|| "écartée".into()),
                                                } }
                                            </small>
                                        </li>
                                    }) }
                                    if let Some(q) = &d.quote {
                                        { for q.sources.iter().map(|s| html! {
                                            <li key={format!("q-{}", s.name)}>
                                                <span class={if s.ok { "up" } else { "down" }}>{ if s.ok { "●" } else { "○" } }</span>{ format!(" {} ", s.name) }
                                                <small class="muted">
                                                    { format!("(cours) {}", match s.price.filter(|p| *p != 0.0) { Some(p) => crate::money::price(p), None => s.error.clone().unwrap_or_default() }) }
                                                </small>
                                            </li>
                                        }) }
                                    }
                                    { for d.snap.quality.issues.iter().map(|i| html! { <li key={i.clone()} class="warn small">{ format!("⚠ {i}") }</li> }) }
                                </ul>
                            }
                        </div>
                    }

                    <div class="card bt-card">
                        <h2 class="card-title">{ format!("Backtest · {label} · historique chargé") }</h2>
                        { match &*bt {
                            Some(b) => html! {
                                <>
                                    <div class="metrics">
                                        <div><small>{ "Stratégie" }</small><b><Change value={Some(b.total_return_percent)} /></b></div>
                                        <div><small>{ "Achat-conservation" }</small><b><Change value={Some(b.buy_and_hold_percent)} /></b></div>
                                        <div><small>{ "Trades" }</small><b>{ b.trades.len() }</b></div>
                                        <div><small>{ "Réussite" }</small><b>{ format!("{} %", to_fixed(b.win_rate_percent, 0)) }</b></div>
                                        <div><small>{ "Drawdown max" }</small><b class="down">{ format!("−{} %", to_fixed(b.max_drawdown_percent, 1)) }</b></div>
                                    </div>
                                    <p class="muted small">
                                        { if b.trades.len() < 5 {
                                            "Trop peu de trades pour conclure : prudence."
                                        } else if b.total_return_percent > b.buy_and_hold_percent {
                                            "La stratégie a fait mieux que l'achat-conservation sur cette période (frais de 0,1 % inclus). Les performances passées ne préjugent pas des performances futures."
                                        } else {
                                            "Sur cette période, conserver l'actif a mieux rapporté que suivre les signaux. La stratégie sert surtout à limiter les pertes en marché baissier."
                                        } }
                                    </p>
                                </>
                            },
                            None => html! { <div class="skeleton" /> },
                        } }
                    </div>

                    { sentiment_card(&sent) }
                </div>
            }
        </section>
    }
}

/// « Lecture du signal · 4 h »: the advice of one timeframe (`adviseAsset`), below the decision that prevails.
fn advice_card(
    a: &Advice,
    label: &str,
    held: bool,
    in_watchlist: bool,
    on_link: &Callback<MouseEvent>,
    name: &str,
    (symbol, kind): (&str, Kind),
) -> Html {
    let tone = match a.tone {
        AdviceTone::Buy => "buy",
        AdviceTone::Hold => "hold",
        AdviceTone::Sell => "sell",
        AdviceTone::Unknown => "unknown",
    };
    let add = {
        let item = WatchItem { symbol: symbol.to_string(), kind, name: name.to_string() };
        Callback::from(move |_: MouseEvent| {
            let item = item.clone();
            set_app_state(move |s| s.watchlist.push(item));
        })
    };
    html! {
        <div class={format!("card advice advice-{tone}")}>
            <h2 class="card-title">{ format!("Lecture du signal · {label}") }</h2>
            <p class="muted small">{ "Une seule unité de temps et le signal technique : la décision en haut de page réunit toutes les familles, les interdictions d'achat et le rapport gain/risque, et c'est elle qui prime." }</p>
            <p class="advice-title">{ a.title.clone() }</p>
            <ul>{ for a.points.iter().map(|p| html! { <li>{ p.clone() }</li> }) }</ul>
            <div class="advice-actions">
                <a href="/app/avoirs" onclick={on_link.clone()} class="link">{ if held { "Voir mes avoirs" } else { "J'en possède déjà" } }</a>
                if !in_watchlist {
                    <button class="link-btn" onclick={add}>{ "+ Ajouter au radar" }</button>
                }
            </div>
            <p class="muted small">{ "Conseil indicatif, pas une recommandation d'investissement personnalisée. Altim ne passe aucun ordre." }</p>
        </div>
    }
}

/// "Signal technique · 4 h": the gauge, the direction, the factors and the warnings.
fn signal_card(s: &Signal, label: &str) -> Html {
    let dir = match s.action {
        Action::Buy | Action::StrongBuy => "up",
        Action::Sell | Action::StrongSell => "down",
        Action::Hold => "",
    };
    html! {
        <div class="card signal-card">
            <h2 class="card-title">{ format!("Signal technique · {label}") }</h2>
            <p class="muted small">{ "Un indice parmi d'autres : le verdict à suivre est celui de la carte Décision, qui y ajoute les interdictions d'achat, la zone d'achat, le gain/risque, l'agenda et la preuve du modèle." }</p>
            <div class="signal-top">
                <Gauge score={s.score} size={150.0} />
                <div class="signal-meta">
                    <b class={format!("tech-dir {dir}")}>{ format!("Tendance technique : {}", technical_text(s.action)) }</b>
                    <small class="muted">{ format!("confiance {} %", round(s.confidence)) }</small>
                    <small class="muted">{ format!("bougie du {}", crate::ui::fr_date_time_short(s.time as f64)) }</small>
                </div>
            </div>
            <ul class="factors">
                { for s.factors.iter().map(|f| {
                    let w = f.score.abs() * 50.0;
                    let n = altim_core::js::number_to_string;
                    let style = if f.score >= 0.0 { format!("width: {}%; left: 50%;", n(w)) } else { format!("width: {}%; left: {}%;", n(w), n(50.0 - w)) };
                    html! {
                        <li key={f.name.clone()}>
                            <div class="factor-head"><span>{ f.name.clone() }</span><small class="muted">{ f.detail.clone() }</small></div>
                            <div class="bar"><i class={if f.score >= 0.0 { "pos" } else { "neg" }} {style} /></div>
                        </li>
                    }
                }) }
            </ul>
            { for s.warnings.iter().map(|w| html! { <p key={w.clone()} class="warn small">{ format!("⚠ {w}") }</p> }) }
        </div>
    }
}

fn sentiment_card(sent: &Option<Sentiment>) -> Html {
    let Some(s) = sent else { return html! {} };
    let bullish = s.social.as_ref().and_then(|x| x.bullish_percent.map(|b| (b, x.sample)));
    if s.fear_greed.is_none() && bullish.is_none() {
        return html! {};
    }
    html! {
        <div class="card">
            <h2 class="card-title">{ "Sentiment du marché" }</h2>
            if let Some(fg) = &s.fear_greed {
                <p class="kv"><span>{ "Fear & Greed crypto" }</span><b>{ format!("{} · {}", altim_core::js::number_to_string(fg.value), fg.label) }</b></p>
            }
            if let Some((b, sample)) = bullish {
                <p class="kv">
                    <span>{ "StockTwits" }</span>
                    <b class={if b >= 50.0 { "up" } else { "down" }}>{ format!("{} % haussier ({} avis)", to_fixed(b, 0), altim_core::js::number_to_string(sample)) }</b>
                </p>
            }
            <p class="muted small">{ "Contexte, non intégré au score : la foule se trompe souvent aux extrêmes." }</p>
        </div>
    }
}
