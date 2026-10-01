//! "Comparer les stratégies" (StrategiesCard.tsx, /api/strategies): textbook strategies with fixed parameters on
//! this asset's daily history. Chips to pick them, one single-axis chart (value of 100 invested) or its list view,
//! then one stacked card of metrics per strategy and how the test avoids flattering itself.
use altim_core::engine::strategies::{StrategiesReport, StrategyId, StrategyResult};
use altim_core::js::{number_to_string as n, to_fixed};
use altim_core::types::Kind;
use altim_core::web::decision::strategies::*;
use wasm_bindgen::JsCast;
use yew::prelude::*;

const BOX: ChartBox = ChartBox { w: 320.0, h: 180.0, l: 34.0, r: 6.0, t: 10.0, b: 18.0 };
const LABELS_W: f64 = 62.0;
/// Direct labels up to 4 coloured curves (plus the reference).
const MAX_LABELED: usize = 4;

fn swatch(id: StrategyId) -> Html {
    html! { <i class={if id == REFERENCE { "strat-swatch ref" } else { "strat-swatch" }} style={format!("color: {};", color(id))} aria-hidden="true" /> }
}

#[derive(Properties, PartialEq)]
pub struct StrategiesCardProps {
    pub symbol: AttrValue,
    pub kind: Kind,
}

#[component]
pub fn StrategiesCard(p: &StrategiesCardProps) -> Html {
    let report = use_state(|| None::<std::rc::Rc<StrategiesReport>>);
    let error = use_state(|| None::<String>);
    let selected = use_state(|| DEFAULT_SELECTION.to_vec());
    let as_list = use_state(|| false);
    {
        let (report, error) = (report.clone(), error.clone());
        use_effect_with((p.symbol.clone(), p.kind), move |(symbol, kind)| {
            let alive = std::rc::Rc::new(std::cell::Cell::new(true));
            report.set(None);
            error.set(None);
            let (a, symbol, kind) = (alive.clone(), symbol.to_string(), *kind);
            wasm_bindgen_futures::spawn_local(async move {
                let r = super::api::strategies(&symbol, kind).await;
                if a.get() {
                    match r {
                        Ok(r) => report.set(Some(std::rc::Rc::new(r))),
                        Err(e) => error.set(Some(e.0)),
                    }
                }
            });
            move || alive.set(false)
        });
    }
    let toggle = |id: StrategyId| {
        let selected = selected.clone();
        Callback::from(move |_: MouseEvent| {
            let s = (*selected).clone();
            selected.set(if s.contains(&id) {
                s.into_iter().filter(|x| *x != id).collect()
            } else {
                ORDER.iter().copied().filter(|x| *x == id || s.contains(x)).collect()
            });
        })
    };
    html! {
        <div class="card strat-card">
            <h2 class="card-title">{ "Comparer les stratégies" }</h2>
            <p class="muted small">{ format!("Comment des stratégies classiques, avec leurs réglages de manuel, se seraient comportées sur l'historique de {}.", p.symbol) }</p>
            if let Some(e) = &*error {
                <p class="notice warn">{ format!("⚠ {e}") }</p>
            }
            if report.is_none() && error.is_none() {
                <div class="skeleton" aria-label="Chargement de la comparaison" />
            }
            if let Some(r) = &*report {
                { {
                    let series: Vec<StrategyResult> = drawable(r, &selected).into_iter().cloned().collect();
                    let chosen: Vec<&StrategyResult> = ORDER.iter().filter(|id| selected.contains(id)).filter_map(|id| r.strategies.iter().find(|s| s.id == *id)).collect();
                    let flip = { let as_list = as_list.clone(); Callback::from(move |_| as_list.set(!*as_list)) };
                    html! {
                        <>
                            <div class="strat-chips" role="group" aria-label="Stratégies à comparer">
                                { for ORDER.iter().filter_map(|id| r.strategies.iter().find(|x| x.id == *id)).map(|s| {
                                    let on = selected.contains(&s.id);
                                    html! {
                                        <button key={key(s.id)} type="button" class={if on { "chip pick on" } else { "chip pick" }} aria-pressed={on.to_string()} aria-label={s.name.clone()} title={s.name.clone()} onclick={toggle(s.id)} style={on.then(|| format!("border-color: {};", color(s.id)))}>
                                            { swatch(s.id) }
                                            { short(s.id) }
                                        </button>
                                    }
                                }) }
                            </div>
                            <p class="small strat-period">
                                <b>{ "Période testée" }</b>{ format!(" : {} · source {}", r.period, r.source) }
                            </p>
                            if series.is_empty() {
                                <p class="muted small">{ "Choisissez au moins une stratégie disponible." }</p>
                            } else if *as_list {
                                { list_view(&series) }
                            } else {
                                <Chart series={series.clone()} />
                            }
                            if !series.is_empty() {
                                <button type="button" class="link-btn" onclick={flip}>{ if *as_list { "Voir le graphique" } else { "Voir en liste" } }</button>
                            }
                            <div class="strat-list">
                                { for chosen.iter().map(|s| strategy_block(s)) }
                            </div>
                            <p class="small"><b>{ "Comment ce test évite de se flatter" }</b></p>
                            <ul class="reasons">{ for r.notes.iter().map(|x| html! { <li key={x.clone()}>{ x.clone() }</li> }) }</ul>
                        </>
                    }
                } }
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct ChartProps {
    pub series: Vec<StrategyResult>,
}

#[component]
pub fn Chart(p: &ChartProps) -> Html {
    let hover = use_state(|| None::<usize>);
    let series: Vec<&StrategyResult> = p.series.iter().collect();
    let coloured = series.iter().filter(|s| s.id != REFERENCE).count();
    let labeled = coloured <= MAX_LABELED;
    let bx = ChartBox { r: if labeled { LABELS_W } else { BOX.r }, ..BOX };
    let Some(g) = geometry(&series, bx) else { return html! {} };
    let path = |s: &StrategyResult| {
        s.equity
            .iter()
            .enumerate()
            .map(|(i, pt)| format!("{}{},{}", if i > 0 { "L" } else { "M" }, to_fixed(g.x(i), 1), to_fixed(g.y(pt.1), 1)))
            .collect::<String>()
    };
    let at = hover.unwrap_or(g.n - 1);
    let value_at = |s: &StrategyResult| s.equity[at.min(s.equity.len() - 1)];
    let time = value_at(series[0]).0;
    let ends: Vec<f64> = series.iter().map(|s| g.y(s.equity[s.equity.len() - 1].1)).collect();
    let label_y = if labeled { place_labels(&ends, 11.0, bx.t + 4.0, bx.h - bx.b) } else { Vec::new() };
    let pick = {
        let hover = hover.clone();
        Callback::from(move |e: PointerEvent| {
            let Some(el) = e.current_target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) else { return };
            let r = el.get_bounding_client_rect();
            if r.width() > 0.0 {
                hover.set(Some(index_at((e.client_x() as f64 - r.left()) / r.width() * bx.w, &g, bx)));
            }
        })
    };
    let leave = {
        let hover = hover.clone();
        Callback::from(move |_: PointerEvent| hover.set(None))
    };
    let summary = series.iter().map(|s| format!("{} {}", s.name, value100(s.equity[s.equity.len() - 1].1))).collect::<Vec<_>>().join(", ");
    let first = &series[0].equity;
    html! {
        <div class="history-chart strat-chart">
            <p class="history-read small" aria-live="polite">
                <b>{ short_date(time) }</b>{ format!(" · valeur de 100 investis{NNBSP}:") }
                { for series.iter().map(|s| html! {
                    <span key={key(s.id)} class="strat-read">{ " " }{ swatch(s.id) }{ format!("{} ", short(s.id)) }<b>{ value100(value_at(s).1) }</b></span>
                }) }
            </p>
            <svg viewBox={format!("0 0 {} {}", n(bx.w), n(bx.h))} role="img" aria-label={format!("Valeur de 100 investis à la fin de la période : {summary}")}
                onpointermove={pick.clone()} onpointerdown={pick} onpointerleave={leave}>
                <line x1={n(bx.l)} x2={n(bx.w - bx.r)} y1={n(g.y(100.0))} y2={n(g.y(100.0))} class="zero" />
                <text x={n(bx.l - 4.0)} y={n(g.y(100.0) + 3.0)} class="axis" text-anchor="end">{ "100" }</text>
                <text x={n(bx.l - 4.0)} y={n(bx.t + 6.0)} class="axis" text-anchor="end">{ value100(g.hi) }</text>
                <text x={n(bx.l - 4.0)} y={n(bx.h - bx.b)} class="axis" text-anchor="end">{ value100(g.lo) }</text>
                <text x={n(bx.l)} y={n(bx.h - 4.0)} class="axis">{ short_date(first[0].0) }</text>
                <text x={n(bx.w - bx.r)} y={n(bx.h - 4.0)} class="axis" text-anchor="end">{ short_date(first[first.len() - 1].0) }</text>
                { for series.iter().map(|s| html! {
                    <path key={key(s.id)} d={path(s)} stroke={color(s.id)} class={if s.id == REFERENCE { "bench strat-ref" } else { "mine" }} />
                }) }
                if labeled {
                    { for series.iter().zip(label_y.iter()).map(|(s, y)| html! {
                        <g key={key(s.id)}>
                            <line x1={n(bx.w - bx.r + 3.0)} x2={n(bx.w - bx.r + 9.0)} y1={n(*y)} y2={n(*y)} stroke={color(s.id)} class={if s.id == REFERENCE { "strat-tick ref" } else { "strat-tick" }} />
                            <text x={n(bx.w - bx.r + 12.0)} y={n(y + 3.0)} class="strat-label">{ short(s.id) }</text>
                        </g>
                    }) }
                }
                if hover.is_some() {
                    <line x1={n(g.x(at))} x2={n(g.x(at))} y1={n(bx.t)} y2={n(bx.h - bx.b)} class="cross" />
                    { for series.iter().map(|s| html! {
                        <circle key={key(s.id)} cx={n(g.x(at))} cy={n(g.y(value_at(s).1))} r="4" fill={color(s.id)} class="dot" />
                    }) }
                }
            </svg>
            <ul class="history-legend">
                { for series.iter().map(|s| html! {
                    <li key={key(s.id)}>{ swatch(s.id) }{ format!("{} ", s.name) }<b>{ signed_pct(s.metrics.as_ref().map(|m| m.total_return), 1) }</b></li>
                }) }
            </ul>
            if !labeled {
                <p class="muted small">{ format!("Plus de {MAX_LABELED} courbes : touchez le graphique pour lire les valeurs, ou passez en liste.") }</p>
            }
        </div>
    }
}

/// Stacked rows: the value of 100 invested at five dates of the period, per strategy.
fn list_view(series: &[StrategyResult]) -> Html {
    html! {
        <ul class="strat-rows">
            { for series.iter().map(|s| html! {
                <li key={key(s.id)}>
                    <p class="small">{ swatch(s.id) }<b>{ s.name.clone() }</b>{ format!(" · {}", signed_pct(s.metrics.as_ref().map(|m| m.total_return), 1)) }</p>
                    <ul class="strat-steps small">
                        { for checkpoints(s, 5).into_iter().map(|(t, v)| html! {
                            <li key={t.to_string()}><span class="muted">{ short_date(t) }</span>{ " " }<b>{ value100(v) }</b></li>
                        }) }
                    </ul>
                </li>
            }) }
        </ul>
    }
}

fn kv(label: &str, value: String, tone: Option<&'static str>) -> Html {
    html! { <p class="kv small"><span>{ label.to_string() }</span><b class={tone}>{ value }</b></p> }
}

fn tone(v: Option<f64>) -> Option<&'static str> {
    v.map(|v| if v >= 0.0 { "up" } else { "down" })
}

fn strategy_block(s: &StrategyResult) -> Html {
    let dca = s.id == StrategyId::Dca;
    let low = low_sample_text(s);
    html! {
        <section key={key(s.id)} class="strat-block" aria-label={s.name.clone()}>
            <p class="strat-head">{ swatch(s.id) }<b>{ s.name.clone() }</b>if let Some(l) = low { <span class="chip muted">{ l }</span> }</p>
            <p class="small">{ s.rule.clone() }</p>
            <p class="muted small">{ format!("Paramètres fixes : {}", s.params) }</p>
            { match (&s.metrics, s.available) {
                (Some(m), true) => html! {
                    <>
                        <div class="strat-metrics">
                            { kv(if dca { "Gain sur les sommes versées" } else { "Rendement total" }, signed_pct(Some(m.total_return), 1), tone(Some(m.total_return))) }
                            { kv(if dca { "Rendement annuel (TRI)" } else { "Rendement annuel" }, m.cagr.map(|c| signed_pct(Some(c), 1)).unwrap_or_else(|| "— (< 1 an)".into()), tone(m.cagr)) }
                            { kv("Pire recul", signed_pct(Some(m.max_drawdown), 1), Some("down")) }
                            { kv("Sharpe / Sortino", if dca { "non pertinent".into() } else { format!("{} / {}", ratio(m.sharpe), ratio(m.sortino)) }, None) }
                            { kv(if dca { "Achats" } else { "Trades" }, m.trades.to_string(), None) }
                            { kv("Temps investi", plain_pct(Some(m.exposure), 0), None) }
                            if !dca {
                                { kv("Taux de réussite", plain_pct(m.win_rate, 0), None) }
                                { kv("Profit factor", ratio(m.profit_factor), None) }
                                { kv("Espérance par trade", signed_pct(m.expectancy, 2), tone(m.expectancy)) }
                            }
                            if let Some(r) = m.avg_r {
                                { kv("Multiple de R moyen", format!("{} R", ratio(Some(r))), None) }
                            }
                        </div>
                        if !s.regimes.is_empty() {
                            <ul class="strat-regimes small">
                                { for s.regimes.iter().map(|g| html! {
                                    <li key={altim_core::engine::backtest::regime_label(g.regime)}>
                                        <span>{ g.label.split(" (").next().unwrap_or("").to_string() }</span>
                                        <b>
                                            { format!("{} {}{}", g.trades, if dca { "achat" } else { "trade" }, if g.trades > 1 { "s" } else { "" }) }
                                            if g.trades > 0 {
                                                { format!(" · moy. {}", signed_pct(g.avg_return, 1)) }
                                            }
                                        </b>
                                        if g.trades > 0 && g.low_sample {
                                            <span class="chip muted">{ "échantillon trop faible" }</span>
                                        }
                                    </li>
                                }) }
                            </ul>
                        }
                    </>
                },
                _ => html! { <p class="notice small">{ s.unavailable.clone().unwrap_or_else(|| "Non couvert.".into()) }</p> },
            } }
            if let Some(note) = s.note.as_ref().filter(|x| !x.is_empty()) {
                <p class="muted small">{ note.clone() }</p>
            }
        </section>
    }
}
