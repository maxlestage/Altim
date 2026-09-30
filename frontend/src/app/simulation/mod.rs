//! Simulation (phase 2, batch D: port of web/src/webapp/Simulation.tsx, PaperOrder.tsx, paper-store.ts, paper-ui.ts):
//! the simulated portfolio (no real money, no order), its open positions, closed trades and statistics.
//! `SimulateBuy` (the "Simuler cet achat" block of the Décision card) is exported for batch B.
mod order;
pub mod store;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::js::{fr, number_to_string, round};
use altim_core::types::Kind;
use altim_core::web::market::Tick;
use altim_core::web::money::Currency;
use altim_core::web::trading::paper::{
    DEFAULT_CAPITAL, DailyCandle, OpenLine, PaperStats, PaperTrade, check_exits, close_position, paper_stats, valuation,
};
use altim_core::web::trading::paper_ui::{
    FEW_TRADES, FEW_TRADES_NOTE, assets_to_check, exit_notice, fr_date, journal, pct, price as paper_price, qty, reason_label, signed_pct,
    signed_usd, start_capital, to_daily, usd, verdict_rows,
};
use yew::prelude::*;

use crate::app::common::{FxNote, PortfolioTab, PortfolioTabs};
use crate::live::{LiveBadge, use_live};
use crate::route::use_on_link;
pub use order::SimulateBuy;
use store::{get_paper, reset_paper, set_paper, use_paper};

fn up_down(v: Option<f64>) -> Option<&'static str> {
    match v {
        Some(v) if v > 0.0 => Some("up"),
        Some(v) if v < 0.0 => Some("down"),
        _ => None,
    }
}

const CHECK_EVERY: u32 = 300_000;

fn visible() -> bool {
    gloo::utils::document().visibility_state() == web_sys::VisibilityState::Visible
}

/// Exit notices and the state of the automatic checks (updated from async tasks, hence a reducer).
#[derive(Default, PartialEq)]
struct Checks {
    notices: Vec<String>,
    running: bool,
    at: Option<f64>,
    error: Option<String>,
}

enum CheckAction {
    Running,
    Done { at: f64, error: Option<String>, notices: Vec<String> },
    ClearNotices,
}

impl Reducible for Checks {
    type Action = CheckAction;
    fn reduce(self: Rc<Self>, action: CheckAction) -> Rc<Self> {
        let mut s = Checks { notices: self.notices.clone(), running: self.running, at: self.at, error: self.error.clone() };
        match action {
            CheckAction::Running => s.running = true,
            CheckAction::Done { at, error, notices } => {
                s.running = false;
                s.at = Some(at);
                s.error = error;
                s.notices = notices.into_iter().chain(s.notices).collect();
            }
            CheckAction::ClearNotices => s.notices.clear(),
        }
        Rc::new(s)
    }
}

/// Closes the positions whose stop or target was reached (daily candles of the positions with a stop or a target).
async fn run_checks(checks: UseReducerDispatcher<Checks>, alive: Rc<Cell<bool>>) {
    let Some(s) = get_paper() else { return };
    let list = assets_to_check(&s.positions);
    if list.is_empty() {
        return;
    }
    checks.dispatch(CheckAction::Running);
    let calls = list.iter().map(|(symbol, kind)| crate::app::journal::store::daily_candles(symbol, *kind));
    let results = futures::future::join_all(calls).await;
    if !alive.get() {
        return;
    }
    let mut candles: HashMap<String, Vec<DailyCandle>> = HashMap::new();
    let mut failed = 0;
    for ((symbol, kind), r) in list.iter().zip(results) {
        match r {
            Ok(c) => {
                candles.insert(format!("{}:{symbol}", kind.as_str()), to_daily(&c));
            }
            Err(_) => failed += 1,
        }
    }
    let mut notices = Vec::new();
    if let Some(latest) = get_paper() {
        let (next, closed) = check_exits(&latest, &candles);
        if !closed.is_empty() {
            set_paper(next);
            let d = crate::money::display();
            notices = closed.iter().map(|t| exit_notice(t, &d)).collect();
        }
    }
    let error = (failed > 0).then(|| format!("{failed} actif{} sans bougies (nouvel essai dans 5 min).", if failed > 1 { "s" } else { "" }));
    checks.dispatch(CheckAction::Done { at: js_sys::Date::now(), error, notices });
}

/// Paper trading screen (/app/simulation): simulated portfolio, open positions, journal and statistics.
#[component]
pub fn Simulation() -> Html {
    let m = crate::money::use_money();
    let on_link = use_on_link();
    let saved = use_paper();
    let state = saved.state.as_ref();
    let quotes = use_state(HashMap::<String, f64>::new);
    let checks = use_reducer(Checks::default);
    let restarting = use_state(|| false);
    let sell_error = use_state(|| None::<String>);

    let positions = state.map(|s| s.positions.clone()).unwrap_or_default();
    let mut assets: Vec<(String, Kind)> = Vec::new();
    for p in &positions {
        if !assets.iter().any(|(s, k)| *s == p.symbol && *k == p.kind) {
            assets.push((p.symbol.clone(), p.kind));
        }
    }
    let mut keys: Vec<String> = assets.iter().map(|(s, k)| format!("{}:{s}", k.as_str())).collect();
    keys.sort();
    let assets_key = keys.join(",");

    // Live prices (same stream as the rest of the app) + consensus quotes as a fallback.
    let live = use_live(assets.clone());
    {
        let (quotes, assets) = (quotes.clone(), assets.clone());
        use_effect_with(assets_key.clone(), move |_| {
            let alive = Rc::new(Cell::new(true));
            let mut timer = None;
            if !assets.is_empty() {
                let load = {
                    let (alive, quotes, assets) = (alive.clone(), quotes.clone(), assets.clone());
                    move || {
                        let (alive, quotes, assets) = (alive.clone(), quotes.clone(), assets.clone());
                        wasm_bindgen_futures::spawn_local(async move {
                            if let Ok(q) = crate::api::batched::<Tick>(&assets, |list| format!("/api/tickers?symbols={list}")).await {
                                if alive.get() {
                                    quotes.set(q.into_iter().map(|x| (format!("{}:{}", x.kind.as_str(), x.symbol), x.price)).collect());
                                }
                            }
                        });
                    }
                };
                load();
                timer = Some(gloo::timers::callback::Interval::new(120_000, move || {
                    if visible() {
                        load()
                    }
                }));
            }
            move || {
                alive.set(false);
                drop(timer);
            }
        });
    }
    let mut prices: HashMap<String, f64> = (*quotes).clone();
    for (k, t) in live.ticks.iter() {
        prices.insert(k.clone(), t.price);
    }

    // Automatic exits: daily candles of the positions with a stop or a target, on opening and every 5 min while visible.
    {
        let dispatcher = checks.dispatcher();
        use_effect_with(assets_key, move |_| {
            let alive = Rc::new(Cell::new(true));
            let run = {
                let alive = alive.clone();
                move || wasm_bindgen_futures::spawn_local(run_checks(dispatcher.clone(), alive.clone()))
            };
            run();
            let timer = {
                let run = run.clone();
                gloo::timers::callback::Interval::new(CHECK_EVERY, move || {
                    if visible() {
                        run()
                    }
                })
            };
            let on_visible = gloo::events::EventListener::new(&gloo::utils::document(), "visibilitychange", move |_| {
                if visible() {
                    run()
                }
            });
            move || {
                alive.set(false);
                drop(timer);
                drop(on_visible);
            }
        });
    }

    let val = state.map(|s| valuation(s, |k| prices.get(k).copied()));
    let stats = state.map(paper_stats);
    let sell = {
        let (sell_error, prices) = (sell_error.clone(), prices.clone());
        Callback::from(move |l: Rc<OpenLine>| {
            sell_error.set(None);
            let d = crate::money::display();
            let Some(p) = prices.get(&format!("{}:{}", l.kind.as_str(), l.symbol)).copied().filter(|p| *p > 0.0) else {
                sell_error.set(Some(format!("Cours de {} indisponible : vente simulée impossible pour l'instant.", l.name)));
                return;
            };
            let estimate = match (l.value, l.pnl) {
                (Some(_), Some(pnl)) => format!(" Résultat estimé : {} ({}).", signed_usd(&d, pnl), signed_pct(l.pnl_pct.unwrap_or(0.0), 2)),
                _ => String::new(),
            };
            let msg =
                format!("Vendre (simulé) toute la position {} au cours de {} ?{estimate}\nAucun ordre réel n'est passé.", l.name, paper_price(&d, p));
            if !web_sys::window().and_then(|w| w.confirm_with_message(&msg).ok()).unwrap_or(false) {
                return;
            }
            let Some(s) = get_paper() else { return };
            match close_position(&s, &l.id, p, js_sys::Date::now()) {
                Ok(next) => set_paper(next),
                Err(e) => sell_error.set(Some(e)),
            }
        })
    };
    let n_pos = positions.len();
    let to_check = !assets_to_check(&positions).is_empty();

    html! {
        <section class="app-screen simulation">
            <PortfolioTabs active={PortfolioTab::Paper} />
            <div class="screen-top">
                <h1>{ "Simulation" }</h1>
                if state.is_some() {
                    <button class="btn btn-small btn-ghost" onclick={{ let r = restarting.clone(); Callback::from(move |_| r.set(true)) }}>{ "Recommencer" }</button>
                }
            </div>
            <p class="paper-banner"><span aria-hidden="true">{ "🧪" }</span>{ " Portefeuille simulé — aucun argent réel, aucun ordre passé" }</p>

            if let Some(e) = &saved.error {
                <p class="notice warn" role="alert">{ format!("⚠ {e}") }</p>
            }

            if !checks.notices.is_empty() {
                <div class="paper-notices" role="status">
                    { for checks.notices.iter().map(|n| html! { <p key={n.clone()} class="notice small">{ format!("🔔 {n}") }</p> }) }
                    <button class="link-btn" onclick={{ let c = checks.dispatcher(); Callback::from(move |_| c.dispatch(CheckAction::ClearNotices)) }}>{ "Masquer ces avis" }</button>
                </div>
            }

            if let (Some(state), Some(val), Some(stats)) = (state, val.as_ref(), stats) {
                <div class="card paper-summary">
                    <div class="summary-top">
                        <small class="muted">{ "Valeur du portefeuille simulé" }</small>
                        if n_pos > 0 {
                            <LiveBadge status={live.status} last={live.last} />
                        }
                    </div>
                    <b class="mono big">{ usd(&m, val.equity) }</b>
                    <p class={classes!("paper-pnl", "mono", up_down(Some(val.pnl)))}>
                        { format!("{} ({}) ", signed_usd(&m, val.pnl), signed_pct(val.pnl_pct, 2)) }<span class="muted">{ "depuis le début" }</span>
                    </p>
                    <dl class="paper-figures">
                        <div><dt>{ "Capital de départ" }</dt><dd class="mono">{ usd(&m, state.start_capital) }</dd></div>
                        <div><dt>{ "Liquidités" }</dt><dd class="mono">{ usd(&m, state.cash) }</dd></div>
                        <div><dt>{ "Positions ouvertes" }</dt><dd class="mono">{ usd(&m, val.positions_value) }</dd></div>
                        <div><dt>{ "Début" }</dt><dd>{ fr_date(state.started_at, false) }</dd></div>
                    </dl>
                    if val.unpriced > 0 {
                        <p class="muted small">
                            { format!("{0} position{1} sans cours pour l'instant : comptée{1} à son prix d'achat.", val.unpriced, if val.unpriced > 1 { "s" } else { "" }) }
                        </p>
                    }
                    <p class="muted small">{ "La valeur compte les frais et le glissement d'une vente immédiate." }</p>
                    if m.currency() == Currency::Eur {
                        <p class="muted small">{ "Portefeuille simulé tenu en $ comme les cours ; montants saisis en € convertis au taux du jour de la saisie, affichés au taux du jour." }</p>
                    }
                    <FxNote />
                </div>

                <div class="card">
                    <h2 class="card-title">{ format!("Positions ouvertes · {n_pos}") }</h2>
                    if n_pos == 0 {
                        <p class="muted small">
                            { "Aucune position. Ouvrez la fiche d'un actif (depuis le " }<a href="/app" onclick={on_link.clone()} class="link">{ "Radar" }</a>
                            { ") et touchez « Simuler cet achat » dans la carte Décision." }
                        </p>
                    }
                    if let Some(e) = &*sell_error {
                        <p class="notice danger small" role="alert">{ format!("✕ {e}") }</p>
                    }
                    if checks.running {
                        <p class="muted small" role="status">{ "Vérification des stops et objectifs sur les bougies journalières…" }</p>
                    }
                    if let Some(at) = checks.at.filter(|_| !checks.running && to_check) {
                        <p class="muted small">{ format!("Stops et objectifs vérifiés à {} (toutes les 5 min).", crate::ui::fr_time(at)) }</p>
                    }
                    if let Some(e) = &checks.error {
                        <p class="notice warn small">{ format!("⚠ {e}") }</p>
                    }
                    if !val.lines.is_empty() {
                        <ul class="paper-positions">
                            { for val.lines.iter().map(|l| {
                                let l = Rc::new(l.clone());
                                let on_sell = { let (sell, l) = (sell.clone(), l.clone()); Callback::from(move |_| sell.emit(l.clone())) };
                                let key = l.id.clone();
                                html! { <PositionCard {key} {l} {on_sell} /> }
                            }) }
                        </ul>
                    }
                </div>

                <StatsCard stats={Rc::new(stats)} />

                <div class="card">
                    <h2 class="card-title">{ format!("Journal des trades clôturés · {}", state.trades.len()) }</h2>
                    if state.trades.is_empty() {
                        <p class="muted small">{ "Aucun trade clôturé pour l'instant : les ventes (manuelles, au stop ou à l'objectif) apparaîtront ici, la plus récente en premier." }</p>
                    } else {
                        <TradesTable trades={Rc::new(journal(&state.trades))} />
                    }
                </div>
            } else {
                <StartCard on_start={Callback::from(|c: f64| reset_paper(crate::money::from_display(c)))} />
            }

            <Rules />

            if *restarting {
                <RestartSheet
                    on_close={{ let r = restarting.clone(); Callback::from(move |_| r.set(false)) }}
                    on_confirm={{
                        let (r, c) = (restarting.clone(), checks.dispatcher());
                        Callback::from(move |cap: f64| {
                            reset_paper(crate::money::from_display(cap));
                            c.dispatch(CheckAction::ClearNotices);
                            r.set(false);
                        })
                    }}
                />
            }
        </section>
    }
}

#[derive(Properties, PartialEq)]
struct PositionCardProps {
    l: Rc<OpenLine>,
    on_sell: Callback<MouseEvent>,
}

#[component]
fn PositionCard(p: &PositionCardProps) -> Html {
    let m = crate::money::use_money();
    let on_link = use_on_link();
    let l = &p.l;
    let fmt = |v: f64| paper_price(&m, v);
    html! {
        <li class="paper-pos">
            <div class="holding-head">
                <a href={format!("/app/actif/{}/{}", l.kind.as_str(), l.symbol)} onclick={on_link} class="holding-name">
                    <b>{ l.name.clone() }</b>
                    <small class="muted mono">{ format!("{} {} · investi {} · ouvert le {}", qty(l.quantity), l.symbol, usd(&m, l.invested), fr_date(l.opened_at, false)) }</small>
                </a>
                <b class={classes!("mono", "paper-pos-pnl", up_down(l.pnl))}>{ if l.pnl.is_some() { signed_pct(l.pnl_pct.unwrap_or(0.0), 2) } else { "—".into() } }</b>
            </div>
            <p class="paper-decision small">
                if let Some(d) = &l.decision {
                    { "Décision à l'achat : " }<b>{ d.label.clone() }</b>
                    { format!(" · confiance {}/100 · du {}", number_to_string(round(d.confidence)), fr_date(d.as_of, true)) }
                } else {
                    { "Achat sans décision affichée" }
                }
            </p>
            <dl class="paper-figures six">
                <div><dt>{ "Entrée" }</dt><dd class="mono">{ fmt(l.entry) }</dd></div>
                <div><dt>{ "Cours" }</dt><dd class="mono">{ l.price.map(fmt).unwrap_or_else(|| "indisponible".into()) }</dd></div>
                <div><dt>{ "Valeur" }</dt><dd class="mono">{ l.value.map(|v| usd(&m, v)).unwrap_or_else(|| "—".into()) }</dd></div>
                <div><dt>{ "Gain / perte" }</dt><dd class={classes!("mono", up_down(l.pnl))}>{ l.pnl.map(|v| signed_usd(&m, v)).unwrap_or_else(|| "—".into()) }</dd></div>
                <div><dt>{ "Stop" }</dt><dd class="mono">{ l.stop.map(fmt).unwrap_or_else(|| "aucun".into()) }</dd></div>
                <div><dt>{ "Objectif" }</dt><dd class="mono">{ l.target.map(fmt).unwrap_or_else(|| "aucun".into()) }</dd></div>
            </dl>
            <button class="btn btn-ghost btn-small sell-sim" onclick={p.on_sell.clone()} disabled={l.price.is_none()} aria-label={format!("Vendre (simulé) {}", l.name)}>
                { "Vendre (simulé)" }
            </button>
        </li>
    }
}

#[derive(Properties, PartialEq)]
struct TradesTableProps {
    trades: Rc<Vec<PaperTrade>>,
}

#[component]
fn TradesTable(p: &TradesTableProps) -> Html {
    let m = crate::money::use_money();
    let fmt = |v: f64| paper_price(&m, v);
    html! {
        <table class="paper-table">
            <caption class="sr-only">{ "Trades clôturés, le plus récent en premier" }</caption>
            <thead>
                <tr>
                    <th scope="col">{ "Actif" }</th><th scope="col">{ "Achat" }</th><th scope="col">{ "Vente" }</th><th scope="col">{ "Entrée" }</th>
                    <th scope="col">{ "Sortie" }</th><th scope="col">{ "Motif" }</th><th scope="col">{ "Résultat" }</th>
                </tr>
            </thead>
            <tbody>
                { for p.trades.iter().map(|t| html! {
                    <tr key={t.id.clone()}>
                        <th scope="row" data-label="Actif">
                            <b>{ t.name.clone() }</b>{ " " }<small class="muted mono">{ t.symbol.clone() }</small>
                            <small class="muted paper-trade-dec">
                                { t.decision.as_ref().map(|d| format!("{} · {}/100", d.label, number_to_string(round(d.confidence)))).unwrap_or_else(|| "sans décision".into()) }
                            </small>
                        </th>
                        <td data-label="Achat">{ fr_date(t.opened_at, false) }</td>
                        <td data-label="Vente">{ fr_date(t.closed_at, false) }</td>
                        <td data-label="Entrée" class="mono">{ fmt(t.entry) }</td>
                        <td data-label="Sortie" class="mono">{ fmt(t.exit) }</td>
                        <td data-label="Motif">{ reason_label(t.reason) }</td>
                        <td data-label="Résultat" class={classes!("mono", up_down(Some(t.pnl)))}>{ format!("{} ({})", signed_usd(&m, t.pnl), signed_pct(t.pnl_pct, 2)) }</td>
                    </tr>
                }) }
            </tbody>
        </table>
    }
}

#[derive(Properties, PartialEq)]
struct StatsCardProps {
    stats: Rc<PaperStats>,
}

#[component]
fn StatsCard(p: &StatsCardProps) -> Html {
    let m = crate::money::use_money();
    let stats = &p.stats;
    let rows = verdict_rows(&stats.by_verdict);
    let s = if stats.trades > 1 { "s" } else { "" };
    html! {
        <div class="card">
            <h2 class="card-title">{ "Statistiques" }</h2>
            if stats.trades == 0 {
                <p class="muted small">{ "Les statistiques se calculent sur les trades clôturés : aucun pour l'instant." }</p>
            } else {
                <dl class="paper-figures">
                    <div><dt>{ "Trades clôturés" }</dt><dd class="mono">{ stats.trades }</dd></div>
                    <div><dt>{ "% gagnants" }</dt><dd class="mono">{ format!("{} ", pct(stats.win_rate, 0)) }<small class="muted">{ format!("({}/{})", stats.wins, stats.trades) }</small></dd></div>
                    <div><dt>{ "Gain moyen" }</dt><dd class="mono up">{ stats.avg_win_pct.map(|v| signed_pct(v, 2)).unwrap_or_else(|| "aucun gain".into()) }</dd></div>
                    <div><dt>{ "Perte moyenne" }</dt><dd class="mono down">{ stats.avg_loss_pct.map(|v| signed_pct(v, 2)).unwrap_or_else(|| "aucune perte".into()) }</dd></div>
                    <div><dt>{ "Profit factor" }</dt><dd class="mono">{ stats.profit_factor.map(|v| fr(v, 0, 2)).unwrap_or_else(|| "— (aucune perte)".into()) }</dd></div>
                    <div><dt>{ "Pire recul" }</dt><dd class="mono">{ signed_pct(stats.max_drawdown_pct, 1) }</dd></div>
                    <div><dt>{ "Résultat réalisé" }</dt><dd class={classes!("mono", up_down(Some(stats.realized_pnl)))}>{ signed_usd(&m, stats.realized_pnl) }</dd></div>
                    <div>
                        <dt>{ "Sorties" }</dt>
                        <dd class="small">{ format!("{} objectif · {} stop · {} manuelle", stats.by_reason.target, stats.by_reason.stop, stats.by_reason.manual) }</dd>
                    </div>
                </dl>
                <h3 class="paper-h3">{ "Résultats par décision affichée à l'achat" }</h3>
                <table class="paper-table verdicts">
                    <caption class="sr-only">{ "Résultats par décision affichée à l'achat" }</caption>
                    <thead><tr><th scope="col">{ "Décision" }</th><th scope="col">{ "Trades" }</th><th scope="col">{ "% gagnants" }</th><th scope="col">{ "Résultat moyen" }</th></tr></thead>
                    <tbody>
                        { for rows.iter().map(|r| html! {
                            <tr key={r.verdict.clone()}>
                                <th scope="row" data-label="Décision">{ r.label.clone() }</th>
                                <td data-label="Trades" class="mono">{ r.trades }</td>
                                <td data-label="% gagnants" class="mono">{ pct(r.win_rate, 0) }</td>
                                <td data-label="Résultat moyen" class={classes!("mono", up_down(Some(r.avg_pnl_pct)))}>{ signed_pct(r.avg_pnl_pct, 2) }</td>
                            </tr>
                        }) }
                    </tbody>
                </table>
            }
            if stats.trades < FEW_TRADES {
                <p class="notice warn small">{ format!("⚠ {} trade{s} clôturé{s} seulement. {FEW_TRADES_NOTE}", stats.trades) }</p>
            } else {
                <p class="muted small">{ "Résultats simulés du passé : ils ne garantissent rien pour la suite." }</p>
            }
        </div>
    }
}

#[component]
fn Rules() -> Html {
    html! {
        <details class="card paper-rules">
            <summary>{ "Comment c'est calculé" }</summary>
            <ul class="dec-list small">
                <li>{ "Chaque achat et chaque vente paie " }<b>{ "0,1 % de frais" }</b>{ " et " }<b>{ "0,05 % de glissement" }</b>{ " (le prix obtenu est un peu moins bon que le cours affiché), comme l'historique du signal." }</li>
                <li>
                    { "Stop et objectif sont vérifiés sur les " }<b>{ "bougies journalières" }</b>
                    { " (à l'ouverture de cet écran puis toutes les 5 min). La bougie du jour de l'achat n'est pas utilisée : un niveau touché ce jour-là n'est pris en compte que " }
                    <b>{ "le lendemain" }</b>{ " (son plus bas a pu avoir lieu avant l'achat)." }
                </li>
                <li>{ "Si une même bougie touche le stop " }<b>{ "et" }</b>{ " l'objectif, on compte " }<b>{ "le stop" }</b>{ " : l'ordre des mouvements dans la journée n'est pas connu, on prend le pire cas." }</li>
                <li>{ "Si le cours ouvre déjà sous le stop (trou de cotation), la vente se fait " }<b>{ "au cours d'ouverture" }</b>{ ", plus bas que le stop. Un objectif est vendu à l'objectif, jamais mieux." }</li>
                <li>{ "« Vendre (simulé) » vend toute la position au cours en direct, glissement et frais déduits." }</li>
                <li>{ "Les résultats sont regroupés par décision affichée au moment de l'achat, pour voir quels verdicts ont vraiment marché." }</li>
                <li>{ "Tout reste dans ce navigateur : rien n'est envoyé au serveur, aucun ordre n'est passé. Mêmes règles sur iPhone et Android." }</li>
            </ul>
        </details>
    }
}

#[derive(Properties, PartialEq)]
struct StartCardProps {
    on_start: Callback<f64>,
}

#[component]
fn StartCard(p: &StartCardProps) -> Html {
    let _m = crate::money::use_money();
    let text = use_state(|| "10 000".to_string());
    let oninput = {
        let text = text.clone();
        Callback::from(move |e: InputEvent| text.set(e.target_unchecked_into::<web_sys::HtmlInputElement>().value()))
    };
    let start = {
        let (cb, text) = (p.on_start.clone(), text.clone());
        Callback::from(move |_| cb.emit(start_capital(&text)))
    };
    html! {
        <div class="card empty-card">
            <h2>{ "Testez les décisions d'Altim sans risquer d'argent" }</h2>
            <p class="muted">
                { "Un portefeuille virtuel : vous « achetez » depuis la carte Décision d'un actif, Altim suit le cours, vend au stop ou à l'objectif, et mesure quels verdicts ont vraiment marché." }
            </p>
            <label class="field">
                <span>{ format!("Capital de départ simulé ({})", crate::money::symbol()) }</span>
                <input inputmode="decimal" value={(*text).clone()} {oninput} />
            </label>
            <button class="btn" onclick={start}>{ "Commencer la simulation" }</button>
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct RestartSheetProps {
    on_close: Callback<()>,
    on_confirm: Callback<f64>,
}

/// Focuses the element once shown (`autoFocus`).
#[hook]
pub(crate) fn use_autofocus() -> NodeRef {
    let node = use_node_ref();
    {
        let node = node.clone();
        use_effect_with((), move |_| {
            if let Some(el) = node.cast::<web_sys::HtmlElement>() {
                let _ = el.focus();
            }
        });
    }
    node
}

#[component]
fn RestartSheet(p: &RestartSheetProps) -> Html {
    let m = crate::money::use_money();
    let text = use_state(|| fr(DEFAULT_CAPITAL, 0, 3));
    let focus = use_autofocus();
    let sym = m.symbol();
    let oninput = {
        let text = text.clone();
        Callback::from(move |e: InputEvent| text.set(e.target_unchecked_into::<web_sys::HtmlInputElement>().value()))
    };
    let close = {
        let cb = p.on_close.clone();
        Callback::from(move |_: MouseEvent| cb.emit(()))
    };
    let confirm = {
        let (cb, text) = (p.on_confirm.clone(), text.clone());
        Callback::from(move |_| cb.emit(start_capital(&text)))
    };
    html! {
        <div class="sheet-backdrop" onclick={close.clone()}>
            <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="paper-restart-title" onclick={Callback::from(|e: MouseEvent| e.stop_propagation())}>
                <div class="sheet-handle" />
                <div class="sheet-head"><h2 id="paper-restart-title">{ "Recommencer la simulation" }</h2></div>
                <p class="notice warn small">{ "⚠ Les positions ouvertes, le journal et les statistiques simulés seront effacés. Vos avoirs réels ne sont pas touchés." }</p>
                <label class="field">
                    <span>{ format!("Capital de départ simulé ({sym})") }</span>
                    <input inputmode="decimal" ref={focus} value={(*text).clone()} {oninput} />
                </label>
                <small class="muted">{ format!("Par défaut 10 000 {sym}. Un montant illisible ou nul reprend 10 000 {sym}.") }</small>
                <button class="btn" onclick={confirm}>{ format!("Effacer et recommencer avec {}", usd(&m, m.from_display(start_capital(&text)))) }</button>
                <button class="btn btn-ghost" onclick={close}>{ "Annuler" }</button>
            </div>
        </div>
    }
}
